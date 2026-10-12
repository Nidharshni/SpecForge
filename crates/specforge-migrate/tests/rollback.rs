//! What `specforge migrate --rollback` restores: the recorded migration, no more.

use specforge_migrate::{MigrationRecord, RecordChange, migrate_project, restore, run_rollback};
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

/// Migrate `dir` with backups and record it, as `specforge migrate` does once its checks pass.
fn migrated(dir: &TempDir) {
    let summary = migrate_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        &CURRENT_FORMAT_VERSION,
        false,
        false,
    );
    assert!(summary.migrated_count >= 1, "{summary:?}");
    MigrationRecord::of(dir.path(), &CURRENT_FORMAT_VERSION, &summary.backups)
        .unwrap()
        .write(dir.path())
        .unwrap();
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

    let rolled = run_rollback(&specforge_common::ProjectRoot::of(dir.path()).unwrap());

    assert_eq!(rolled.restored_count, 0, "{rolled:?}");
    assert_eq!(rolled.skipped_count, 1, "{rolled:?}");
    assert_eq!(std::fs::read_to_string(&a).unwrap(), edited);
    assert!(rolled.warnings[0].contains("spec/a.spec"), "{rolled:?}");
    assert!(rolled.warnings[0].contains("spec/a.spec.bak"), "{rolled:?}");
    assert!(dir.path().join("spec/a.spec.bak").exists());
    let record = MigrationRecord::read(dir.path()).unwrap().unwrap();
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

    let rolled = run_rollback(&specforge_common::ProjectRoot::of(dir.path()).unwrap());

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

    let rolled = run_rollback(&specforge_common::ProjectRoot::of(dir.path()).unwrap());

    assert_eq!(rolled.restored_count, 1, "{rolled:?}");
    assert_eq!(rolled.record, RecordChange::Removed);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("spec/a.spec")).unwrap(),
        OLD
    );
    assert!(dir.path().join("spec/a.spec.bak").exists());
    assert!(MigrationRecord::read(dir.path()).unwrap().is_none());
}

#[test]
fn restore_writes_back_the_texts_a_run_read_without_any_backup() {
    let dir = project();
    let summary = migrate_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        &CURRENT_FORMAT_VERSION,
        false,
        true,
    );
    assert_eq!(summary.originals.len(), 1);
    assert!(!dir.path().join("spec/a.spec.bak").exists());

    let restored = restore(&summary.originals);

    assert_eq!(restored.restored_count, 1);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("spec/a.spec")).unwrap(),
        OLD
    );
}
