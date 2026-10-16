//! A one-shot compile and a session open share one read path (ADR 0106 D2, D2 of the plan: a
//! held source is stamped when it is read).

use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;

use specforge_common::ProjectRoot;
use specforge_test_macros::test as specforge_test;

use crate::ledger::{Ledger, Reads, SessionInputs, Unrecorded};
use crate::sources::{self, Read};
use crate::{CompiledProject, Environment};

const OLD: &str = "behavior a \"A\" {\n}\n";
const NEW: &str = "behavior a \"A\" {\n}\n\nbehavior b \"B\" {\n}\n";
const CONFIG: &str = "{\"name\":\"d\",\"version\":\"0.1.0\",\"extensions\":[]}\n";

fn project() -> (tempfile::TempDir, ProjectRoot) {
    let dir = tempfile::TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), CONFIG).unwrap();
    fs::write(dir.path().join("a.spec"), OLD).unwrap();
    fs::create_dir_all(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/b.spec"), OLD).unwrap();
    let root = ProjectRoot::of(dir.path()).unwrap();
    (dir, root)
}

fn text(read: &Read) -> Option<&str> {
    match read {
        Read::Text(text) => Some(text),
        _ => None,
    }
}

#[specforge_test(
    behavior = "bring_session_up_to_date",
    verify = "a source or check input written between its stamp and its read is seen next time"
)]
fn a_source_written_between_its_stamp_and_its_read_is_seen_next_time() {
    let (dir, root) = project();
    let spec_root = dir.path();
    let mut ledger = Ledger::over(SessionInputs::opened(
        root.dir(),
        &specforge_common::ProjectConfig::default(),
        Vec::new(),
    ));
    let read = ledger.source_with(spec_root, "a.spec", |path| {
        fs::write(path, NEW).unwrap();
        sources::read(spec_root, "a.spec")
    });
    assert_eq!(text(&read), Some(NEW));
    // The stamp is the old file's, so the next comparison catches up.
    assert_eq!(ledger.stale().sources, ["a.spec", "sub/b.spec"]);
}

#[test]
fn the_unrecorded_reads_are_the_ledgers() {
    let (_dir, root) = project();
    let mut ledger = Ledger::opening();
    let mut unrecorded = Unrecorded;
    let from_ledger = Environment::read_through(&mut ledger, &root, |_| None);
    let from_unrecorded = Environment::read_through(&mut unrecorded, &root, |_| None);
    assert_eq!(
        format!("{:?}", from_ledger.config()),
        format!("{:?}", from_unrecorded.config())
    );

    let sources = from_ledger.config().sources(root.dir());
    let keys = ledger.discover(&sources);
    assert_eq!(keys, unrecorded.discover(&sources));
    for key in &keys {
        let one = ledger.source(sources.dir(), key);
        let two = unrecorded.source(sources.dir(), key);
        assert_eq!(text(&one), text(&two), "{key}");
    }

    let env = Environment::load(&root, None);
    let mut project = CompiledProject::read(Arc::new(env), &BTreeMap::new(), &mut unrecorded);
    assert!(!project.check(&mut unrecorded));
}

#[test]
fn a_held_key_is_not_stamped_until_it_is_read() {
    let (_dir, root) = project();
    let mut ledger = Ledger::opening();
    let env = Environment::read_through(&mut ledger, &root, |_| None);
    let held = BTreeMap::from([("a.spec".to_string(), NEW)]);
    CompiledProject::read(Arc::new(env), &held, &mut ledger);
    assert_eq!(ledger.stale().sources, ["a.spec"]);
    let (reads, _) = ledger.reread(&["a.spec".to_string()]);
    assert_eq!(text(&reads[0].1), Some(OLD));
    assert!(ledger.stale().is_empty());
}
