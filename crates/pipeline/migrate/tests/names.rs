//! A migration names each file and backup from the project root (ADR 0102), and what reopens a name joins it onto
//! the root.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_common::{ProjectPath, ProjectRoot};
use specforge_migrate::{
    MigrationRecord, MigrationStatus, Original, migrate_project, restore, written_beside,
};
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
fn a_rollback_names_what_it_restores_from_the_root() {
    let dir = project();
    let file = dir.path().join("spec/a.spec");

    let summary = restore(&[Original {
        path: file.clone(),
        name: ProjectPath::of(dir.path(), &file),
        text: "restored\n".to_string(),
    }]);

    assert_eq!(summary.results[0].status, MigrationStatus::Restored);
    assert_eq!(summary.results[0].file_path, "spec/a.spec");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "restored\n");
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "the migrate document names each file and backup from the project root"
)]
fn the_record_reads_a_backup_through_the_root() {
    let dir = project();
    let root = ProjectRoot::of(dir.path()).unwrap();
    let summary = migrate_project(
        &root,
        &[dir.path().join("spec/a.spec")],
        &CURRENT_FORMAT_VERSION,
        false,
        false,
    );
    assert_eq!(summary.backups[0].original_path, "spec/a.spec");
    assert_eq!(summary.backups[0].backup_path, "spec/a.spec.bak");

    let record =
        MigrationRecord::of(dir.path(), &CURRENT_FORMAT_VERSION, &summary.backups).unwrap();

    assert_eq!(record.files[0].path, "spec/a.spec");
    assert_eq!(record.files[0].backup, "spec/a.spec.bak");
    let migrated = std::fs::read_to_string(dir.path().join("spec/a.spec")).unwrap();
    assert!(!migrated.contains("0.9"), "{migrated}");
    assert_eq!(record.files[0].sha256.len(), 64);
}

#[test]
fn written_beside_names_the_backup_and_both_temporary_files() {
    let names = written_beside(std::path::Path::new("spec/a.spec"));
    assert_eq!(
        names.map(|p| p.display().to_string()),
        [
            "spec/a.spec.bak",
            "spec/a.spec.tmp",
            "spec/a.spec.restore.tmp"
        ]
    );
}

#[cfg(unix)]
#[test]
#[ignore = "flipped by T2"]
fn an_inside_link_at_a_temp_name_is_not_written_through() {
    let dir = project();
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "notes\n").unwrap();
    let source = dir.path().join("spec/a.spec");
    std::os::unix::fs::symlink("../notes.txt", dir.path().join("spec/a.spec.tmp")).unwrap();
    let root = ProjectRoot::of(dir.path()).unwrap();

    let summary = migrate_project(
        &root,
        &[source.clone()],
        &CURRENT_FORMAT_VERSION,
        false,
        false,
    );

    assert_eq!(summary.results[0].status, MigrationStatus::Migrated);
    assert_eq!(std::fs::read_to_string(&notes).unwrap(), "notes\n");
    assert!(!std::fs::symlink_metadata(&source).unwrap().is_symlink());
    assert!(!std::fs::read_to_string(&source).unwrap().contains("0.9"));
}
