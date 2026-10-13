//! What a batch of changed paths is to a project session (behavior
//! `classify_project_changes`): the comparison `stale` makes, narrowed to
//! what the paths reach. These pins drive directory, symlink and
//! unchanged-file events: the bug pins are ignored until the ticket that
//! fixes them.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use specforge_project::{
    Buffer, CompiledProject, ProjectSession, SourceChange, WatchRoot, Watched,
};
use specforge_test::prelude::*;
use tempfile::TempDir;

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

fn plain() -> serde_json::Value {
    json!({"name": "c", "version": "0.1.0"})
}

/// The session agrees with a fresh compile of what is on disk now.
fn assert_matches_a_fresh_compile(session: &ProjectSession, root: &Path) {
    let fresh = CompiledProject::compile(&specforge_common::ProjectRoot::of(root).unwrap(), None);
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
#[ignore = "fixed by T3"]
fn a_directory_moved_inside_the_spec_root_moves_its_sources() {
    let (dir, mut session) = open(plain(), &[("r.spec", ""), ("old/a.spec", TERM)]);
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::rename(root.join("old"), root.join("new")).unwrap();

    let changes = session.changes([root.join("new").as_path(), root.join("old").as_path()]);
    assert_eq!(changes.sources, vec!["new/a.spec", "old/a.spec"]);
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
#[ignore = "fixed by T3"]
fn a_directory_moved_in_from_outside_adds_its_sources() {
    let (dir, session) = open(plain(), &[("r.spec", "")]);
    let root = fs::canonicalize(dir.path()).unwrap();
    let outside = TempDir::new().unwrap();
    write(outside.path(), "pkg/g.spec", TERM);
    fs::rename(outside.path().join("pkg"), root.join("pkg")).unwrap();

    let changes = session.changes([root.join("pkg").as_path()]);
    assert_eq!(changes.sources, vec!["pkg/g.spec"]);
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory created, moved or deleted under the spec root changes every source under it"
)]
#[ignore = "fixed by T3"]
fn a_directory_moved_out_removes_its_sources() {
    let (dir, session) = open(plain(), &[("r.spec", ""), ("d/e/x.spec", TERM)]);
    let root = fs::canonicalize(dir.path()).unwrap();
    let outside = TempDir::new().unwrap();
    fs::rename(root.join("d"), outside.path().join("d")).unwrap();

    let changes = session.changes([root.join("d").as_path()]);
    assert_eq!(changes.sources, vec!["d/e/x.spec"]);
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory created, moved or deleted under the spec root changes every source under it"
)]
#[ignore = "fixed by T3"]
fn a_directory_reported_alone_brings_the_sources_written_in_it() {
    let (dir, session) = open(plain(), &[("r.spec", "")]);
    let root = fs::canonicalize(dir.path()).unwrap();
    write(&root, "n/m/a.spec", TERM);
    write(&root, "n/m/b.spec", "term beta_one \"B\" {}\n");

    // inotify can report the new directory alone: the files written in it
    // before its watch was added are never reported.
    let changes = session.changes([root.join("n").as_path()]);
    assert_eq!(changes.sources, vec!["n/m/a.spec", "n/m/b.spec"]);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a symlinked source is keyed by its link and changes when its target does"
)]
#[ignore = "fixed by T3"]
fn a_symlink_inside_the_spec_root_is_keyed_by_its_own_name() {
    let (dir, mut session) = open(
        plain(),
        &[
            ("shared/r.spec", "term root_one \"R\" {}\n"),
            ("r.spec", "term root_one \"R\" {}\n"),
        ],
    );
    let root = fs::canonicalize(dir.path()).unwrap();
    std::os::unix::fs::symlink("shared/r.spec", root.join("alias.spec")).unwrap();

    let changes = session.changes([root.join("alias.spec").as_path()]);
    assert_eq!(changes.sources, vec!["alias.spec"]);
    assert!(session.apply(&changes).is_some());
    assert_matches_a_fresh_compile(&session, &root);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a symlinked source is keyed by its link and changes when its target does"
)]
#[ignore = "fixed by T4"]
fn an_edit_to_a_symlinked_sources_target_changes_that_source() {
    let (_dir, session, outside) = linked_project();
    fs::write(
        outside.join("real.spec"),
        "term real_one \"R\" {}\nterm real_two \"R\" {}\n",
    )
    .unwrap();

    let changes = session.changes([outside.join("real.spec").as_path()]);
    assert_eq!(changes.sources, vec!["link.spec"]);
}

/// A project whose `link.spec` is a symlink to `real.spec` in a directory
/// outside it: (project, opened session, canonical outside directory).
#[cfg(unix)]
fn linked_project() -> (TempDir, ProjectSession, PathBuf) {
    let outside = TempDir::new().unwrap();
    let outside_dir = fs::canonicalize(outside.path()).unwrap();
    // Kept alive for the test: the directory is leaked into the process.
    std::mem::forget(outside);
    fs::write(outside_dir.join("real.spec"), "term real_one \"R\" {}\n").unwrap();
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), plain().to_string()).unwrap();
    fs::write(dir.path().join("a.spec"), "").unwrap();
    std::os::unix::fs::symlink(outside_dir.join("real.spec"), dir.path().join("link.spec"))
        .unwrap();
    let session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        None,
    );
    (dir, session, outside_dir)
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a path whose file is as the session last read it changes nothing"
)]
#[ignore = "fixed by T3"]
fn a_path_whose_file_is_as_read_changes_nothing() {
    let (dir, session) = open(plain(), &[("a.spec", TERM)]);
    let root = fs::canonicalize(dir.path()).unwrap();

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
#[ignore = "fixed by T3"]
fn a_detached_session_takes_no_change_from_disk() {
    let session = ProjectSession::detached();
    let dir = TempDir::new().unwrap();
    write(dir.path(), "b.spec", TERM);

    assert!(
        session
            .changes([dir.path().join("b.spec").as_path()])
            .is_empty()
    );
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a session's watch roots and the LSP's watchers cover every symlinked source's target"
)]
#[ignore = "fixed by T4"]
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
    verify = "a directory moved above an environment or check input changes that input"
)]
#[ignore = "fixed by T3"]
fn a_moved_directory_holding_a_module_reloads_the_environment() {
    let (dir, session) = open(
        json!({"name": "c", "version": "0.1.0", "extensions": ["@acme/local=ext/local.wasm"]}),
        &[("ext/local.wasm", "not wasm")],
    );
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::rename(root.join("ext"), root.join("ext2")).unwrap();

    let changes = session.changes([root.join("ext").as_path(), root.join("ext2").as_path()]);
    // Today a bool; the ticket that fixes it makes it `[root/ext]`.
    assert!(changes.environment);
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory created, moved or deleted under the spec root changes every source under it"
)]
#[ignore = "fixed by T3"]
fn a_held_file_under_a_moved_directory_is_left_out() {
    let (dir, mut session) = open(
        plain(),
        &[
            ("old/a.spec", TERM),
            ("old/b.spec", "term beta_one \"B\" {}\n"),
        ],
    );
    let root = fs::canonicalize(dir.path()).unwrap();
    session.update(SourceChange::Hold(&[Buffer::new(
        root.join("old/a.spec"),
        "term held_one \"H\" {}\n".to_string(),
    )]));
    fs::rename(root.join("old"), root.join("new")).unwrap();

    let changes = session.changes([root.join("new").as_path(), root.join("old").as_path()]);
    assert_eq!(
        changes.sources,
        vec!["new/a.spec", "new/b.spec", "old/b.spec"]
    );
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory created, moved or deleted under the spec root changes every source under it"
)]
fn a_batch_of_many_file_events_is_one_update() {
    let files: Vec<String> = (0..40).map(|n| format!("feat/f{n:02}.spec")).collect();
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), plain().to_string()).unwrap();
    for (n, path) in files.iter().enumerate() {
        write(dir.path(), path, &format!("term feat_{n:02} \"T\" {{}}\n"));
    }
    let mut session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        None,
    );
    let root = fs::canonicalize(dir.path()).unwrap();

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
    assert_eq!(changes.sources.len(), 80, "{changes:?}");
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

    assert_eq!(session.stale().sources, vec!["link.spec"]);
}
