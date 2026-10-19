//! A migration's failed run (ADR 0170): `Outcome::failed` answers it as an `OpError` carrying the
//! migration document and what the run left written; a finding fails the verdict only.

use specforge_common::ProjectRoot;
use specforge_ops::OpErrorKind;
use std::sync::Arc;

use crate::migrate::{hooked, project, project_with_extension, request, run};

const BAD: &str = "// specforge-format: 99.0\nbehavior bad \"Bad\" {\n}\n";

#[test]
fn a_file_that_could_not_migrate_is_a_failed_run() {
    let dir = project();
    std::fs::write(dir.path().join("bad.spec"), BAD).unwrap();
    let outcome = run(&request(&ProjectRoot::of(dir.path()).unwrap()), None);

    let error = outcome.failed().expect("a failed run");

    assert_eq!(error.kind, OpErrorKind::SchemaMismatch);
    assert_eq!(error.code, "migration_file_failed");
    let data = error.data.as_deref().expect("the document");
    assert_eq!(data["ok"], false);
    assert_eq!(data["files_failed"], 1);
    assert_eq!(*data, serde_json::to_value(outcome.document()).unwrap());
    assert_eq!(error.writes, outcome.writes);
    assert!(error.suggestion.is_none());
    assert!(!outcome.ok());
}

#[test]
fn a_rolled_back_migration_is_a_failed_run() {
    let dir = project_with_extension();
    let runtime = Arc::new(hooked(|_| Err("trapped".into())));
    let outcome = run(
        &request(&ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime),
    );

    let error = outcome.failed().expect("a failed run");

    assert_eq!(error.kind, OpErrorKind::Internal);
    assert_eq!(error.data.as_deref().unwrap()["rolled_back"], true);
    assert_eq!(error.writes, outcome.writes);
    assert!(error.suggestion.is_none());
    assert!(!outcome.ok());
}

#[test]
fn a_config_problem_the_migration_goes_around_is_no_failed_run() {
    let dir = project();
    std::fs::write(dir.path().join("specforge.json"), r#"{"exclude": ["["]}"#).unwrap();
    let outcome = run(&request(&ProjectRoot::of(dir.path()).unwrap()), None);

    assert!(outcome.failed().is_none());
    assert!(!outcome.ok());
    assert!(!outcome.writes.is_empty(), "the migration was kept");
}

#[test]
fn a_migration_that_kept_everything_is_no_failed_run() {
    let dir = project();
    let outcome = run(&request(&ProjectRoot::of(dir.path()).unwrap()), None);

    assert!(outcome.failed().is_none());
    assert!(outcome.ok());
}
