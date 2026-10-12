//! `specforge_ops::migrate::run` through the runtime the extensions are served
//! by: the hooks they declare run in it, in dependency order, and a failing
//! hook or a changed graph structure restores the migrated files.

use std::path::Path;
use std::sync::Arc;

use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_migrate::migrate_project;
use specforge_ops::OpErrorKind;
use specforge_ops::migrate::{MigrationInput, Request, invoke_hooks, parse_target, rollback, run};
use specforge_parser::CURRENT_FORMAT_VERSION;
use specforge_protocol_types::{ExtensionDeclaration, FieldType, PeerDependency, SandboxPolicy};
use specforge_registry::build_registries;
use specforge_test_macros::test as specforge_test;
use specforge_wasm::runtime::{WasmCallResult, WasmTrapInfo};
use specforge_wasm::testing::InProcessRuntime;

const OLD: &str = "// specforge-format: 0.9\nbehavior alpha \"Alpha\" {\n}\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": []}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("old.spec"), OLD).unwrap();
    dir
}

fn request(root: &specforge_common::ProjectRoot) -> Request<'_> {
    Request {
        root,
        target: CURRENT_FORMAT_VERSION,
        dry_run: false,
        no_backup: false,
    }
}

/// A project enabling `@acme/x`, with one file at the old format version.
fn project_with_extension() -> tempfile::TempDir {
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
fn hooked(
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

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "a migration whose graph changes structure is rolled back automatically"
)]
fn a_migration_that_changes_the_graph_is_rolled_back() {
    let dir = project_with_extension();
    let file = dir.path().join("old.spec");

    // A hook that renames the entity: the graph loses `alpha`.
    let runtime = Arc::new(hooked(|input| {
        let file = &input.files[0];
        let migrated = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        std::fs::write(file, migrated.replace("alpha", "beta")).map_err(|e| e.to_string())
    }));
    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert_eq!(outcome.hooks_invoked, ["@acme/x:migrate_acme"]);
    assert!(outcome.validated);
    assert!(
        outcome
            .structural_differences
            .iter()
            .any(|d| d.code == "W054" && d.message.contains("alpha")),
        "{:?}",
        outcome.structural_differences
    );
    let rollback = outcome.rollback.as_ref().expect("rolled back");
    assert_eq!(rollback.restored_count, 1, "{rollback:?}");
    assert!(!outcome.ok() && !outcome.migrated());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), OLD);
}

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "a migration whose extension hook fails is rolled back automatically"
)]
fn a_migration_whose_hook_fails_is_rolled_back() {
    let dir = project_with_extension();
    let runtime = Arc::new(hooked(|_| Err("the data cannot be migrated".into())));

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert!(!outcome.validated, "no validation after a failed hook");
    assert_eq!(
        outcome.hook_failures.len(),
        1,
        "{:?}",
        outcome.hook_failures
    );
    assert!(
        outcome.hook_failures[0].contains("migrate_acme() of '@acme/x' trapped")
            && outcome.hook_failures[0].contains("the data cannot be migrated"),
        "{:?}",
        outcome.hook_failures
    );
    assert_eq!(outcome.rollback.as_ref().unwrap().restored_count, 1);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("old.spec")).unwrap(),
        OLD
    );
}

#[test]
fn a_failed_migration_names_its_kind() {
    // Applied: nothing failed, no failure.
    let dir = project();
    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        None,
    );
    assert!(outcome.ok());
    assert_eq!(outcome.failure(), None);

    // A file that cannot migrate: the run failed on its own side.
    let dir = project();
    std::fs::write(
        dir.path().join("bad.spec"),
        "// specforge-format: 99.0\nbehavior bad \"Bad\" {\n}\n",
    )
    .unwrap();
    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        None,
    );
    let failure = outcome.failure().expect("a file failed to migrate");
    assert!(!outcome.ok());
    assert_eq!(
        (
            failure.kind,
            failure.code.as_ref(),
            failure.message.as_str()
        ),
        (
            OpErrorKind::Internal,
            "migration_failed",
            "the migration failed"
        )
    );

    // A hook that fails: rolled back, the project never compiled again.
    let dir = project_with_extension();
    let runtime = Arc::new(hooked(|_| Err("trapped".into())));
    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );
    assert!(outcome.rollback.is_some() && !outcome.ok());
    assert_eq!(outcome.failure().unwrap().kind, OpErrorKind::Internal);

    // A hook that leaves the project not compiling: rolled back with the
    // errors the migrated project reported.
    let dir = project_with_extension();
    let runtime = Arc::new(hooked(|input| {
        std::fs::write(&input.files[0], "behavior {\n").map_err(|e| e.to_string())
    }));
    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );
    assert!(outcome.post_errors().next().is_some(), "{outcome:?}");
    assert!(outcome.rollback.is_some());
    let failure = outcome.failure().expect("rolled back");
    assert_eq!(
        (failure.kind, failure.message.as_str()),
        (
            OpErrorKind::CompilationFailed,
            "the migrated project does not compile"
        )
    );
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
        outcome.validated,
        "the migrated project is compiled and compared"
    );
    assert!(outcome.structural_differences.is_empty());
    assert!(outcome.rollback.is_none());
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

    assert!(!outcome.pending && !outcome.applied && !outcome.validated);
    assert!(
        runtime.calls().is_empty(),
        "no compile and no hook with nothing to migrate: {:?}",
        exports_called(&runtime)
    );
}

#[test]
fn a_target_newer_than_supported_or_malformed_is_e019() {
    assert_eq!(parse_target(Some("99.0")).unwrap_err().code, "E019");
    assert_eq!(parse_target(Some("latest")).unwrap_err().code, "E019");
    assert_eq!(parse_target(None).unwrap(), CURRENT_FORMAT_VERSION);
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "only the project's sources are migrated: under spec_root, without excluded files"
)]
fn migrate_reads_the_project_sources() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"spec_root": "specs", "exclude": ["drafts"]}"#,
    )
    .unwrap();
    for file in [
        "specs/a.spec",
        "specs/drafts/d.spec",
        "spec/old.spec",
        "fixtures/fx.spec",
    ] {
        std::fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
        std::fs::write(root.join(file), OLD).unwrap();
    }

    let summary = migrate_project(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        &CURRENT_FORMAT_VERSION,
        true,
        true,
    );

    let visited: Vec<&str> = summary
        .results
        .iter()
        .map(|r| r.file_path.as_str())
        .collect();
    assert_eq!(visited.len(), 1, "{visited:?}");
    assert!(visited[0].ends_with("specs/a.spec"), "{visited:?}");
    // A dry run records nothing, so there is nothing to roll back.
    let rollback = rollback(&specforge_common::ProjectRoot::of(root).unwrap()).summary;
    assert!(rollback.results.is_empty(), "{rollback:?}");
    assert_eq!(rollback.warnings.len(), 1, "{rollback:?}");
    assert!(
        rollback.warnings[0].contains("nothing to roll back"),
        "{rollback:?}"
    );
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

    let outcome = rollback(&specforge_common::ProjectRoot::of(root).unwrap());

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
    assert_eq!(outcome.hooks_invoked, ["@acme/x:migrate_acme"]);
    assert!(outcome.hook_failures.is_empty(), "{outcome:?}");
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
    assert!(outcome.hooks_invoked.is_empty() && outcome.hook_failures.is_empty());
    assert!(
        exports_called(&runtime).iter().all(|c| c.starts_with("__")),
        "only the protocol exports are called"
    );
    assert!(
        !outcome
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

/// `@acme/base` (a kind `Thing`, a hook that rewrites `specforge.json` to
/// enable only itself) and `@acme/extra`, which peers on it and adds a kind
/// `Gizmo`, an edge `gizmo_links` and an enhancement field `badge` on
/// `Thing`: what a project loses when the hook drops `@acme/extra`.
fn schema_runtime() -> InProcessRuntime {
    InProcessRuntime::new()
        .with(|| {
            let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/base", "1.0.0"));
            c.kind("Thing", |k| {
                k.keyword("thing");
            });
            c.migration_hook_handler("migrate_base", |input| {
                let root = Path::new(&input.files[0]).parent().unwrap().to_path_buf();
                std::fs::write(
                    root.join("specforge.json"),
                    r#"{"name": "p", "version": "0.1.0", "extensions": ["@acme/base"]}"#,
                )
                .map_err(|e| e.to_string())
            });
            c
        })
        .with(|| {
            let mut meta = ExtensionMeta::new("@acme/extra", "1.0.0");
            meta.peer_dependencies = vec![PeerDependency {
                name: "@acme/base".into(),
                version: "^1".into(),
                optional: false,
            }];
            let mut c = ContributionsBuilder::new(meta);
            c.kind("Gizmo", |k| {
                k.keyword("gizmo");
            });
            c.edge("gizmo_links", |e| {
                e.source_kind("gizmo").target_kind("thing");
            });
            c.enhance("thing", "@acme/base", |e| {
                e.field("badge", |f| {
                    f.field_type(FieldType::String);
                });
            });
            c
        })
}

fn schema_project() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": ["@acme/base", "@acme/extra"]}"#,
    )
    .unwrap();
    specforge_installed::testing::install(dir.path(), &["@acme/base", "@acme/extra"]);
    std::fs::write(
        dir.path().join("old.spec"),
        "// specforge-format: 0.9\nthing t \"T\" {\n}\n",
    )
    .unwrap();
    dir
}

#[specforge_test(
    behavior = "capture_pre_migration_schema_snapshot",
    verify = "snapshot includes node kinds, edge types, and field definitions"
)]
fn the_schema_before_the_hooks_holds_their_kinds_edges_and_fields() {
    let dir = schema_project();
    let runtime = Arc::new(schema_runtime());

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert!(outcome.validated, "{outcome:?}");
    assert!(outcome.rollback.is_none(), "{outcome:?}");
    // Only a pre-migration schema holding all three can name each as lost.
    let warnings: Vec<String> = outcome
        .schema_warnings
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message).to_lowercase())
        .collect();
    for lost in ["kindremoved", "edgeremoved", "fieldremoved"] {
        assert!(
            warnings
                .iter()
                .any(|w| w.starts_with("w053") && w.contains(lost)),
            "{lost} not in {warnings:?}"
        );
    }
    for name in ["gizmo", "gizmo_links", "badge"] {
        assert!(
            warnings.iter().any(|w| w.contains(name)),
            "{name} not in {warnings:?}"
        );
    }
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
        outcome.hook_failures.is_empty(),
        "{:?}",
        outcome.hook_failures
    );
    assert_eq!(
        outcome.hooks_invoked,
        ["@acme/a:migrate_a", "@acme/b:migrate_b"]
    );
    assert!(outcome.rollback.is_none(), "{outcome:?}");
    assert!(outcome.ok());
    // The cycle is the compile's E027, among the post-migration diagnostics.
    assert!(
        outcome
            .post_diagnostics
            .iter()
            .any(|d| d.code == "E027" && d.message.contains("cycle detected in peer dependencies")),
        "{:?}",
        outcome.post_diagnostics
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
    let runtime = Arc::new(hooked(|input| {
        let file = &input.files[0];
        let migrated = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        std::fs::write(
            file,
            format!("{migrated}\nfeature extra \"Extra\" {{\n  behaviors [ghost]\n}}\n"),
        )
        .map_err(|e| e.to_string())
    }));
    let before = specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    );
    assert!(
        !before.diagnostics().iter().any(|d| d.code == "E003"),
        "{:?}",
        before.diagnostics()
    );

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert!(
        outcome
            .post_diagnostics
            .iter()
            .any(|d| d.code == "E003" && d.message.contains("ghost")),
        "{:?}",
        outcome.post_diagnostics
    );
    // `extra` appeared: the graph changed structure, so the run is undone.
    assert!(outcome.rollback.is_some(), "{outcome:?}");
}

#[specforge_test(
    behavior = "verify_graph_protocol_compatibility_after_migration",
    verify = "comparison runs once after extension_migration_hooks_complete"
)]
fn the_schema_is_compared_once_after_the_hooks() {
    let dir = schema_project();
    let runtime = Arc::new(schema_runtime());

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    // The comparison saw the hook's change to the enabled extensions...
    assert!(!outcome.schema_warnings.is_empty(), "{outcome:?}");
    // ...from the two compiles of the run: before the files, after the hooks.
    assert_eq!(handshakes_of(&runtime, "@acme/base"), 2);
    assert_eq!(handshakes_of(&runtime, "@acme/extra"), 1);
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

    assert_eq!(outcome.hooks_invoked, ["@acme/b:migrate_b"]);
    assert_eq!(
        outcome.hook_failures.len(),
        1,
        "{:?}",
        outcome.hook_failures
    );
    assert!(
        outcome.hook_failures[0].contains("migrate_a() of '@acme/a' trapped")
            && outcome.hook_failures[0].contains("bad data"),
        "{:?}",
        outcome.hook_failures
    );
    assert_eq!(hooks_called(&runtime), ["migrate_a", "migrate_b"]);
    assert!(
        outcome.rollback.is_some(),
        "a failed hook rolls the run back"
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
        (outcome.hooks_invoked, hooks_called(&runtime))
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

    assert_eq!(outcome.hooks_invoked, ["@acme/ok:migrate_ok"]);
    assert_eq!(hooks_called(&runtime), ["migrate_ok"]);
    assert!(
        outcome.hook_failures.is_empty(),
        "{:?}",
        outcome.hook_failures
    );
    assert!(
        outcome
            .post_diagnostics
            .iter()
            .any(|d| d.code == "E028" && d.message.contains("@acme/broken")),
        "{:?}",
        outcome.post_diagnostics
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
        outcome.hook_failures.len(),
        1,
        "{:?}",
        outcome.hook_failures
    );
    assert!(
        outcome.hook_failures[0].contains("deadline_exceeded"),
        "{:?}",
        outcome.hook_failures
    );
    assert_eq!(outcome.hooks_invoked, ["@acme/b:migrate_b"]);
    assert!(outcome.rollback.is_some(), "{outcome:?}");
}

#[specforge_test(
    behavior = "invoke_extension_migration_hooks",
    verify = "validation runs once after both core and extension hooks complete"
)]
fn the_project_is_checked_once_after_the_files_and_the_hooks() {
    let dir = project_with_extension();
    let file = dir.path().join("old.spec");
    // The hook sees the file the core migration already rewrote.
    let runtime = Arc::new(hooked(|input| {
        let file = &input.files[0];
        let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        if !text.starts_with("// specforge-format: 1.0") {
            return Err(format!("the core migration did not run first: {text}"));
        }
        std::fs::write(file, format!("{text}// migrated by @acme/x\n")).map_err(|e| e.to_string())
    }));

    let outcome = run(
        &request(&specforge_common::ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime.clone()),
    );

    assert!(
        outcome.validated && outcome.rollback.is_none(),
        "{outcome:?}"
    );
    assert!(
        outcome.hook_failures.is_empty(),
        "{:?}",
        outcome.hook_failures
    );
    assert!(
        std::fs::read_to_string(file)
            .unwrap()
            .ends_with("// migrated by @acme/x\n")
    );
    // One compile before the files, one after the hooks.
    assert_eq!(handshakes_of(&runtime, "@acme/x"), 2);
}

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "an automatic rollback restores only the files its run migrated"
)]
fn an_automatic_rollback_restores_only_what_its_run_migrated() {
    let dir = project_with_extension();
    let root = dir.path();
    let first = run(
        &request(&specforge_common::ProjectRoot::of(root).unwrap()),
        Some(Arc::new(unhooked())),
    );
    assert!(first.ok(), "{first:?}");
    let old = root.join("old.spec");
    let migrated = std::fs::read_to_string(&old).unwrap();
    let edited = format!("{migrated}// edited\n");
    std::fs::write(&old, &edited).unwrap();
    std::fs::write(root.join("second.spec"), OLD).unwrap();

    let second = run(
        &request(&specforge_common::ProjectRoot::of(root).unwrap()),
        Some(Arc::new(hooked(|_| Err("trapped".into())))),
    );

    assert!(second.rollback.is_some() && !second.ok(), "{second:?}");
    assert_eq!(
        std::fs::read_to_string(&old).unwrap(),
        edited,
        "an earlier migration's file keeps its edit"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("second.spec")).unwrap(),
        OLD
    );
    assert!(
        !second
            .writes
            .paths()
            .any(|p| p.ends_with("old.spec") || p.ends_with("second.spec")),
        "{:?}",
        second.writes
    );
}

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "an automatic rollback restores a run made without backups"
)]
fn an_automatic_rollback_restores_a_run_without_backups() {
    let dir = project_with_extension();
    let root = dir.path();
    let project = specforge_common::ProjectRoot::of(root).unwrap();
    let mut req = request(&project);
    req.no_backup = true;

    let outcome = run(&req, Some(Arc::new(hooked(|_| Err("trapped".into())))));

    assert!(outcome.rollback.is_some() && !outcome.ok(), "{outcome:?}");
    assert_eq!(std::fs::read_to_string(root.join("old.spec")).unwrap(), OLD);
    assert!(!root.join("old.spec.bak").exists());
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
    let record = specforge_migrate::MigrationRecord::read(root)
        .unwrap()
        .expect("a record");
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
    assert!(
        specforge_migrate::MigrationRecord::read(root)
            .unwrap()
            .is_some()
    );
    std::fs::write(root.join("newer.spec"), OLD).unwrap();
    let project = specforge_common::ProjectRoot::of(root).unwrap();
    let mut req = request(&project);
    req.no_backup = true;

    let outcome = run(&req, Some(Arc::new(unhooked())));

    assert!(outcome.ok(), "{outcome:?}");
    assert!(
        specforge_migrate::MigrationRecord::read(root)
            .unwrap()
            .is_none()
    );
}
