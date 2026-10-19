//! A migration is one project change over held files (ADR 0171): a source keeps its permissions, a
//! backup has its source's, a file the process may not write refuses the run, and a rollback writes
//! each file the way a typed edit does.

use specforge_common::ProjectRoot;
use specforge_ops::OpErrorKind;
use specforge_ops::authoring::migrate::{MigrationStatus, rollback};
use specforge_test_macros::test as specforge_test;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::sync::Arc;

use crate::migrate::{OLD, hooked, project, project_with_extension, request, run};

/// Whether a 0444 file binds this user (not root): a test of a refusal returns early when it does not.
fn binds(dir: &Path) -> bool {
    let probe = dir.join("probe");
    std::fs::write(&probe, "x").unwrap();
    chmod(&probe, 0o444);
    let binds = std::fs::OpenOptions::new()
        .write(true)
        .open(&probe)
        .is_err();
    std::fs::remove_file(&probe).unwrap();
    binds
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn chmod(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
#[ignore = "flipped by T5"]
fn a_migrated_source_keeps_its_permissions() {
    let dir = project();
    if !binds(dir.path()) {
        return;
    }
    chmod(&dir.path().join("old.spec"), 0o600);
    let outcome = run(&request(&ProjectRoot::of(dir.path()).unwrap()), None);
    assert!(outcome.ok());
    assert_eq!(mode(&dir.path().join("old.spec")), 0o600);
}

#[test]
#[ignore = "flipped by T5"]
fn a_backup_has_its_sources_permissions() {
    let dir = project();
    if !binds(dir.path()) {
        return;
    }
    chmod(&dir.path().join("old.spec"), 0o600);
    let outcome = run(&request(&ProjectRoot::of(dir.path()).unwrap()), None);
    assert!(outcome.ok());
    assert_eq!(mode(&dir.path().join("old.spec.bak")), 0o600);
}

#[test]
#[ignore = "flipped by T5"]
fn a_read_only_source_refuses_the_migration_and_nothing_is_written() {
    let dir = project();
    if !binds(dir.path()) {
        return;
    }
    for no_backup in [false, true] {
        let source = dir.path().join("old.spec");
        chmod(&source, 0o444);
        let root = ProjectRoot::of(dir.path()).unwrap();
        let mut req = request(&root);
        req.no_backup = no_backup;
        let error = specforge_ops::authoring::migrate::run(&req, None).unwrap_err();
        assert_eq!(error.code, "file_unwritable");
        assert_eq!(error.kind, OpErrorKind::PermissionDenied);
        assert!(
            error
                .message
                .starts_with("cannot migrate: failed to write old.spec")
                && error.message.ends_with("nothing was written"),
            "{}",
            error.message
        );
        assert!(error.writes.is_empty());
        assert_eq!(std::fs::read_to_string(&source).unwrap(), OLD);
        assert_eq!(mode(&source), 0o444);
        assert!(!dir.path().join("old.spec.bak").exists());
        assert!(!dir.path().join(".specforge/migration.json").exists());
    }
}

#[test]
#[ignore = "flipped by T5"]
fn a_record_the_migration_cannot_write_refuses_it() {
    let dir = project();
    if !binds(dir.path()) {
        return;
    }
    let record = dir.path().join(".specforge/migration.json");
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, "{\"target\":\"1.0\",\"files\":[]}\n").unwrap();
    chmod(&record, 0o444);
    let root = ProjectRoot::of(dir.path()).unwrap();
    let error = specforge_ops::authoring::migrate::run(&request(&root), None).unwrap_err();
    assert_eq!(error.code, "file_unwritable");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("old.spec")).unwrap(),
        OLD
    );
    assert!(!dir.path().join("old.spec.bak").exists());
    assert_eq!(
        std::fs::read_to_string(&record).unwrap(),
        "{\"target\":\"1.0\",\"files\":[]}\n"
    );
}

#[test]
#[ignore = "flipped by T5"]
fn an_automatic_rollback_keeps_a_files_permissions() {
    let dir = project_with_extension();
    if !binds(dir.path()) {
        return;
    }
    let source = dir.path().join("old.spec");
    chmod(&source, 0o600);
    let runtime = Arc::new(hooked(|_| Err("trapped".into())));
    let outcome = run(
        &request(&ProjectRoot::of(dir.path()).unwrap()),
        Some(runtime),
    );
    assert!(outcome.rollback.is_some());
    assert_eq!(std::fs::read_to_string(&source).unwrap(), OLD);
    assert_eq!(mode(&source), 0o600);
}

#[test]
#[ignore = "flipped by T4"]
fn a_rollback_keeps_a_files_permissions_and_refuses_a_read_only_one() {
    let dir = project();
    if !binds(dir.path()) {
        return;
    }
    let other = dir.path().join("other.spec");
    std::fs::write(&other, OLD.replace("alpha", "beta")).unwrap();
    let root = ProjectRoot::of(dir.path()).unwrap();
    assert!(run(&request(&root), None).ok());
    let migrated = std::fs::read_to_string(&other).unwrap();
    chmod(&dir.path().join("old.spec"), 0o600);
    chmod(&other, 0o444);

    let outcome = rollback(&root);

    assert_eq!(
        std::fs::read_to_string(dir.path().join("old.spec")).unwrap(),
        OLD
    );
    assert_eq!(mode(&dir.path().join("old.spec")), 0o600);
    assert_eq!(std::fs::read_to_string(&other).unwrap(), migrated);
    assert_eq!(mode(&other), 0o444);
    assert_eq!(outcome.summary.failed_count, 1);
    let failed = outcome
        .summary
        .results
        .iter()
        .find(|r| r.status == MigrationStatus::Failed)
        .expect("a failed restore");
    assert!(
        failed
            .error
            .as_deref()
            .unwrap()
            .contains("Permission denied")
    );
    let record = std::fs::read_to_string(dir.path().join(".specforge/migration.json")).unwrap();
    assert!(record.contains("other.spec"), "{record}");
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a file that cannot be migrated leaves every file as it was"
)]
fn a_file_that_cannot_be_migrated_leaves_every_file_as_it_was() {
    let dir = project();
    std::fs::write(
        dir.path().join("bad.spec"),
        "// specforge-format: 99.0\nbehavior bad \"Bad\" {\n}\n",
    )
    .unwrap();
    let before = crate::migrate::files_under(dir.path());

    let outcome = run(&request(&ProjectRoot::of(dir.path()).unwrap()), None);

    assert!(matches!(
        outcome.failure(),
        Some(specforge_ops::authoring::migrate::Failure::FilesFailed { .. })
    ));
    assert!(outcome.writes.is_empty());
    assert!(!outcome.migrated());
    assert_eq!(crate::migrate::files_under(dir.path()), before);
}
