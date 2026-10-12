//! What a changed path is to a project session (behavior
//! `classify_project_changes`): a source, an environment input, a check
//! input, or nothing.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde_json::json;
use specforge_project::{InputRole, ProjectSession, SharedRuntime, UpdateKind, WatchRoot, Watched};
use specforge_test::prelude::*;
use tempfile::TempDir;

use crate::check_passes::{SPEC, passes_extension, project as passes_project, write_cache};

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A project whose `specforge.json` is `config`, opened with no runtime
/// (no extension loads; the config is still read).
fn open(config: serde_json::Value, files: &[(&str, &str)]) -> (TempDir, ProjectSession) {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    for (path, text) in files {
        write(dir.path(), path, text);
    }
    let session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        None,
    );
    (dir, session)
}

fn source(key: &str) -> InputRole {
    InputRole::Source(key.to_string())
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a discovered .spec file is a source change keyed relative to the spec root"
)]
fn a_discovered_spec_file_is_a_source_keyed_from_the_spec_root() {
    let (dir, session) = open(
        json!({"name": "c", "version": "0.1.0", "spec_root": "spec"}),
        &[
            ("spec/a.spec", "term alpha \"A\" {\n}\n"),
            ("spec/sub/b.spec", "term beta \"B\" {\n}\n"),
        ],
    );
    let root = dir.path();
    let canonical_root = fs::canonicalize(root).unwrap();

    assert_eq!(
        session.inputs().classify(&root.join("spec/a.spec")),
        source("a.spec")
    );
    assert_eq!(
        session.inputs().classify(&root.join("spec/sub/b.spec")),
        source("sub/b.spec")
    );
    // The same file however the directory is spelled (a symlinked temp dir).
    assert_eq!(
        session
            .inputs()
            .classify(&canonical_root.join("spec/a.spec")),
        source("a.spec")
    );
    // A deleted (or not yet written) file is keyed the same way.
    assert_eq!(
        session.inputs().classify(&root.join("spec/gone.spec")),
        source("gone.spec")
    );
    assert_eq!(
        session
            .project()
            .environment()
            .source_key(&root.join("spec/sub/b.spec")),
        "sub/b.spec"
    );
    // A .spec file outside the spec root is not a source of the project.
    assert_eq!(
        session.inputs().classify(&root.join("outside.spec")),
        InputRole::Unrelated
    );

    // A batch: sorted, deduplicated keys, nothing else.
    let paths = [
        root.join("spec/sub/b.spec"),
        root.join("spec/a.spec"),
        canonical_root.join("spec/a.spec"),
        root.join("notes.txt"),
    ];
    let changes = session.inputs().changes(paths.iter().map(|p| p.as_path()));
    assert_eq!(changes.sources, vec!["a.spec", "sub/b.spec"]);
    assert!(!changes.environment && !changes.check_inputs);
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "specforge.json, specforge.lock and a loaded extension module are environment changes"
)]
fn config_lock_and_loaded_modules_are_environment_inputs() {
    let (dir, session) = open(
        json!({
            "name": "c",
            "version": "0.1.0",
            "extensions": ["@specforge/software", "@acme/local=ext/local.wasm", "@acme/installed"]
        }),
        &[("main.spec", "")],
    );
    let root = dir.path();

    // A builtin has no module on disk; a local entry and an installed one do.
    assert_eq!(
        session.inputs().watched(),
        vec![
            Watched::Sources(root.to_path_buf()),
            Watched::File(root.join("specforge.json")),
            Watched::File(root.join("specforge.lock")),
            Watched::File(root.join("ext/local.wasm")),
            Watched::File(root.join(".specforge/extensions/@acme/installed/extension.wasm")),
        ]
    );
    for path in [
        "specforge.json",
        "specforge.lock",
        "ext/local.wasm",
        ".specforge/extensions/@acme/installed/extension.wasm",
    ] {
        assert_eq!(
            session.inputs().classify(&root.join(path)),
            InputRole::Environment,
            "{path}"
        );
    }
    let changes = session
        .inputs()
        .changes([root.join("specforge.lock").as_path()]);
    assert!(changes.environment && changes.sources.is_empty());
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a .wasm file no extension loads changes nothing"
)]
fn an_unloaded_wasm_changes_nothing() {
    let (dir, session) = open(
        json!({"name": "c", "version": "0.1.0", "extensions": ["@acme/local=ext/local.wasm"]}),
        &[("main.spec", "")],
    );
    let root = dir.path();

    for path in ["target/x.wasm", "ext/other.wasm", "plugin.wasm"] {
        assert_eq!(
            session.inputs().classify(&root.join(path)),
            InputRole::Unrelated,
            "{path}"
        );
    }
    let changes = session
        .inputs()
        .changes([root.join("target/x.wasm").as_path()]);
    assert!(changes.is_empty());
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "specforge-cache.json re-runs the checks without re-parsing"
)]
fn the_build_cache_reruns_the_checks_only() {
    let dir = passes_project(SPEC);
    let root = dir.path();
    let ext = Arc::new(passes_extension());
    let mut session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(Arc::clone(&ext) as SharedRuntime),
    );
    let cache = root.join(specforge_project::BUILD_CACHE_FILE);
    assert!(
        session
            .inputs()
            .watched()
            .contains(&Watched::File(cache.clone()))
    );
    assert!(
        session
            .project()
            .diagnostics()
            .iter()
            .all(|d| d.code != "W144")
    );

    // An invalid cache: the check passes report W144 when they read it.
    write_cache(&dir, "{ not json");
    assert_eq!(session.inputs().classify(&cache), InputRole::CheckInput);
    let changes = session.inputs().changes([cache.as_path()]);
    assert!(changes.check_inputs && !changes.environment && changes.sources.is_empty());

    let update = session.apply(&changes).expect("a check input changed");
    assert_eq!(update.kind, UpdateKind::Checks);
    assert!(
        update.rebuilt_files.is_empty(),
        "{:?}",
        update.rebuilt_files
    );
    assert!(update.delta.is_empty());
    assert!(update.diagnostics.iter().any(|d| d.code == "W144"));

    // Without a check pass, the cache is no input at all.
    let (plain, session) = open(json!({"name": "c", "version": "0.1.0"}), &[("a.spec", "")]);
    let cache = plain.path().join(specforge_project::BUILD_CACHE_FILE);
    assert!(
        !session
            .inputs()
            .watched()
            .contains(&Watched::File(cache.clone()))
    );
    assert_eq!(session.inputs().classify(&cache), InputRole::Unrelated);
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a file a file_reference field names re-runs the checks"
)]
fn a_referenced_file_reruns_the_checks() {
    let dir = passes_project("gadget doc \"Doc\" {\n  docs [\"docs/guide.md\"]\n}\n");
    let root = dir.path();
    let ext = Arc::new(passes_extension());
    let mut session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(Arc::clone(&ext) as SharedRuntime),
    );
    let e016 = |session: &ProjectSession| {
        session
            .project()
            .diagnostics()
            .iter()
            .filter(|d| d.code == "E016")
            .count()
    };
    assert_eq!(e016(&session), 1, "{:?}", session.project().diagnostics());

    write(root, "docs/guide.md", "# Guide\n");
    let guide = root.join("docs/guide.md");
    assert_eq!(session.inputs().classify(&guide), InputRole::CheckInput);
    let update = session
        .apply(&session.inputs().changes([guide.as_path()]))
        .expect("a referenced file changed");
    assert_eq!(update.kind, UpdateKind::Checks);
    assert_eq!(e016(&session), 0, "{:?}", session.project().diagnostics());
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "an update that names a new file the checks read changes the session's inputs"
)]
fn an_update_that_names_a_new_file_changes_the_inputs() {
    use specforge_project::{Changes, SourceChange};

    let dir = passes_project("gadget doc \"Doc\" {\n}\n");
    let root = dir.path();
    let ext = Arc::new(passes_extension());
    let mut session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(Arc::clone(&ext) as SharedRuntime),
    );
    let before = session.inputs().clone();

    // An edit that names a file: the inputs gain it.
    let named = "gadget doc \"Doc\" {\n  docs [\"new.md\"]\n}\n";
    write(root, "a.spec", named);
    let keys = ["a.spec".to_string()];
    let update = session.update(SourceChange::Disk(&keys));
    assert!(update.inputs_changed);
    assert_ne!(*session.inputs(), before);
    assert!(
        session
            .inputs()
            .watched()
            .contains(&Watched::File(root.join("new.md")))
    );

    // The same update again, and a re-check of nothing renamed: no change.
    assert!(!session.update(SourceChange::Disk(&keys)).inputs_changed);
    let recheck = session
        .apply(&Changes {
            check_inputs: true,
            ..Changes::default()
        })
        .unwrap();
    assert!(!recheck.inputs_changed);

    // A reload with the same config reads the same files: same inputs.
    assert!(!session.reload_environment().inputs_changed);

    // A reload that moves the spec root changes them.
    fs::write(
        root.join("specforge.json"),
        json!({"name": "p", "version": "0.1.0", "extensions": ["@test/passes"], "spec_root": "other"})
            .to_string(),
    )
    .unwrap();
    assert!(session.reload_environment().inputs_changed);
}

/// An extension whose `note` kind's `doc` (a string) and `docs` (a list)
/// a `file_exists` rule `F001` reads.
fn file_rule_extension() -> specforge_wasm::testing::InProcessRuntime {
    use specforge_extension_sdk::prelude::*;
    specforge_wasm::testing::InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@test/files", "1.0.0"));
        c.kind("note", |k| {
            k.keyword("note");
            k.field("doc", |f| {
                f.field_type(FieldType::String);
            });
            k.field("docs", |f| {
                f.field_type(FieldType::StringList);
            });
        });
        for (code, field) in [("F001", "doc"), ("F002", "docs")] {
            c.rule(code, |r| {
                r.check(CheckKind::FileExists)
                    .severity(ValidationSeverity::Warning)
                    .target_kind("note")
                    .field(field)
                    .message_template("note '{id}': missing '{value}'");
            });
        }
        c
    })
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a file a file_exists rule names re-runs the checks"
)]
fn a_file_a_file_exists_rule_names_reruns_the_checks() {
    let (dir, _) = open(
        json!({"name": "c", "version": "0.1.0", "spec_root": "spec", "extensions": ["@test/files"]}),
        &[(
            "spec/a.spec",
            "note n \"N\" {\n  doc \"guide.md\"\n  docs [\"more/one.md\"]\n}\n",
        )],
    );
    let root = dir.path();
    specforge_installed::testing::install(root, &["@test/files"]);
    let ext = Arc::new(file_rule_extension());
    let mut session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(Arc::clone(&ext) as SharedRuntime),
    );
    let missing = |session: &ProjectSession| -> Vec<String> {
        session
            .project()
            .diagnostics()
            .iter()
            .filter(|d| d.code.starts_with('F'))
            .map(|d| d.message.clone())
            .collect()
    };
    assert_eq!(
        missing(&session),
        [
            "note 'n': missing 'guide.md'",
            "note 'n': missing 'more/one.md'"
        ]
    );

    // A file named by a scalar field, under the spec root.
    write(root, "spec/guide.md", "# Guide\n");
    let guide = root.join("spec/guide.md");
    assert_eq!(session.inputs().classify(&guide), InputRole::CheckInput);
    let update = session
        .apply(&session.inputs().changes([guide.as_path()]))
        .expect("a file a file_exists rule names changed");
    assert_eq!(update.kind, UpdateKind::Checks);
    assert_eq!(missing(&session), ["note 'n': missing 'more/one.md'"]);

    // An item of a list field, in a directory that did not exist.
    write(root, "spec/more/one.md", "# One\n");
    let one = root.join("spec/more/one.md");
    assert_eq!(session.inputs().classify(&one), InputRole::CheckInput);
    session
        .apply(&session.inputs().changes([one.as_path()]))
        .unwrap();
    assert!(
        missing(&session).is_empty(),
        "{:?}",
        session.project().diagnostics()
    );
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "an excluded or undiscovered .spec file changes nothing"
)]
fn excluded_and_undiscovered_spec_files_change_nothing() {
    let (dir, session) = open(
        json!({"name": "c", "version": "0.1.0", "spec_root": "spec", "exclude": ["drafts"]}),
        &[("spec/a.spec", "")],
    );
    let root = dir.path();

    for path in [
        "spec/drafts/x.spec",
        "spec/target/y.spec",
        "spec/node_modules/z.spec",
        "spec/notes.md",
        "outside/w.spec",
    ] {
        assert_eq!(
            session.inputs().classify(&root.join(path)),
            InputRole::Unrelated,
            "{path}"
        );
    }
    let paths = [root.join("spec/drafts/x.spec"), root.join("spec/notes.md")];
    assert!(
        session
            .inputs()
            .changes(paths.iter().map(|p| p.as_path()))
            .is_empty()
    );
}

/// The directories a watcher must watch: the root, a spec root outside
/// it, and the directory of a module outside both.
#[test]
fn watch_roots_cover_every_input() {
    let outside = TempDir::new().unwrap();
    let module_dir = fs::canonicalize(outside.path()).unwrap().join("mods");
    fs::create_dir_all(&module_dir).unwrap();
    let module = module_dir.join("ext.wasm");
    let (dir, session) = open(
        json!({
            "name": "c",
            "version": "0.1.0",
            "spec_root": "spec",
            "extensions": [format!("@acme/far={}", module.display())]
        }),
        &[("spec/a.spec", "")],
    );
    let root = fs::canonicalize(dir.path()).unwrap();
    assert_eq!(
        session.inputs().watch_roots(),
        vec![
            WatchRoot {
                dir: root,
                recursive: true
            },
            WatchRoot {
                dir: module_dir,
                recursive: true
            },
        ]
    );
}

/// A session with no project has no inputs: a `.spec` path is a buffer
/// source keyed by itself, and nothing else is anything.
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a detached session classifies a .spec buffer as a source and nothing else as an input"
)]
fn a_detached_session_takes_spec_buffers_only() {
    let mut session = ProjectSession::detached();

    assert_eq!(
        session.inputs().classify(Path::new("/x/a.spec")),
        source("/x/a.spec")
    );
    for path in ["/x/specforge.json", "/x/specforge.lock", "/x/n.md"] {
        assert_eq!(
            session.inputs().classify(Path::new(path)),
            InputRole::Unrelated,
            "{path}"
        );
    }
    assert!(session.inputs().watch_roots().is_empty());
    assert!(session.stale().is_empty());
    let environment = specforge_project::Changes {
        environment: true,
        ..Default::default()
    };
    assert!(session.apply(&environment).is_none());

    let update = session.reload_environment();
    assert_eq!(update.kind, UpdateKind::Environment);
    assert!(update.delta.is_empty());
    assert!(update.rebuilt_files.is_empty());
}

/// A project whose `specforge.json` names a module in another temp
/// directory, and whose one `gadget` (the in-process passes extension, so
/// build-cache and file-reference inputs both exist) names `docs`. The
/// paths of every input the session classifies, as a test spells them:
/// config, lock, module, build cache, then each of `docs` resolved against
/// the root (the spec root), and a sibling of the last in its directory.
struct InputsLayout {
    project: TempDir,
    _outside: TempDir,
    _far: TempDir,
    elsewhere: TempDir,
    /// The module in `_far`, and the directory it lies in.
    module: std::path::PathBuf,
    /// A file that exists in an existing directory outside the root.
    out_guide: std::path::PathBuf,
    /// A file in a directory outside the root that does not exist, in a
    /// directory (`elsewhere`) nothing else is in.
    far_guide: std::path::PathBuf,
    /// A file in a directory under the root that does not exist.
    missing_sub: std::path::PathBuf,
    /// Another file in that missing directory.
    missing_x: std::path::PathBuf,
}

fn inputs_layout() -> InputsLayout {
    let outside = TempDir::new().unwrap();
    let far = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let module_dir = fs::canonicalize(far.path()).unwrap().join("mods");
    fs::create_dir_all(&module_dir).unwrap();
    let module = module_dir.join("ext.wasm");
    let outside_name = outside.path().file_name().unwrap().to_string_lossy();
    let elsewhere_name = elsewhere.path().file_name().unwrap().to_string_lossy();
    let project = passes_project("");
    fs::write(
        project.path().join("specforge.json"),
        json!({
            "name": "p",
            "version": "0.1.0",
            "extensions": ["@test/passes", format!("@acme/far={}", module.display())],
        })
        .to_string(),
    )
    .unwrap();
    write(
        project.path(),
        "a.spec",
        &format!(
            "gadget doc \"Doc\" {{\n  docs [\"../{outside_name}/guide.md\", \"missing/sub.md\", \"../{elsewhere_name}/far/deep/guide.md\"]\n}}\n"
        ),
    );
    let root = project.path();
    InputsLayout {
        out_guide: root.join(format!("../{outside_name}/guide.md")),
        far_guide: root.join(format!("../{elsewhere_name}/far/deep/guide.md")),
        missing_sub: root.join("missing/sub.md"),
        missing_x: root.join("missing/x.md"),
        project,
        _outside: outside,
        _far: far,
        elsewhere,
        module,
    }
}

/// Every input the session classifies lies under a directory a watcher
/// watches (plan 03 pin: today's behaviour, which holds).
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a session's watch roots cover every input it classifies"
)]
fn every_input_lies_under_a_watch_root() {
    let layout = inputs_layout();
    let root = layout.project.path();
    let ext = Arc::new(passes_extension());
    let session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(ext as SharedRuntime),
    );
    let roots: Vec<_> = session
        .inputs()
        .watch_roots()
        .into_iter()
        .map(|root| root.dir)
        .collect();

    let inputs = [
        (root.join("specforge.json"), InputRole::Environment),
        (root.join("specforge.lock"), InputRole::Environment),
        (layout.module.clone(), InputRole::Environment),
        (
            root.join(specforge_project::BUILD_CACHE_FILE),
            InputRole::CheckInput,
        ),
        (layout.out_guide.clone(), InputRole::CheckInput),
        (layout.missing_sub.clone(), InputRole::CheckInput),
        (layout.far_guide.clone(), InputRole::CheckInput),
        // A file in a missing file's directory changes the E016 suggestion.
        (layout.missing_x.clone(), InputRole::CheckInput),
    ];
    for (path, role) in inputs {
        assert_eq!(session.inputs().classify(&path), role, "{}", path.display());
        let canonical = fs::canonicalize(path.parent().unwrap())
            .map(|dir| dir.join(path.file_name().unwrap()))
            // The directory does not exist: its nearest existing ancestor.
            .unwrap_or_else(|_| {
                let mut dir = path.parent().unwrap();
                while !dir.exists() {
                    dir = dir.parent().unwrap();
                }
                fs::canonicalize(dir).unwrap()
            });
        assert!(
            roots.iter().any(|watched| canonical.starts_with(watched)),
            "{} is under no watch root of {roots:?}",
            path.display()
        );
    }

    // The missing directory outside the root is watched from its nearest
    // existing ancestor, for that ancestor's own entries, and the first
    // directory created on the way to it is the input's change.
    let elsewhere = fs::canonicalize(layout.elsewhere.path()).unwrap();
    assert!(
        session.inputs().watch_roots().contains(&WatchRoot {
            dir: elsewhere.clone(),
            recursive: false
        }),
        "{roots:?}"
    );
    assert_eq!(
        session.inputs().classify(&elsewhere.join("far")),
        InputRole::CheckInput
    );
}
