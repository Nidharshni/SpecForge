//! What `specforge migrate --rollback` restores: the recorded migration, no more.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_migrate::backup_of;
use specforge_migrate::{MigrationRecord, RecordChange, migrate_project, run_rollback};
use specforge_parser::CURRENT_FORMAT_VERSION;
use specforge_test_macros::test as specforge_test;
use std::path::Path;
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

/// Migrate `dir` with backups and record it, as `specforge migrate` does once its checks pass: the plan's texts are
/// written, each backup with the text it read, and the record.
fn migrated(dir: &TempDir) {
    let summary = migrate_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        &[dir.path().join("spec/a.spec")],
        &CURRENT_FORMAT_VERSION,
        &|path: &Path| std::fs::read_to_string(path),
    );
    assert!(summary.migrated_count >= 1, "{summary:?}");
    let rewrite = &summary.rewrites[0];
    std::fs::write(&rewrite.path, &rewrite.after).unwrap();
    std::fs::write(backup_of(&rewrite.path), &rewrite.before).unwrap();
    let record = MigrationRecord::of(dir.path(), &CURRENT_FORMAT_VERSION, &summary.rewrites);
    std::fs::create_dir_all(dir.path().join(".specforge")).unwrap();
    std::fs::write(dir.path().join(MigrationRecord::PATH), record.render()).unwrap();
}

/// The project's record, as ops reads it.
fn record(dir: &TempDir) -> Result<Option<MigrationRecord>, String> {
    match std::fs::read_to_string(dir.path().join(MigrationRecord::PATH)) {
        Ok(text) => MigrationRecord::parse(&text).map(Some),
        Err(_) => Ok(None),
    }
}

/// `run_rollback` over the plain disk.
fn rolled_back(dir: &TempDir) -> specforge_migrate::RollbackSummary {
    run_rollback(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        record(dir),
        &|path: &Path| std::fs::read_to_string(path),
        &|path: &Path, text: &str| std::fs::write(path, text).map_err(|e| e.to_string()),
    )
}

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "a file edited since the migration is left as it is, its backup kept"
)]
fn a_file_edited_since_the_migration_is_left_as_it_is() {
    let dir = project();
    migrated(&dir);
    let a = dir.path().join("spec/a.spec");
    let edited = format!("{}// edited\n", std::fs::read_to_string(&a).unwrap());
    std::fs::write(&a, &edited).unwrap();

    let rolled = rolled_back(&dir);

    assert_eq!(rolled.restored_count, 0, "{rolled:?}");
    assert_eq!(rolled.skipped_count, 1, "{rolled:?}");
    assert_eq!(std::fs::read_to_string(&a).unwrap(), edited);
    assert!(rolled.warnings[0].contains("spec/a.spec"), "{rolled:?}");
    assert!(rolled.warnings[0].contains("spec/a.spec.bak"), "{rolled:?}");
    assert!(dir.path().join("spec/a.spec.bak").exists());
    let record = record(&dir).unwrap().unwrap();
    assert_eq!(record.files[0].path, "spec/a.spec");
    assert_eq!(rolled.record, RecordChange::Unchanged);
}

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "a rollback with no recorded migration restores nothing"
)]
fn a_rollback_with_no_recorded_migration_restores_nothing() {
    let dir = project();
    let a = dir.path().join("spec/a.spec");
    std::fs::write(dir.path().join("spec/a.spec.bak"), "stale").unwrap();

    let rolled = rolled_back(&dir);

    assert_eq!(rolled.restored_count, 0, "{rolled:?}");
    assert_eq!(
        rolled.warnings,
        ["nothing to roll back: no migration is recorded (.specforge/migration.json)"]
    );
    assert_eq!(std::fs::read_to_string(&a).unwrap(), OLD);
}

#[test]
fn a_full_rollback_removes_the_record() {
    let dir = project();
    migrated(&dir);

    let rolled = rolled_back(&dir);

    assert_eq!(rolled.restored_count, 1, "{rolled:?}");
    assert_eq!(rolled.record, RecordChange::Removed);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("spec/a.spec")).unwrap(),
        OLD
    );
    assert!(dir.path().join("spec/a.spec.bak").exists());
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "each file is read once: its backup, its diff and what a rollback restores are that text"
)]
fn a_migrated_file_is_read_once() {
    use sha2::Digest;
    let dir = project();
    let reads = std::cell::Cell::new(0);

    let summary = migrate_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        &[dir.path().join("spec/a.spec")],
        &CURRENT_FORMAT_VERSION,
        &|path: &Path| {
            reads.set(reads.get() + 1);
            std::fs::read_to_string(path)
        },
    );

    assert_eq!(reads.get(), 1);
    assert_eq!(summary.rewrites.len(), 1);
    assert_eq!(summary.rewrites[0].before, OLD);
    assert_eq!(
        summary.diffs[0].before_hash,
        format!("{:x}", sha2::Sha256::digest(OLD.as_bytes()))
    );
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "the preview and the migration rewrite the same sources, read once"
)]
fn migrate_project_rewrites_only_the_sources_it_is_given() {
    let dir = project();
    std::fs::write(dir.path().join("spec/b.spec"), OLD).unwrap();
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let given = [dir.path().join("spec/a.spec")];

    let summary = migrate_project(&root, &given, &CURRENT_FORMAT_VERSION, &|path: &Path| {
        std::fs::read_to_string(path)
    });

    assert_eq!(summary.results.len(), 1, "{summary:?}");
    assert_eq!(summary.results[0].file_path, "spec/a.spec");
    assert_eq!(summary.rewrites.len(), 1);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("spec/b.spec")).unwrap(),
        OLD,
        "a file not handed over keeps its header"
    );
}
