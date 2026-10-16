//! What a session's check inputs are: the reads its checks made (behavior
//! `classify_project_changes`, ADR 0107). The pins here are ignored until the
//! ticket that fixes them.

use std::fs;
use std::sync::Arc;

use specforge_project::testing::TestProject;
use specforge_project::{ProjectSession, SharedRuntime, UpdateKind, Watched};
use specforge_test::prelude::*;
use tempfile::TempDir;

use crate::check_passes::{passes_extension, project as passes_project};

fn e016(session: &ProjectSession) -> usize {
    session
        .project()
        .diagnostics()
        .iter()
        .filter(|d| d.code == "E016")
        .count()
}

/// A `file_reference` path that is a symlink is read through the link: its
/// target, created or deleted, must re-run the checks.
#[cfg(unix)]
#[ignore = "flipped by T8"]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a check input that is a symlink is followed to its target"
)]
fn a_symlinked_check_inputs_target_reruns_the_checks() {
    let dir: TestProject = passes_project("gadget doc \"Doc\" {\n  docs [\"link.md\"]\n}\n");
    let root = dir.root();
    let outside = TempDir::new().unwrap();
    let outside = fs::canonicalize(outside.path()).unwrap();
    let target = outside.join("target.md");
    std::os::unix::fs::symlink(&target, root.join("link.md")).unwrap();
    let ext = Arc::new(passes_extension());
    let mut session = ProjectSession::open_with_runtime(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(Arc::clone(&ext) as SharedRuntime),
    );
    assert_eq!(e016(&session), 1, "{:?}", session.project().diagnostics());
    assert!(
        session
            .inputs()
            .watch_roots()
            .iter()
            .any(|w| outside.starts_with(&w.dir) || w.dir == outside),
        "{:?}",
        session.inputs().watch_roots()
    );
    assert!(
        session
            .inputs()
            .watched()
            .contains(&Watched::File(target.clone())),
        "{:?}",
        session.inputs().watched()
    );

    fs::write(&target, "# target\n").unwrap();
    let update = session
        .apply(&session.changes([target.as_path()]))
        .expect("the target of a symlinked check input appeared");
    assert_eq!(update.kind, UpdateKind::Checks);
    assert_eq!(e016(&session), 0, "{:?}", session.project().diagnostics());

    fs::remove_file(&target).unwrap();
    let update = session
        .apply(&session.changes([target.as_path()]))
        .expect("the target of a symlinked check input went");
    assert_eq!(update.kind, UpdateKind::Checks);
    assert_eq!(e016(&session), 1, "{:?}", session.project().diagnostics());
}
