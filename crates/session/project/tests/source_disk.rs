//! A surface hands a session the disk it reads sources through (ADR 0145): a source the disk refuses
//! is that source's E025, absent from the graph, still stamped; the environment and the checks are
//! read plain; a reload keeps the disk. Modification times are set explicitly, never the clock.
#![cfg(unix)]

use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use specforge_project::testing::TestProject;
use specforge_project::{ProjectSession, RuntimeSource, SourceDisk};
use specforge_registry::{Disk, PlainDisk};
use specforge_test::prelude::*;
use tempfile::TempDir;

const TERM: &str = "term alpha_one \"A\" {}\n";

/// Refuses a name that lands outside `root`, as a contained disk does.
struct Bounded(std::path::PathBuf);

impl Disk for Bounded {
    fn exists(&self, path: &Path) -> bool {
        PlainDisk.exists(path)
    }
    fn list(&self, dir: &Path) -> Option<Vec<String>> {
        PlainDisk.list(dir)
    }
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match fs::canonicalize(path) {
            Ok(real) if !real.starts_with(&self.0) => {
                Err(io::Error::new(io::ErrorKind::InvalidInput, "outside"))
            }
            _ => PlainDisk.read(path),
        }
    }
}

/// Refuses every read and every listing.
struct Refusing;

impl Disk for Refusing {
    fn exists(&self, _: &Path) -> bool {
        false
    }
    fn list(&self, _: &Path) -> Option<Vec<String>> {
        None
    }
    fn read(&self, _: &Path) -> io::Result<Vec<u8>> {
        Err(io::Error::new(io::ErrorKind::InvalidInput, "refused"))
    }
}

fn set_time(path: &Path, secs: u64) {
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
}

/// A project whose `a.spec` is a link to an outside file, and its session read through `disk`.
fn linked_outside(disk: impl Fn(&Path) -> SourceDisk) -> (TestProject, TempDir, ProjectSession) {
    let dir = TestProject::new()
        .raw_config(serde_json::json!({"name": "c", "version": "0.1.0"}).to_string())
        .files(&[("b.spec", "term beta_one \"B\" {}\n")]);
    let outside = TempDir::new().unwrap();
    let real = fs::canonicalize(outside.path()).unwrap().join("real.spec");
    fs::write(&real, TERM).unwrap();
    symlink(&real, dir.root().join("a.spec")).unwrap();
    let root = fs::canonicalize(dir.root()).unwrap();
    let session = ProjectSession::open_through(
        &dir.project_root(),
        RuntimeSource::Fixed(None),
        Some(disk(&root)),
    );
    (dir, outside, session)
}

fn bounded(root: &Path) -> SourceDisk {
    Arc::new(Bounded(root.to_path_buf()))
}

fn codes(session: &ProjectSession) -> Vec<String> {
    session
        .project()
        .diagnostics()
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_source_the_handed_disk_refuses_is_unreadable_and_not_in_the_graph() {
    let (_dir, _outside, session) = linked_outside(bounded);
    assert!(codes(&session).contains(&"E025".to_string()), "{:?}", codes(&session));
    assert!(session.project().graph().node("alpha_one").is_none());
    assert!(session.project().graph().node("beta_one").is_some());
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_refused_source_keeps_its_stamp_and_hops() {
    let (dir, _outside, mut session) = linked_outside(bounded);
    let a = dir.root().join("a.spec");
    fs::remove_file(&a).unwrap();
    fs::write(&a, TERM).unwrap();
    set_time(&a, 1_000_000_000);

    assert_eq!(session.stale().sources(), ["a.spec"]);
    session.ensure_fresh().expect("the source changed");
    assert!(session.project().graph().node("alpha_one").is_some());
    assert!(!codes(&session).contains(&"E025".to_string()));
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn the_handed_disk_does_not_read_the_environment_or_the_checks() {
    let (_dir, _outside, session) = linked_outside(|_| Arc::new(Refusing));
    assert_eq!(session.project().environment().config().name, "c");
    assert!(session.project().graph().node("beta_one").is_none());
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn an_environment_reload_keeps_the_handed_disk() {
    let (dir, _outside, mut session) = linked_outside(bounded);
    let config = dir.root().join("specforge.json");
    fs::write(
        &config,
        serde_json::json!({"name": "d", "version": "0.1.0"}).to_string(),
    )
    .unwrap();
    set_time(&config, 1_000_000_000);

    session.ensure_fresh().expect("the config changed");
    assert_eq!(session.project().environment().config().name, "d");
    assert!(codes(&session).contains(&"E025".to_string()));
    assert!(session.project().graph().node("alpha_one").is_none());
}
