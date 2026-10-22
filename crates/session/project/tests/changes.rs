//! What a batch of changed paths is to a project session (behavior
//! `classify_project_changes`): the comparison `stale` makes, narrowed to
//! what the paths reach. These pins drive directory, symlink and
//! unchanged-file events.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use specforge_project::testing::{TestProject, compile_at, disk_changed};
use specforge_project::{Buffer, ProjectSession, SharedRuntime, UpdateKind, WatchRoot, Watched};
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
fn open(config: serde_json::Value, files: &[(&str, &str)]) -> (TestProject, ProjectSession) {
    let dir = TestProject::new()
        .raw_config(config.to_string())
        .files(files);
    let session = dir.session_bare();
    (dir, session)
}

fn plain() -> serde_json::Value {
    json!({"name": "c", "version": "0.1.0"})
}

/// The session agrees with a fresh compile of what is on disk now.
fn assert_matches_a_fresh_compile(session: &ProjectSession, root: &Path) {
    let fresh = compile_at(root, None);
    assert_eq!(
        session.project().diagnostics(),
        fresh.diagnostics(),
        "a session reports a fresh compile's diagnostics"
    );
}

const TERM: &str = "term alpha_one \"A\" {}\n";

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory created, moved or deleted under the spec root changes every source under it"
)]
fn a_directory_moved_inside_the_spec_root_moves_its_sources() {
    let (dir, mut session) = open(plain(), &[("r.spec", ""), ("old/a.spec", TERM)]);
    let root = fs::canonicalize(dir.root()).unwrap();
    fs::rename(root.join("old"), root.join("new")).unwrap();

    let changes = session.changes([root.join("new").as_path(), root.join("old").as_path()]);
    assert_eq!(changes.sources(), vec!["new/a.spec", "old/a.spec"]);
    assert!(session.apply(&changes).is_some());
    assert!(session.stale().is_empty());
    let keys: Vec<&str> = session.project().source_keys().into_iter().collect();
    assert_eq!(keys, vec!["new/a.spec", "r.spec"]);
    assert_matches_a_fresh_compile(&session, &root);
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory created, moved or deleted under the spec root changes every source under it"
)]
fn a_directory_moved_in_from_outside_adds_its_sources() {
    let (dir, session) = open(plain(), &[("r.spec", "")]);
    let root = fs::canonicalize(dir.root()).unwrap();
    let outside = TempDir::new().unwrap();
    write(outside.path(), "pkg/g.spec", TERM);
    fs::rename(outside.path().join("pkg"), root.join("pkg")).unwrap();

    let changes = session.changes([root.join("pkg").as_path()]);
    assert_eq!(changes.sources(), vec!["pkg/g.spec"]);
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory created, moved or deleted under the spec root changes every source under it"
)]
fn a_directory_moved_out_removes_its_sources() {
    let (dir, session) = open(plain(), &[("r.spec", ""), ("d/e/x.spec", TERM)]);
    let root = fs::canonicalize(dir.root()).unwrap();
    let outside = TempDir::new().unwrap();
    fs::rename(root.join("d"), outside.path().join("d")).unwrap();

    let changes = session.changes([root.join("d").as_path()]);
    assert_eq!(changes.sources(), vec!["d/e/x.spec"]);
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory created, moved or deleted under the spec root changes every source under it"
)]
fn a_directory_reported_alone_brings_the_sources_written_in_it() {
    let (dir, session) = open(plain(), &[("r.spec", "")]);
    let root = fs::canonicalize(dir.root()).unwrap();
    write(&root, "n/m/a.spec", TERM);
    write(&root, "n/m/b.spec", "term beta_one \"B\" {}\n");

    // inotify can report the new directory alone: the files written in it
    // before its watch was added are never reported.
    let changes = session.changes([root.join("n").as_path()]);
    assert_eq!(changes.sources(), vec!["n/m/a.spec", "n/m/b.spec"]);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a symlinked source is keyed by its link and changes when its target does"
)]
fn a_symlink_inside_the_spec_root_is_keyed_by_its_own_name() {
    let (dir, mut session) = open(
        plain(),
        &[
            ("shared/r.spec", "term root_one \"R\" {}\n"),
            ("r.spec", "term root_one \"R\" {}\n"),
        ],
    );
    let root = fs::canonicalize(dir.root()).unwrap();
    std::os::unix::fs::symlink("shared/r.spec", root.join("alias.spec")).unwrap();

    let changes = session.changes([root.join("alias.spec").as_path()]);
    assert_eq!(changes.sources(), vec!["alias.spec"]);
    assert!(session.apply(&changes).is_some());
    assert_matches_a_fresh_compile(&session, &root);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a symlinked source is keyed by its link and changes when its target does"
)]
fn an_edit_to_a_symlinked_sources_target_changes_that_source() {
    let (_dir, session, outside) = linked_project();
    fs::write(
        outside.join("real.spec"),
        "term real_one \"R\" {}\nterm real_two \"R\" {}\n",
    )
    .unwrap();

    let changes = session.changes([outside.join("real.spec").as_path()]);
    assert_eq!(changes.sources(), vec!["link.spec"]);
}

/// A project whose `link.spec` is a symlink to `real.spec` in a directory
/// outside it: (project, opened session, canonical outside directory).
#[cfg(unix)]
fn linked_project() -> (TestProject, ProjectSession, PathBuf) {
    let outside = TempDir::new().unwrap();
    let outside_dir = fs::canonicalize(outside.path()).unwrap();
    // Kept alive for the test: the directory is leaked into the process.
    std::mem::forget(outside);
    fs::write(outside_dir.join("real.spec"), "term real_one \"R\" {}\n").unwrap();
    let dir = TestProject::new()
        .raw_config(plain().to_string())
        .file("a.spec", "");
    std::os::unix::fs::symlink(outside_dir.join("real.spec"), dir.root().join("link.spec"))
        .unwrap();
    let session = dir.session_bare();
    (dir, session, outside_dir)
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a path whose file is as the session last read it changes nothing"
)]
fn a_path_whose_file_is_as_read_changes_nothing() {
    let (dir, session) = open(plain(), &[("a.spec", TERM)]);
    let root = fs::canonicalize(dir.root()).unwrap();

    assert!(session.changes([root.join("a.spec").as_path()]).is_empty());
    assert!(
        session
            .changes([root.join("specforge.json").as_path()])
            .is_empty()
    );
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a detached session takes no change from disk"
)]
fn a_detached_session_takes_no_change_from_disk() {
    let session = ProjectSession::detached();
    let dir = TestProject::new().without_config().file("b.spec", TERM);

    assert!(
        session
            .changes([dir.root().join("b.spec").as_path()])
            .is_empty()
    );
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a session's watch roots and the LSP's watchers cover every symlinked source's target"
)]
fn a_symlinked_sources_target_directory_is_watched() {
    let (_dir, session, outside) = linked_project();

    assert!(
        session.inputs().watch_roots().contains(&WatchRoot {
            dir: outside.clone(),
            recursive: true
        }),
        "{:?}",
        session.inputs().watch_roots()
    );
    assert!(
        session
            .inputs()
            .watched()
            .contains(&Watched::File(outside.join("real.spec"))),
        "{:?}",
        session.inputs().watched()
    );
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory created, moved or deleted under the spec root changes every source under it"
)]
fn a_held_file_under_a_moved_directory_is_left_out() {
    let (dir, mut session) = open(
        plain(),
        &[
            ("old/a.spec", TERM),
            ("old/b.spec", "term beta_one \"B\" {}\n"),
        ],
    );
    let root = fs::canonicalize(dir.root()).unwrap();
    session.hold(&[Buffer::new(
        root.join("old/a.spec"),
        "term held_one \"H\" {}\n".to_string(),
    )]);
    fs::rename(root.join("old"), root.join("new")).unwrap();

    let changes = session.changes([root.join("new").as_path(), root.join("old").as_path()]);
    assert_eq!(
        changes.sources(),
        vec!["new/a.spec", "new/b.spec", "old/b.spec"]
    );
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory created, moved or deleted under the spec root changes every source under it"
)]
fn a_batch_of_many_file_events_is_one_update() {
    let files: Vec<String> = (0..40).map(|n| format!("feat/f{n:02}.spec")).collect();
    let dir = files.iter().enumerate().fold(
        TestProject::new().raw_config(plain().to_string()),
        |dir, (n, path)| dir.file(path, format!("term feat_{n:02} \"T\" {{}}\n")),
    );
    let mut session = dir.session_bare();
    let root = fs::canonicalize(dir.root()).unwrap();

    // The branch checkout: every file of `feat/` goes, forty more are
    // written into a new nested directory.
    let mut batch: Vec<PathBuf> = Vec::new();
    for path in &files {
        fs::remove_file(root.join(path)).unwrap();
        batch.push(root.join(path));
    }
    for n in 0..40 {
        let path = format!("moved/deep/g{n:02}.spec");
        write(&root, &path, &format!("term moved_{n:02} \"T\" {{}}\n"));
        batch.push(root.join(path));
    }
    let changes = session.changes(batch.iter().map(PathBuf::as_path));
    assert_eq!(changes.sources().len(), 80, "{changes:?}");
    let update = session.apply(&changes).expect("one update");
    assert_eq!(update.rebuilt_files.len(), 80);
    assert_matches_a_fresh_compile(&session, &root);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a symlinked source is keyed by its link and changes when its target does"
)]
fn stale_sees_an_edit_to_a_symlinked_sources_target() {
    let (_dir, session, outside) = linked_project();
    fs::write(
        outside.join("real.spec"),
        "term real_one \"R\" {}\nterm real_two \"R\" {}\n",
    )
    .unwrap();

    assert_eq!(session.stale().sources(), vec!["link.spec"]);
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
    let root = dir.root();
    let canonical_root = fs::canonicalize(root).unwrap();
    write(root, "spec/a.spec", "term alpha \"A\" {\n}\n\n");
    write(root, "spec/sub/b.spec", "term beta \"B\" {\n}\n\n");

    // A deleted (or not yet written) file is keyed the same way.
    assert!(
        session
            .changes([root.join("spec/gone.spec").as_path()])
            .is_empty()
    );
    write(root, "spec/gone.spec", "");
    assert_eq!(
        session
            .changes([root.join("spec/gone.spec").as_path()])
            .sources(),
        vec!["gone.spec"]
    );
    assert_eq!(
        session
            .project()
            .environment()
            .source_key(&root.join("spec/sub/b.spec")),
        "sub/b.spec"
    );
    // A .spec file outside the spec root is not a source of the project.
    write(root, "outside.spec", "term out \"O\" {}\n");
    assert!(
        session
            .changes([root.join("outside.spec").as_path()])
            .is_empty()
    );

    // A batch: sorted, deduplicated keys, nothing else; the same file
    // however the directory is spelled (a symlinked temp dir).
    let paths = [
        root.join("spec/sub/b.spec"),
        root.join("spec/a.spec"),
        canonical_root.join("spec/a.spec"),
        root.join("notes.txt"),
    ];
    let changes = session.changes(paths.iter().map(|p| p.as_path()));
    assert_eq!(changes.sources(), vec!["a.spec", "sub/b.spec"]);
    assert!(changes.environment().is_empty() && changes.check_inputs().is_empty());
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
    let root = dir.root();

    for path in ["target/x.wasm", "ext/other.wasm", "plugin.wasm"] {
        write(root, path, "bytes");
        assert!(
            session.changes([root.join(path).as_path()]).is_empty(),
            "{path}"
        );
    }
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "specforge-cache.json re-runs the checks without re-parsing"
)]
fn the_build_cache_reruns_the_checks_only() {
    let dir = passes_project(SPEC);
    let root = dir.root();
    let ext = Arc::new(passes_extension());
    let mut session = dir.session_in(Arc::clone(&ext) as SharedRuntime);
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
    let changes = session.changes([cache.as_path()]);
    assert_eq!(changes.check_inputs(), vec![cache.clone()]);
    assert!(changes.environment().is_empty() && changes.sources().is_empty());

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
    let cache = plain.root().join(specforge_project::BUILD_CACHE_FILE);
    assert!(
        !session
            .inputs()
            .watched()
            .contains(&Watched::File(cache.clone()))
    );
    fs::write(&cache, "{}").unwrap();
    assert!(session.changes([cache.as_path()]).is_empty());
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a file a file_reference field names re-runs the checks"
)]
fn a_referenced_file_reruns_the_checks() {
    let dir = passes_project("gadget doc \"Doc\" {\n  docs [\"docs/guide.md\"]\n}\n");
    let root = dir.root();
    let ext = Arc::new(passes_extension());
    let mut session = dir.session_in(Arc::clone(&ext) as SharedRuntime);
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
    let update = session
        .apply(&session.changes([guide.as_path()]))
        .expect("a referenced file changed");
    assert_eq!(update.kind, UpdateKind::Checks);
    assert_eq!(e016(&session), 0, "{:?}", session.project().diagnostics());
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "an update that names a new file the checks read changes the session's inputs"
)]
fn an_update_that_names_a_new_file_changes_the_inputs() {
    let dir = passes_project("gadget doc \"Doc\" {\n}\n");
    let root = dir.root();
    let ext = Arc::new(passes_extension());
    let mut session = dir.session_in(Arc::clone(&ext) as SharedRuntime);
    let before = session.inputs().clone();

    // An edit that names a file: the inputs gain it.
    write(root, "new.md", "# New\n");
    let named = "gadget doc \"Doc\" {\n  docs [\"new.md\"]\n}\n";
    write(root, "a.spec", named);
    let keys = ["a.spec".to_string()];
    let update = disk_changed(&mut session, &keys).unwrap();
    assert!(update.inputs_changed);
    assert_ne!(*session.inputs(), before);
    assert!(
        session
            .inputs()
            .watched()
            .contains(&Watched::File(root.join("new.md")))
    );

    // The same update again, and a re-check after the named file is rewritten: no change.
    assert!(disk_changed(&mut session, &keys).is_none());
    write(root, "new.md", "# New, longer\n");
    let rewritten = session.changes([root.join("new.md").as_path()]);
    let recheck = session.apply(&rewritten).expect("a check input changed");
    assert_eq!(recheck.kind, UpdateKind::Checks);
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
pub(crate) fn file_rule_extension() -> specforge_wasm::testing::InProcessRuntime {
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
    let root = dir.root();
    let ext = Arc::new(file_rule_extension());
    specforge_installed::testing::install(root, &*ext, &["@test/files"]);
    let mut session = dir.session_in(Arc::clone(&ext) as SharedRuntime);
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
    let update = session
        .apply(&session.changes([guide.as_path()]))
        .expect("a file a file_exists rule names changed");
    assert_eq!(update.kind, UpdateKind::Checks);
    assert_eq!(missing(&session), ["note 'n': missing 'more/one.md'"]);

    // An item of a list field, in a directory that did not exist.
    write(root, "spec/more/one.md", "# One\n");
    let one = root.join("spec/more/one.md");
    session.apply(&session.changes([one.as_path()])).unwrap();
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
    let root = dir.root();

    for path in [
        "spec/drafts/x.spec",
        "spec/target/y.spec",
        "spec/node_modules/z.spec",
        "spec/notes.md",
        "outside/w.spec",
    ] {
        write(root, path, "");
        assert!(
            session.changes([root.join(path).as_path()]).is_empty(),
            "{path}"
        );
    }
}

/// A session with no project has no inputs: it takes no change from disk,
/// and the environment has nothing to reload.
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a detached session takes no change from disk"
)]
fn a_detached_session_has_no_inputs() {
    let mut session = ProjectSession::detached();

    assert!(session.inputs().watch_roots().is_empty());
    assert!(session.stale().is_empty());
    let dir = TempDir::new().unwrap();
    write(dir.path(), "specforge.json", "{}");
    assert!(
        session
            .changes([dir.path().join("specforge.json").as_path()])
            .is_empty()
    );
    assert!(session.ensure_fresh().is_none());

    let update = session.reload_environment();
    assert_eq!(update.kind, UpdateKind::Environment);
    assert!(update.delta.is_empty());
    assert!(update.rebuilt_files.is_empty());
}

/// A project whose `specforge.json` names a module in another temp
/// directory, and whose one `gadget` (the in-process passes extension, so
/// build-cache and file-reference inputs both exist) names `docs`. The
/// paths of every input the session reaches, as a test spells them:
/// config, lock, module, build cache, then each of `docs` resolved against
/// the root (the spec root), and a sibling of the last in its directory.
struct InputsLayout {
    project: TestProject,
    _outside: TempDir,
    _far: TempDir,
    elsewhere: TempDir,
    /// The module in `_far`, and the directory it lies in.
    module: PathBuf,
    /// A file that exists in an existing directory outside the root.
    out_guide: PathBuf,
    /// A file in a directory outside the root that does not exist, in a
    /// directory (`elsewhere`) nothing else is in.
    far_guide: PathBuf,
    /// A file in a directory under the root that does not exist.
    missing_sub: PathBuf,
    /// Another file in that missing directory.
    missing_x: PathBuf,
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
        project.root().join("specforge.json"),
        json!({
            "name": "p",
            "version": "0.1.0",
            "extensions": ["@test/passes", format!("@acme/far={}", module.display())],
        })
        .to_string(),
    )
    .unwrap();
    write(
        project.root(),
        "a.spec",
        &format!(
            "gadget doc \"Doc\" {{\n  docs [\"../{outside_name}/guide.md\", \"missing/sub.md\", \"../{elsewhere_name}/far/deep/guide.md\"]\n}}\n"
        ),
    );
    let root = project.root();
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

/// Every input the session reaches lies under a directory a watcher
/// watches.
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a session's watch roots cover every input it classifies"
)]
fn every_input_lies_under_a_watch_root() {
    let layout = inputs_layout();
    let root = layout.project.root();
    let ext = Arc::new(passes_extension());
    let session = layout.project.session_in(ext as SharedRuntime);
    let roots: Vec<_> = session
        .inputs()
        .watch_roots()
        .into_iter()
        .map(|root| root.dir)
        .collect();

    // A directory created on the way to the missing directory outside the
    // root is the input's change, whatever the stamps say.
    let elsewhere = fs::canonicalize(layout.elsewhere.path()).unwrap();
    fs::create_dir(elsewhere.join("far")).unwrap();
    assert_eq!(
        session
            .changes([elsewhere.join("far").as_path()])
            .check_inputs(),
        vec![elsewhere.join("far")]
    );

    let inputs = [
        root.join("specforge.json"),
        root.join("specforge.lock"),
        layout.module.clone(),
        root.join(specforge_project::BUILD_CACHE_FILE),
        layout.out_guide.clone(),
        layout.missing_sub.clone(),
        layout.far_guide.clone(),
        // A file in a missing file's directory changes the E016 suggestion.
        layout.missing_x.clone(),
    ];
    for path in inputs {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "new content").unwrap();
        assert!(
            !session.changes([path.as_path()]).is_empty(),
            "{}",
            path.display()
        );
        let canonical = fs::canonicalize(path.parent().unwrap())
            .map(|dir| dir.join(path.file_name().unwrap()))
            .unwrap();
        assert!(
            roots.iter().any(|watched| canonical.starts_with(watched))
                || canonical.starts_with(&elsewhere),
            "{} is under no watch root of {roots:?}",
            path.display()
        );
    }

    // The missing directory outside the root is watched from its nearest
    // existing ancestor, for that ancestor's own entries.
    assert!(
        session.inputs().watch_roots().contains(&WatchRoot {
            dir: elsewhere.clone(),
            recursive: false
        }),
        "{roots:?}"
    );
}

/// A source that becomes a symlink, or stops being one, moves what a
/// session watches: its update says the inputs changed.
#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a session's watch roots and the LSP's watchers cover every symlinked source's target"
)]
fn relinking_a_source_changes_the_inputs() {
    let (dir, mut session) = open(plain(), &[("a.spec", TERM)]);
    let outside = TempDir::new().unwrap();
    let outside_dir = fs::canonicalize(outside.path()).unwrap();
    fs::write(outside_dir.join("real.spec"), "term real_one \"R\" {}\n").unwrap();
    let keys = ["a.spec".to_string()];

    fs::remove_file(dir.root().join("a.spec")).unwrap();
    std::os::unix::fs::symlink(outside_dir.join("real.spec"), dir.root().join("a.spec")).unwrap();
    let update = disk_changed(&mut session, &keys).unwrap();
    assert!(update.inputs_changed);
    let outside_root = WatchRoot {
        dir: outside_dir.clone(),
        recursive: true,
    };
    assert!(session.inputs().watch_roots().contains(&outside_root));

    // The same update again moves nothing.
    assert!(disk_changed(&mut session, &keys).is_none());

    fs::remove_file(dir.root().join("a.spec")).unwrap();
    fs::write(dir.root().join("a.spec"), TERM).unwrap();
    let update = disk_changed(&mut session, &keys).unwrap();
    assert!(update.inputs_changed);
    assert!(!session.inputs().watch_roots().contains(&outside_root));
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a nested project's specforge.json appearing or going changes every source under its directory"
)]
fn a_nested_projects_marker_changes_the_sources_under_it() {
    let (dir, mut session) = open(
        plain(),
        &[("a.spec", TERM), ("sub/c.spec", "term sub_only \"S\" {}\n")],
    );
    let root = fs::canonicalize(dir.root()).unwrap();
    assert!(session.project().graph().node("sub_only").is_some());

    write(&root, "sub/specforge.json", "{}");
    let marker = root.join("sub/specforge.json");
    let changes = session.changes([marker.as_path()]);
    assert_eq!(changes.sources(), vec!["sub/c.spec"]);
    assert!(session.apply(&changes).is_some());
    assert!(session.project().graph().node("sub_only").is_none());

    fs::remove_file(&marker).unwrap();
    let changes = session.changes([marker.as_path()]);
    assert_eq!(changes.sources(), vec!["sub/c.spec"]);
    assert!(session.apply(&changes).is_some());
    assert!(session.project().graph().node("sub_only").is_some());

    // The project's own specforge.json stays an environment change.
    write(&root, "specforge.json", r#"{"name":"c","version":"0.2.0"}"#);
    let own = root.join("specforge.json");
    let changes = session.changes([own.as_path()]);
    assert!(changes.sources().is_empty(), "{changes:?}");
    assert_eq!(changes.environment(), vec![own]);
}
