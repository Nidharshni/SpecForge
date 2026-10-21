//! `specforge_ops::authoring::migrate::run` through the runtime the extensions are served
//! by: the hooks they declare run in it, in dependency order, and a failing
//! hook or a changed graph structure restores the migrated files.

use specforge_project::testing::compile_at;
use std::path::Path;
use std::sync::Arc;

use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_ops::authoring::migrate::{
    Failure, MigrationInput, Outcome, Request, invoke_hooks, parse_target, rollback,
};
use specforge_ops::{OpError, OpErrorKind};

use crate::migrate_judged::{judge_planned, migrated};
pub(crate) use crate::writes::files_under;
use specforge_migrate::structural_differences;
use specforge_parser::CURRENT_FORMAT_VERSION;
use specforge_protocol_types::{ExtensionDeclaration, PeerDependency, SandboxPolicy};
use specforge_registry::build_registries;
use specforge_test_macros::test as specforge_test;
use specforge_wasm::runtime::{WasmCallResult, WasmTrapInfo};
use specforge_wasm::testing::InProcessRuntime;

/// A migration that is not refused: the tests that expect a refusal call
/// `specforge_ops::authoring::migrate::run`.
pub(crate) fn run(request: &Request, runtime: Option<specforge_project::SharedRuntime>) -> Outcome {
    specforge_ops::authoring::migrate::run(request, runtime).unwrap()
}

pub(crate) const OLD: &str = "// specforge-format: 0.9\nbehavior alpha \"Alpha\" {\n}\n";

pub(crate) fn project() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": []}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("old.spec"), OLD).unwrap();
    dir
}

pub(crate) fn request(root: &specforge_common::ProjectRoot) -> Request<'_> {
    Request {
        root,
        target: CURRENT_FORMAT_VERSION,
        dry_run: false,
        no_backup: false,
        caller: specforge_ops::testing::caller(),
    }
}

/// A project enabling `@acme/x`, with one file at the old format version.
pub(crate) fn project_with_extension() -> tempfile::TempDir {
    let dir = project();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": ["@acme/x"]}"#,
    )
    .unwrap();
    specforge_installed::testing::install(dir.path(), &["@acme/x"]);
    dir
}

/// `@acme/x`, served in process, declaring its migration hook
/// `migrate_acme` with `handler`; the runtime records every export the host
/// calls.
pub(crate) fn hooked(
    handler: impl Fn(&MigrationInput) -> Result<(), String> + Clone + Send + Sync + 'static,
) -> InProcessRuntime {
    InProcessRuntime::new().with(move || {
        let handler = handler.clone();
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
        c.migration_hook_handler("migrate_acme", move |input| handler(input));
        c
    })
}

/// `@acme/x` declaring no hook at all.
fn unhooked() -> InProcessRuntime {
    InProcessRuntime::new()
        .with(|| ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0")))
}

fn exports_called(runtime: &InProcessRuntime) -> Vec<String> {
    runtime.calls().into_iter().map(|c| c.export).collect()
}

/// Copy `from` (a spec root) into `to`, prefixing each `.spec` without a header with an old one; the number of files
/// prefixed.
fn copy_at_old_format(from: &Path, to: &Path) -> usize {
    let mut prefixed = 0;
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            prefixed += copy_at_old_format(&path, &target);
        } else if path.extension().is_some_and(|e| e == "spec") {
            let text = std::fs::read_to_string(&path).unwrap();
            let old = if text.starts_with("// specforge-format:") {
                text
            } else {
                prefixed += 1;
                format!("// specforge-format: 0.9\n{text}")
            };
            std::fs::write(target, old).unwrap();
        }
    }
    prefixed
}

#[specforge_test(
    invariant = "migration_semantic_preservation",
    verify = "migration that only changes formatting preserves graph structure"
)]
fn a_header_only_migration_of_the_dogfood_spec_keeps_its_graph() {
    let repo = specforge_repo_root::repo_root();
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::copy(
        repo.join("specforge.json"),
        dir.path().join("specforge.json"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("spec")).unwrap();
    let files = copy_at_old_format(&repo.join("spec"), &dir.path().join("spec"));
    assert!(files > 100, "the corpus is the dogfood spec: {files}");
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    let runtime = specforge_project::testing::component_runtime();

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime),
    );

    assert!(
        outcome.verdict.compared && outcome.migrated(),
        "{outcome:?}"
    );
    assert_eq!(outcome.summary.migrated_count, files);
    assert!(
        outcome.structural_differences().is_empty(),
        "{:?}",
        outcome.structural_differences()
    );
    assert!(
        outcome.verdict.post_errors().next().is_none(),
        "{:?}",
        outcome.verdict.post_errors().collect::<Vec<_>>()
    );
}

#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "a planned text that changes an existing title or a kind is reported and refused"
)]
fn a_planned_title_or_kind_change_is_refused() {
    for (from, to, aspect) in [
        ("\"Alpha\"", "\"Renamed\"", "title"),
        ("behavior alpha", "feature alpha", "kind"),
    ] {
        let dir = project_with_extension();

        let verdict = judge_planned(dir.path(), None, migrated().replace(from, to));

        assert!(verdict.compared, "{aspect}: {verdict:?}");
        let differences = structural_differences(&verdict.graph_delta);
        let messages: Vec<&str> = differences.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(
            messages,
            [format!("entity 'alpha' changed during migration: {aspect}")],
            "{aspect}"
        );
        assert!(
            matches!(
                verdict.refusal(),
                Some(Failure::ChangedGraph { differences: 1 })
            ),
            "{aspect}: {verdict:?}"
        );
    }
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "a migration whose extension hook fails is refused and nothing is written"
)]
fn a_migration_whose_hook_fails_is_refused() {
    let dir = project_with_extension();
    let runtime = Arc::new(hooked(|_| Err("the data cannot be migrated".into())));

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert!(
        !outcome.verdict.compared,
        "no validation after a failed hook"
    );
    assert_eq!(
        outcome.verdict.hook_failures.len(),
        1,
        "{:?}",
        outcome.verdict.hook_failures
    );
    assert!(
        outcome.verdict.hook_failures[0].contains("migrate_acme() of '@acme/x' trapped")
            && outcome.verdict.hook_failures[0].contains("the data cannot be migrated"),
        "{:?}",
        outcome.verdict.hook_failures
    );
    assert!(
        matches!(outcome.failure(), Some(Failure::HookFailed { .. })),
        "{outcome:?}"
    );
    assert!(outcome.writes.is_empty(), "{:?}", outcome.writes);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("old.spec")).unwrap(),
        OLD
    );
}

#[specforge_test(
    behavior = "provide_mcp_migrate_tool",
    verify = "a failed migration's error names its cause: a hook, a changed graph, a project that does not compile, or files that could not be migrated"
)]
fn a_failed_migration_names_its_cause() {
    let root = |dir: &tempfile::TempDir| specforge_common::ProjectRoot::of(dir.path()).unwrap();

    // Applied: nothing failed, no failure.
    let dir = project();
    let outcome = run(&request(&root(&dir)), None);
    assert!(outcome.ok());
    assert_eq!(outcome.failure(), None);

    // A file that cannot migrate: it is not what it should hold.
    let dir = project();
    std::fs::write(
        dir.path().join("bad.spec"),
        "// specforge-format: 99.0\nbehavior bad \"Bad\" {\n}\n",
    )
    .unwrap();
    let outcome = run(&request(&root(&dir)), None);
    let failure = outcome.failure().expect("a file failed to migrate");
    assert!(!outcome.ok());
    assert!(
        matches!(&failure, Failure::FilesFailed { files } if files.len() == 1 && files[0].0.ends_with("bad.spec")),
        "{failure:?}"
    );
    let error = OpError::from(failure);
    assert_eq!(
        (error.kind, error.code.as_ref()),
        (OpErrorKind::SchemaMismatch, "migration_file_failed")
    );
    assert!(
        error
            .message
            .starts_with("1 file(s) could not be migrated: ")
            && error.message.contains("bad.spec"),
        "{}",
        error.message
    );

    // A hook that fails: refused.
    let dir = project_with_extension();
    let runtime = Arc::new(hooked(|_| Err("trapped".into())));
    let outcome = run(&request(&root(&dir)), Some(runtime));
    assert!(!outcome.ok());
    let failure = outcome.failure().expect("refused");
    assert!(matches!(failure, Failure::HookFailed { .. }), "{failure:?}");
    let error = OpError::from(failure);
    assert_eq!(
        (error.kind, error.code.as_ref()),
        (OpErrorKind::Internal, "migration_hook_failed")
    );
    assert!(error.message.contains("trapped"), "{}", error.message);

    // A planned text that leaves the project not compiling: refused with the errors the migrated project reported.
    let dir = project_with_extension();
    let verdict = judge_planned(dir.path(), None, "behavior {\n");
    assert!(verdict.post_errors().next().is_some(), "{verdict:?}");
    let failure = verdict.refusal().expect("refused");
    assert!(
        matches!(failure, Failure::DoesNotCompile { errors } if errors > 0),
        "{failure:?}"
    );
    let error = OpError::from(failure);
    assert_eq!(
        (error.kind, error.code.as_ref()),
        (OpErrorKind::CompilationFailed, "migration_does_not_compile")
    );

    // A planned text that adds an entity: the graph changed (W054), refused.
    let dir = project_with_extension();
    let verdict = judge_planned(
        dir.path(),
        None,
        format!("{}behavior added \"Added\" {{\n}}\n", migrated()),
    );
    assert!(
        !structural_differences(&verdict.graph_delta).is_empty(),
        "{verdict:?}"
    );
    assert!(verdict.post_errors().next().is_none());
    let failure = verdict.refusal().expect("refused");
    assert!(
        matches!(failure, Failure::ChangedGraph { differences } if differences > 0),
        "{failure:?}"
    );
    let error = OpError::from(failure);
    assert_eq!(
        (error.kind, error.code.as_ref()),
        (OpErrorKind::CompilationFailed, "migration_changed_graph")
    );
    assert!(error.message.contains("W054"), "{}", error.message);
}

#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "post-migration check runs automatically"
)]
fn a_header_only_migration_is_checked_after_it_runs() {
    let dir = project();

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        None,
    );

    assert!(outcome.migrated(), "{outcome:?}");
    assert!(
        outcome.verdict.compared,
        "the migrated project is compiled and compared"
    );
    assert!(outcome.structural_differences().is_empty());
    assert_eq!(outcome.from.to_string(), "0.9");
}

#[test]
fn nothing_pending_runs_nothing() {
    let dir = project_with_extension();
    std::fs::write(dir.path().join("old.spec"), "behavior alpha \"A\" {\n}\n").unwrap();
    let runtime = Arc::new(hooked(|_| Ok(())));

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert!(!outcome.pending && !outcome.applied && !outcome.verdict.compared);
    assert!(
        runtime.calls().is_empty(),
        "no compile and no hook with nothing to migrate: {:?}",
        exports_called(&runtime)
    );
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a problem in specforge.json the migration goes around is an E069 in its document and fails it"
)]
fn a_problem_the_migration_goes_around_fails_it_after_it_ran() {
    let dir = project();
    std::fs::write(dir.path().join("specforge.json"), r#"{"exclude": ["["]}"#).unwrap();

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        None,
    );

    // The migration ran and is kept.
    assert!(
        std::fs::read_to_string(dir.path().join("old.spec"))
            .unwrap()
            .starts_with("// specforge-format: 1.0"),
    );
    assert!(dir.path().join("old.spec.bak").exists());
    assert!(dir.path().join(".specforge/migration.json").exists());
    // And it fails, with the problem the compile reports.
    assert_eq!(outcome.diagnostics.len(), 1, "{:?}", outcome.diagnostics);
    assert!(outcome.diagnostics[0].is(specforge_common::codes::E069));
    assert!(!outcome.ok());
    // A problem the run went around fails its verdict, not the run (ADR 0170).
    assert_eq!(outcome.failure(), None);
    assert!(outcome.document().diagnostics.0.len() == 1);
}

#[test]
fn a_target_newer_than_supported_or_malformed_is_e019() {
    assert_eq!(parse_target(Some("99.0")).unwrap_err().error.code, "E019");
    assert_eq!(parse_target(Some("latest")).unwrap_err().error.code, "E019");
    assert_eq!(parse_target(None).unwrap(), CURRENT_FORMAT_VERSION);
}

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "a rollback reports the files it restored as written"
)]
fn a_rollback_reports_the_files_it_restored_as_written() {
    let dir = project();
    let root = dir.path();
    let migrated = run(
        &request(&specforge_common::ProjectRoot::of(root).unwrap()),
        None,
    );
    assert!(migrated.ok(), "{migrated:?}");
    assert_ne!(std::fs::read_to_string(root.join("old.spec")).unwrap(), OLD);

    let outcome = rollback(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        specforge_ops::testing::caller(),
    )
    .unwrap();

    assert!(outcome.ok(), "{outcome:?}");
    assert_eq!(std::fs::read_to_string(root.join("old.spec")).unwrap(), OLD);
    let written: Vec<&Path> = outcome.writes.paths().collect();
    assert_eq!(written.len(), 2, "{written:?}");
    assert!(
        written.iter().any(|p| p.ends_with("old.spec")),
        "{written:?}"
    );
    assert!(
        written
            .iter()
            .any(|p| p.ends_with(".specforge/migration.json")),
        "the record, removed, is a write: {written:?}"
    );
    assert!(!root.join(".specforge/migration.json").exists());
    assert!(
        root.join("old.spec.bak").exists(),
        "the backup is preserved for the user"
    );
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "extension with migration_hook field has it invoked during migrate"
)]
fn the_hook_an_extension_declares_in_its_handshake_runs_on_migrate() {
    let dir = project_with_extension();
    let runtime = Arc::new(hooked(|_| Ok(())));

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert!(outcome.migrated(), "{outcome:?}");
    assert_eq!(outcome.verdict.hooks_invoked, ["@acme/x:migrate_acme"]);
    assert!(outcome.verdict.hook_failures.is_empty(), "{outcome:?}");
    assert!(exports_called(&runtime).contains(&"migrate_acme".to_string()));
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "extension without migration_hook field is skipped silently"
)]
fn an_extension_whose_handshake_names_no_hook_is_skipped_silently() {
    let dir = project_with_extension();
    let runtime = Arc::new(unhooked());

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert!(outcome.migrated(), "{outcome:?}");
    assert!(outcome.verdict.hooks_invoked.is_empty() && outcome.verdict.hook_failures.is_empty());
    assert!(
        exports_called(&runtime).iter().all(|c| c.starts_with("__")),
        "only the protocol exports are called"
    );
    assert!(
        !outcome
            .verdict
            .post_diagnostics
            .iter()
            .any(|d| d.message.contains("hook")),
        "{outcome:?}"
    );
}

/// Two extensions with hooks, `@acme/a`'s panicking.
fn hooks() -> InProcessRuntime {
    InProcessRuntime::new()
        .with(|| {
            let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/a", "1.0.0"));
            c.migration_hook_handler("migrate_a", |_| panic!("hook panicked"));
            c
        })
        .with(|| {
            let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/b", "1.0.0"));
            c.migration_hook_handler("migrate_b", |_| Ok(()));
            c
        })
}

fn manifest(name: &str, hook: &str) -> ExtensionDeclaration {
    ExtensionDeclaration {
        handshake: specforge_protocol_types::HandshakeResponse {
            name: name.into(),
            version: "1.0.0".into(),
            migration_hook: Some(hook.into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "a migration hook receives the from and to format versions and the migrated files"
)]
fn a_hook_receives_the_versions_and_the_migrated_files() {
    use std::sync::{Arc, Mutex};
    // The hook's handler records what it decoded.
    let seen: Arc<Mutex<Vec<MigrationInput>>> = Arc::default();
    let recorder = Arc::clone(&seen);
    let runtime = Arc::new(InProcessRuntime::new().with(move || {
        let recorder = Arc::clone(&recorder);
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/a", "1.0.0"));
        c.migration_hook_handler("migrate_a", move |input| {
            recorder.lock().unwrap().push(input.clone());
            Ok(())
        });
        c
    }));
    let input = MigrationInput {
        from: "0.9".into(),
        to: "1.0".into(),
        files: vec!["old.spec".into()],
    };

    invoke_hooks(
        &build_registries(vec![manifest("@acme/a", "migrate_a")]),
        runtime.as_ref(),
        &input,
    );

    assert_eq!(*seen.lock().unwrap(), [input]);
    assert_eq!(
        runtime.calls()[0].input,
        serde_json::json!({"from": "0.9", "to": "1.0", "files": ["old.spec"]})
    );
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "the nine builtin extensions' hooks run in dependency order with no failure"
)]
fn the_builtin_extensions_hooks_run_without_a_dependency_failure() {
    // Every builtin loaded together, as a project enabling all of them
    // does: the peer graph (optional peers included) must sort, or the
    // hooks never run and a migration rolls back.
    let names: Vec<&str> = specforge_component::builtins::BUILTIN_EXTENSIONS
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(names.len(), 9);
    let runtime = Arc::new(specforge_component::ComponentRuntime::new());
    specforge_component::builtins::load_builtins(runtime.as_ref()).unwrap();
    let declarations: Vec<ExtensionDeclaration> = names
        .iter()
        .map(|name| {
            specforge_wasm::protocol::load_declaration(runtime.as_ref(), name)
                .unwrap()
                .declaration
        })
        .collect();
    let input = MigrationInput {
        from: "0.9".into(),
        to: "1.0".into(),
        files: Vec::new(),
    };

    let build = build_registries(declarations);

    let (_, failures) = invoke_hooks(&build, runtime.as_ref(), &input);

    assert!(failures.is_empty(), "{failures:?}");
    assert!(
        build.diagnostics().all(|d| d.code != "E027"),
        "{:?}",
        build.diagnostics().collect::<Vec<_>>()
    );
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "hook that traps collects WasmTrapInfo and continues"
)]
fn a_trapping_hook_is_recorded_and_the_next_one_still_runs() {
    let runtime = Arc::new(hooks());
    let manifests = build_registries(vec![
        manifest("@acme/a", "migrate_a"),
        manifest("@acme/b", "migrate_b"),
    ]);

    let input = MigrationInput {
        from: "0.9".into(),
        to: "1.0".into(),
        files: Vec::new(),
    };
    let (invoked, failures) = invoke_hooks(&manifests, runtime.as_ref(), &input);

    assert_eq!(invoked, ["@acme/b:migrate_b"]);
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(
        failures[0].contains("unreachable") && failures[0].contains("hook panicked"),
        "{failures:?}"
    );
    let called: Vec<String> = runtime
        .calls()
        .into_iter()
        .map(|c| format!("{}:{}", c.extension, c.export))
        .collect();
    assert_eq!(called, ["@acme/a:migrate_a", "@acme/b:migrate_b"]);
}

/// `name`, declaring the migration hook `hook` answered by `handler`, and
/// peer-depending on `peers`.
fn extension(
    name: &'static str,
    hook: &'static str,
    peers: &'static [&'static str],
    handler: fn(&MigrationInput) -> Result<(), String>,
) -> impl Fn() -> ContributionsBuilder + Send + Sync + 'static {
    move || {
        let mut meta = ExtensionMeta::new(name, "1.0.0");
        meta.peer_dependencies = peers
            .iter()
            .map(|peer| PeerDependency {
                name: (*peer).into(),
                version: "^1".into(),
                optional: false,
            })
            .collect();
        let mut c = ContributionsBuilder::new(meta);
        c.migration_hook_handler(hook, handler);
        c
    }
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "hooks invoked in deterministic extension load order"
)]
fn a_required_peer_cycle_does_not_stop_the_hooks() {
    let dir = project_enabling(&["@acme/a", "@acme/b"]);
    let runtime = Arc::new(
        InProcessRuntime::new()
            .with(extension("@acme/a", "migrate_a", &["@acme/b"], |_| Ok(())))
            .with(extension("@acme/b", "migrate_b", &["@acme/a"], |_| Ok(()))),
    );

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert!(
        outcome.verdict.hook_failures.is_empty(),
        "{:?}",
        outcome.verdict.hook_failures
    );
    assert_eq!(
        outcome.verdict.hooks_invoked,
        ["@acme/a:migrate_a", "@acme/b:migrate_b"]
    );
    assert!(outcome.ok(), "{outcome:?}");
    // The cycle is the compile's E027, among the post-migration diagnostics.
    assert!(
        outcome
            .verdict
            .post_diagnostics
            .iter()
            .any(|d| d.code == "E027" && d.message.contains("cycle detected in peer dependencies")),
        "{:?}",
        outcome.verdict.post_diagnostics
    );
}

/// A project enabling `extensions`, with one file at the old format version.
fn project_enabling(extensions: &[&str]) -> tempfile::TempDir {
    let dir = project();
    let config = serde_json::json!({"name": "p", "version": "0.1.0", "extensions": extensions});
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    specforge_installed::testing::install(dir.path(), extensions);
    dir
}

/// The hook exports (`migrate_*`) the runtime was called with, in order.
fn hooks_called(runtime: &InProcessRuntime) -> Vec<String> {
    exports_called(runtime)
        .into_iter()
        .filter(|export| export.starts_with("migrate_"))
        .collect()
}

/// How many times the host read `extension`'s handshake: once per compile
/// that loads it.
fn handshakes_of(runtime: &InProcessRuntime, extension: &str) -> usize {
    runtime
        .calls()
        .iter()
        .filter(|call| call.extension == extension && call.export == "__handshake")
        .count()
}

#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "new diagnostics from migration reported"
)]
fn a_diagnostic_the_migration_introduces_is_reported() {
    let dir = project_with_extension();
    let before = compile_at(dir.path(), None);
    assert!(
        !before.diagnostics().iter().any(|d| d.code == "E003"),
        "{:?}",
        before.diagnostics()
    );

    let verdict = judge_planned(
        dir.path(),
        None,
        format!(
            "{}\nbehavior extra \"Extra\" {{\n  features [ghost]\n}}\n",
            migrated()
        ),
    );

    assert!(
        verdict
            .post_diagnostics
            .iter()
            .any(|d| d.code == "E003" && d.message.contains("ghost")),
        "{:?}",
        verdict.post_diagnostics
    );
    // `extra` appeared: the graph changed structure, so the migration is refused.
    assert!(verdict.refusal().is_some(), "{verdict:?}");
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "extension with empty migration_hook field is skipped silently"
)]
fn an_empty_hook_name_is_skipped_silently() {
    let runtime = Arc::new(hooks());
    let input = MigrationInput {
        from: "0.9".into(),
        to: "1.0".into(),
        files: Vec::new(),
    };

    let run = invoke_hooks(
        &build_registries(vec![manifest("@acme/a", "")]),
        runtime.as_ref(),
        &input,
    );

    assert_eq!(run, (Vec::new(), Vec::new()));
    assert!(runtime.calls().is_empty(), "{:?}", runtime.calls());
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "hook returning error collects diagnostic and continues"
)]
fn a_hook_that_answers_an_error_is_recorded_and_the_next_one_runs() {
    let dir = project_enabling(&["@acme/a", "@acme/b"]);
    let runtime = Arc::new(
        InProcessRuntime::new()
            .with(extension("@acme/a", "migrate_a", &[], |_| {
                Err("bad data".into())
            }))
            .with(extension("@acme/b", "migrate_b", &[], |_| Ok(()))),
    );

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert_eq!(outcome.verdict.hooks_invoked, ["@acme/b:migrate_b"]);
    assert_eq!(
        outcome.verdict.hook_failures.len(),
        1,
        "{:?}",
        outcome.verdict.hook_failures
    );
    assert!(
        outcome.verdict.hook_failures[0].contains("migrate_a() of '@acme/a' trapped")
            && outcome.verdict.hook_failures[0].contains("bad data"),
        "{:?}",
        outcome.verdict.hook_failures
    );
    assert_eq!(hooks_called(&runtime), ["migrate_a", "migrate_b"]);
    assert!(
        outcome.failure().is_some() && !outcome.applied,
        "a failed hook refuses the run"
    );
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "hooks invoked in deterministic extension load order"
)]
fn hooks_run_in_dependency_order_every_time() {
    // `@acme/b` is listed first and peer-depends on `@acme/a`.
    let order = || {
        let dir = project_enabling(&["@acme/b", "@acme/a"]);
        let runtime = Arc::new(
            InProcessRuntime::new()
                .with(extension("@acme/b", "migrate_b", &["@acme/a"], |_| Ok(())))
                .with(extension("@acme/a", "migrate_a", &[], |_| Ok(()))),
        );
        let outcome = run(
            &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
            Some(runtime.clone()),
        );
        (outcome.verdict.hooks_invoked, hooks_called(&runtime))
    };

    let (first_invoked, first_called) = order();
    let (second_invoked, second_called) = order();

    assert_eq!(first_called, ["migrate_a", "migrate_b"]);
    assert_eq!(second_called, first_called);
    assert_eq!(first_invoked, ["@acme/a:migrate_a", "@acme/b:migrate_b"]);
    assert_eq!(second_invoked, first_invoked);
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "extension in failed lifecycle state has hook skipped"
)]
fn an_extension_that_failed_to_load_runs_no_hook() {
    let dir = project_enabling(&["@acme/broken", "@acme/ok"]);
    let runtime = Arc::new(
        InProcessRuntime::new()
            .with(extension("@acme/broken", "migrate_broken", &[], |_| Ok(())))
            .with(extension("@acme/ok", "migrate_ok", &[], |_| Ok(())))
            .answer_raw(
                "@acme/broken",
                "__handshake",
                WasmCallResult::Trap(WasmTrapInfo {
                    kind: "call_failed".into(),
                    message: "the module is broken".into(),
                    export_name: "__handshake".into(),
                }),
            ),
    );

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert_eq!(outcome.verdict.hooks_invoked, ["@acme/ok:migrate_ok"]);
    assert_eq!(hooks_called(&runtime), ["migrate_ok"]);
    assert!(
        outcome.verdict.hook_failures.is_empty(),
        "{:?}",
        outcome.verdict.hook_failures
    );
    assert!(
        outcome
            .verdict
            .post_diagnostics
            .iter()
            .any(|d| d.code == "E028" && d.message.contains("@acme/broken")),
        "{:?}",
        outcome.verdict.post_diagnostics
    );
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "hook exceeding timeout treated as trap"
)]
fn a_hook_over_its_deadline_is_a_trap_and_the_next_one_runs() {
    // The component runtime enforces the deadline (its own tests prove it);
    // here the host's reading of the extension's declared limit and of the
    // trap it answers: a failure of that hook, and the next still runs.
    let dir = project_enabling(&["@acme/slow", "@acme/b"]);
    let runtime = Arc::new(
        InProcessRuntime::new()
            .with(|| {
                let mut meta = ExtensionMeta::new("@acme/slow", "1.0.0");
                meta.sandbox_policy = Some(SandboxPolicy {
                    max_execution_ms: Some(50),
                    ..Default::default()
                });
                let mut c = ContributionsBuilder::new(meta);
                c.migration_hook_handler("migrate_slow", |_| Ok(()));
                c
            })
            .with(extension("@acme/b", "migrate_b", &[], |_| Ok(())))
            .answer_raw(
                "@acme/slow",
                "migrate_slow",
                WasmCallResult::Trap(WasmTrapInfo {
                    kind: "deadline_exceeded".into(),
                    message: "interrupted".into(),
                    export_name: "migrate_slow".into(),
                }),
            ),
    );

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert!(
        runtime
            .limits()
            .iter()
            .any(|(extension, limits)| extension == "@acme/slow" && limits.execution_ms == 50),
        "{:?}",
        runtime.limits()
    );
    assert_eq!(
        outcome.verdict.hook_failures.len(),
        1,
        "{:?}",
        outcome.verdict.hook_failures
    );
    assert!(
        outcome.verdict.hook_failures[0].contains("deadline_exceeded"),
        "{:?}",
        outcome.verdict.hook_failures
    );
    assert_eq!(outcome.verdict.hooks_invoked, ["@acme/b:migrate_b"]);
    assert!(outcome.failure().is_some(), "{outcome:?}");
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "validation runs once after both core and extension hooks complete"
)]
fn the_project_is_checked_once_after_the_files_and_the_hooks() {
    let dir = project_with_extension();
    let file = dir.path().join("old.spec");
    // The hooks run on the plan, before any file is written.
    let runtime = Arc::new(hooked(|input| {
        let text = std::fs::read_to_string(&input.files[0]).map_err(|e| e.to_string())?;
        if text == OLD {
            Ok(())
        } else {
            Err(format!("a file was written before the hooks ran: {text}"))
        }
    }));

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert!(
        outcome.verdict.compared && outcome.migrated(),
        "{outcome:?}"
    );
    assert!(
        outcome.verdict.hook_failures.is_empty(),
        "{:?}",
        outcome.verdict.hook_failures
    );
    assert!(
        std::fs::read_to_string(file)
            .unwrap()
            .starts_with("// specforge-format: 1.0")
    );
    // One environment load: the hooks and the planned texts are judged over the project compiled once.
    assert_eq!(handshakes_of(&runtime, "@acme/x"), 1);
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a refused migration made without backups leaves the record as it was"
)]
fn a_refused_migration_without_backups_keeps_the_record() {
    let dir = project_with_extension();
    let root = dir.path();
    let first = run(
        &request(&specforge_common::ProjectRoot::of(root).unwrap()),
        Some(Arc::new(unhooked())),
    );
    assert!(first.ok(), "{first:?}");
    let record = root.join(".specforge/migration.json");
    let recorded = std::fs::read(&record).unwrap();
    std::fs::write(root.join("second.spec"), OLD).unwrap();
    let project = specforge_common::ProjectRoot::of(root).unwrap();
    let mut req = request(&project);
    req.no_backup = true;

    let outcome = run(&req, Some(Arc::new(hooked(|_| Err("trapped".into())))));

    assert!(!outcome.ok() && outcome.failure().is_some(), "{outcome:?}");
    assert_eq!(std::fs::read(&record).unwrap(), recorded);
    assert_eq!(
        std::fs::read_to_string(root.join("second.spec")).unwrap(),
        OLD
    );
    assert!(!root.join("second.spec.bak").exists());
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a kept migration records its files in .specforge/migration.json"
)]
fn a_kept_migration_records_what_a_rollback_undoes() {
    let dir = project_with_extension();
    let root = dir.path();

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(root).unwrap()),
        Some(Arc::new(unhooked())),
    );

    assert!(outcome.ok(), "{outcome:?}");
    let record = specforge_migrate::MigrationRecord::parse(
        &std::fs::read_to_string(root.join(".specforge/migration.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(record.files.len(), 1, "{record:?}");
    assert_eq!(record.files[0].path, "old.spec");
    assert_eq!(record.files[0].backup, "old.spec.bak");
    let migrated = std::fs::read_to_string(root.join("old.spec")).unwrap();
    assert_eq!(
        record.files[0].sha256,
        specforge_installed::hex_sha256(migrated.as_bytes())
    );
    assert!(
        outcome
            .writes
            .paths()
            .any(|p| p.ends_with(".specforge/migration.json")),
        "{:?}",
        outcome.writes
    );
}

#[test]
fn a_migration_without_backups_removes_the_record() {
    let dir = project_with_extension();
    let root = dir.path();
    run(
        &request(&specforge_common::ProjectRoot::of(root).unwrap()),
        Some(Arc::new(unhooked())),
    );
    assert!(root.join(".specforge/migration.json").exists());
    std::fs::write(root.join("newer.spec"), OLD).unwrap();
    let project = specforge_common::ProjectRoot::of(root).unwrap();
    let mut req = request(&project);
    req.no_backup = true;

    let outcome = run(&req, Some(Arc::new(unhooked())));

    assert!(outcome.ok(), "{outcome:?}");
    assert!(!root.join(".specforge/migration.json").exists());
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn migrate_states_which_field_a_refusal_is_about() {
    use specforge_ops::authoring::migrate::MigrateField;
    for target in ["99.0", "latest"] {
        assert_eq!(
            parse_target(Some(target)).unwrap_err().subject,
            Some(MigrateField::Target),
            "{target}"
        );
    }
}
