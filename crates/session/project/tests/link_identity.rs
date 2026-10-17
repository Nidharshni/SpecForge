//! A source whose link chain changed is a change, whatever the stamps say
//! (behavior `invalidate_changed_files`; ADR 0129). Every file gets the
//! same modification time explicitly, so the clock never decides.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use specforge_project::testing::{TestProject, disk_changed};
use specforge_project::{ProjectSession, WatchRoot};
use specforge_test::prelude::*;
use tempfile::TempDir;

const TERM: &str = "term alpha_one \"A\" {}\n";

/// An old time, and a racy one (the content hash decides the stamp).
fn times() -> [SystemTime; 2] {
    [
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000),
        SystemTime::now(),
    ]
}

fn stamp(path: &Path, time: SystemTime) {
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

/// A project whose `a.spec` is a plain file or a link to `link_to`, an outside directory, and its session.
fn project(
    link_to: Option<&str>,
    time: SystemTime,
) -> (TestProject, TempDir, PathBuf, ProjectSession) {
    let dir = TestProject::new()
        .raw_config(serde_json::json!({"name": "c", "version": "0.1.0"}).to_string())
        .files(&[("a.spec", TERM)]);
    let outside = TempDir::new().unwrap();
    let out = fs::canonicalize(outside.path()).unwrap();
    for name in ["real.spec", "other.spec"] {
        fs::write(out.join(name), TERM).unwrap();
        stamp(&out.join(name), time);
    }
    let a = dir.root().join("a.spec");
    stamp(&a, time);
    if let Some(target) = link_to {
        fs::remove_file(&a).unwrap();
        symlink(out.join(target), &a).unwrap();
    }
    let session = dir.session_bare();
    (dir, outside, out, session)
}

fn watches(session: &ProjectSession, dir: &Path) -> bool {
    session.inputs().watch_roots().contains(&WatchRoot {
        dir: dir.to_path_buf(),
        recursive: true,
    })
}

#[specforge_test(
    behavior = "invalidate_changed_files",
    verify = "a source relinked to a file with an equal stamp still moves the inputs"
)]
fn a_source_relinked_to_an_equal_stamp_file_moves_the_inputs() {
    for time in times() {
        let (dir, _outside, out, mut session) = project(None, time);
        let a = dir.root().join("a.spec");
        fs::remove_file(&a).unwrap();
        symlink(out.join("real.spec"), &a).unwrap();

        assert_eq!(session.stale().sources(), ["a.spec"]);
        let update = disk_changed(&mut session, &["a.spec"]).expect("the link is an input");
        assert!(update.inputs_changed);
        assert!(update.rebuilt_files.is_empty());
        assert!(watches(&session, &out));
    }
}

#[test]
fn a_link_retargeted_to_an_equal_stamp_file_moves_the_inputs() {
    for time in times() {
        let (dir, _outside, _out, mut session) = project(Some("real.spec"), time);
        let other_dir = TempDir::new().unwrap();
        let other_dir = fs::canonicalize(other_dir.path()).unwrap();
        let other = other_dir.join("other.spec");
        fs::write(&other, TERM).unwrap();
        stamp(&other, time);
        let a = dir.root().join("a.spec");
        fs::remove_file(&a).unwrap();
        symlink(&other, &a).unwrap();

        let update = disk_changed(&mut session, &["a.spec"]).expect("the new target is an input");
        assert!(update.inputs_changed);
        assert!(watches(&session, &other_dir));

        fs::write(&other, "term alpha_one \"A\" {\n").unwrap();
        assert_eq!(session.changes([other.as_path()]).sources(), ["a.spec"]);
    }
}

#[test]
fn a_hop_retargeted_in_the_middle_of_a_chain_moves_the_inputs() {
    for time in times() {
        let (dir, _outside, out, _) = project(None, time);
        let a = dir.root().join("a.spec");
        let l1 = out.join("l1.spec");
        fs::remove_file(&a).unwrap();
        symlink(&l1, &a).unwrap();
        symlink(out.join("real.spec"), &l1).unwrap();
        // The session is built over the chain a -> l1 -> real.
        let mut session = dir.session_bare();
        fs::remove_file(&l1).unwrap();
        symlink(out.join("other.spec"), &l1).unwrap();

        let update = disk_changed(&mut session, &["a.spec"]).expect("the hop moved");
        assert!(update.inputs_changed);
    }
}
