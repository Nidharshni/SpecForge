//! What each update door says about the inputs and about a source whose
//! text did not change (behaviors `hold_editor_buffers`,
//! `invalidate_changed_files`; ADR 0136).

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

#[cfg(unix)]
use specforge_project::WatchRoot;
use specforge_project::testing::{TestProject, disk_changed};
use specforge_project::{Buffer, CheckMode, ProjectSession};
use specforge_test::prelude::*;
use tempfile::TempDir;

use crate::buffers::{check_runs, counting, opened};

const TERM: &str = "term alpha_one \"A\" {}\n";
const ALPHA: &str = "type alpha \"A\" {}\n";

fn plain_project(files: &[(&str, &str)]) -> (TestProject, ProjectSession) {
    let dir = TestProject::new()
        .raw_config(serde_json::json!({"name": "c", "version": "0.1.0"}).to_string())
        .files(files);
    let session = dir.session_bare();
    (dir, session)
}

/// A modification time that differs, the same bytes.
fn touch(path: &Path) {
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    let modified = fs::metadata(path).unwrap().modified().unwrap();
    file.set_modified(modified + Duration::from_secs(3))
        .unwrap();
}

#[cfg(unix)]
#[specforge_test(
    behavior = "hold_editor_buffers",
    verify = "releasing a buffer reads its file through the one read and leaves nothing stale"
)]
fn releasing_a_buffer_whose_file_became_a_symlink_moves_the_inputs() {
    let (dir, mut session) = plain_project(&[("a.spec", TERM)]);
    let a = dir.root().join("a.spec");
    let outside = TempDir::new().unwrap();
    let outside_dir = fs::canonicalize(outside.path()).unwrap();
    fs::write(outside_dir.join("real.spec"), TERM).unwrap();
    // The same bytes, a later modification time: the stamps must differ whatever the clock's granularity.
    touch(&outside_dir.join("real.spec"));

    session.hold(&[Buffer::new(&a, TERM)]);
    fs::remove_file(&a).unwrap();
    std::os::unix::fs::symlink(outside_dir.join("real.spec"), &a).unwrap();
    let update = session
        .release(std::slice::from_ref(&a))
        .expect("a link hop is an input");
    assert!(update.inputs_changed);
    assert!(update.rebuilt_files.is_empty());
    assert!(session.inputs().watch_roots().contains(&WatchRoot {
        dir: outside_dir,
        recursive: true,
    }));
    // Released again: the same hop, the same text.
    assert!(session.release(&[a]).is_none());
}

#[test]
fn an_update_that_changes_nothing_holds_one_list_twice() {
    let (dir, mut session) = opened(&counting());
    let a = dir.root().join("a.spec");
    let update = session.hold(&[Buffer::new(&a, ALPHA)]);
    assert!(Arc::ptr_eq(
        &update.previous_diagnostics,
        &update.diagnostics
    ));
    assert!(!update.inputs_changed);

    // An update that ran the checks holds two lists.
    let broken = "type alpha \"A\" {\n  contract \"\n";
    session.hold_with(
        &[Buffer::new(&a, broken)],
        CheckMode::SyntaxOnlyIfParseErrors,
    );
    fs::write(&a, broken).unwrap();
    let update = session.release(&[a]).expect("the skipped checks run");
    assert!(!Arc::ptr_eq(
        &update.previous_diagnostics,
        &update.diagnostics
    ));
}

#[specforge_test(
    behavior = "invalidate_changed_files",
    verify = "a source rewritten with its own text re-parses nothing and runs no check"
)]
fn a_source_rewritten_with_its_own_text_changes_nothing() {
    let rt = counting();
    let (dir, mut session) = opened(&rt);
    let a = dir.root().join("a.spec");

    for rewrite in [false, true] {
        if rewrite {
            fs::write(&a, ALPHA).unwrap();
        }
        touch(&a);
        assert_eq!(session.stale().sources(), ["a.spec"]);
        let before = check_runs(&rt);
        let update = disk_changed(&mut session, &["a.spec"]);
        assert!(update.is_none(), "{:?}", update.map(|u| u.rebuilt_files));
        assert_eq!(check_runs(&rt) - before, 0);
        assert!(session.stale().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn a_source_relinked_to_its_own_text_moves_the_inputs_and_rebuilds_nothing() {
    let (dir, mut session) = plain_project(&[("a.spec", TERM)]);
    let a = dir.root().join("a.spec");
    let outside = TempDir::new().unwrap();
    let outside_dir = fs::canonicalize(outside.path()).unwrap();
    fs::write(outside_dir.join("real.spec"), TERM).unwrap();
    // The same bytes, a later modification time: the stamps must differ whatever the clock's granularity.
    touch(&outside_dir.join("real.spec"));
    fs::remove_file(&a).unwrap();
    std::os::unix::fs::symlink(outside_dir.join("real.spec"), &a).unwrap();

    let update = disk_changed(&mut session, &["a.spec"]).expect("the link is an input");
    assert!(update.inputs_changed);
    assert!(
        update.rebuilt_files.is_empty(),
        "{:?}",
        update.rebuilt_files
    );
}
