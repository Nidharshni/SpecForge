//! A migration names each file and backup from the project root (ADR 0102), and what reopens a name joins it onto
//! the root.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use std::path::Path;

use specforge_common::ProjectRoot;
use specforge_graph::FieldForms;
use specforge_migrate::{MigrationRecord, backup_of, migrate_project};
use specforge_parser::CURRENT_FORMAT_VERSION;
use specforge_test_macros::test as specforge_test;
use tempfile::TempDir;

const OLD: &str = "// specforge-format: 0.9\nbehavior alpha \"Alpha\" {\n}\n";

/// A project with one source, `spec/a.spec`, at format 0.9.
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("spec")).unwrap();
    std::fs::write(dir.path().join("spec/a.spec"), OLD).unwrap();
    dir
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "the migrate document names each file and backup from the project root"
)]
fn the_record_names_each_file_and_its_backup_from_the_root() {
    let dir = project();
    let root = ProjectRoot::of(dir.path()).unwrap();
    let summary = migrate_project(
        &root,
        &[dir.path().join("spec/a.spec")],
        &CURRENT_FORMAT_VERSION,
        &|path: &Path| std::fs::read_to_string(path),
        &FieldForms::default(),
    );
    assert_eq!(summary.rewrites[0].name, "spec/a.spec");

    let record = MigrationRecord::of(dir.path(), &CURRENT_FORMAT_VERSION, &summary.rewrites);

    assert_eq!(record.files[0].path, "spec/a.spec");
    assert_eq!(record.files[0].backup, "spec/a.spec.bak");
    assert!(!summary.rewrites[0].after.contains("0.9"));
    assert_eq!(record.files[0].sha256.len(), 64);
}

#[test]
fn backup_of_names_the_backup_beside_its_source() {
    assert_eq!(
        backup_of(Path::new("spec/a.spec")),
        Path::new("spec/a.spec.bak")
    );
}
