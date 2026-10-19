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

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a migrated source keeps its permissions"
)]
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

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a backup has its source's permissions"
)]
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

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a read-only source refuses the migration and nothing is written"
)]
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

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a record the migration cannot write refuses it"
)]
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

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "an automatic rollback keeps a file's permissions"
)]
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

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "a rollback keeps a file's permissions and refuses a read-only one"
)]
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

    let outcome = rollback(&root, specforge_ops::testing::caller()).unwrap();

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

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a source changed since the migration read it refuses the migration"
)]
fn a_source_changed_since_the_plan_refuses_the_migration() {
    use specforge_extension_sdk::prelude::*;
    let dir = project_with_extension();
    let source = dir.path().join("old.spec");
    // The compile before the change runs after the plan read the source: a save lands in between.
    let runtime = specforge_wasm::testing::InProcessRuntime::new().with({
        let source = source.clone();
        move || {
            let source = source.clone();
            let mut c = ContributionsBuilder::new(ExtensionMeta::new("@acme/x", "1.0.0"));
            c.kind("behavior", |k| {
                k.description("probe").open_fields(true);
            });
            c.rule("P100", move |r| {
                let source = source.clone();
                r.check(CheckKind::Custom)
                    .severity(ValidationSeverity::Info)
                    .target_kind("behavior")
                    .wasm_function("validate__edit")
                    .message_template("{id}")
                    .validate(move |_| {
                        std::fs::write(&source, "EDITED").unwrap();
                        ValidatorVerdict::Pass
                    });
            });
            c
        }
    });
    let root = ProjectRoot::of(dir.path()).unwrap();

    let error = specforge_ops::authoring::migrate::run(&request(&root), Some(Arc::new(runtime)))
        .unwrap_err();

    assert_eq!(error.code, "file_changed");
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "EDITED");
    assert!(!dir.path().join("old.spec.bak").exists());
    assert!(!dir.path().join(".specforge/migration.json").exists());
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "the backup and temporary file are created new, never through a planted link"
)]
fn an_inside_link_at_a_temp_name_is_not_written_through() {
    let dir = project();
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "notes\n").unwrap();
    std::os::unix::fs::symlink("notes.txt", dir.path().join("old.spec.tmp")).unwrap();

    let outcome = run(&request(&ProjectRoot::of(dir.path()).unwrap()), None);

    assert!(outcome.ok());
    assert_eq!(std::fs::read_to_string(&notes).unwrap(), "notes\n");
    assert!(!dir.path().join("old.spec.tmp").exists());
    assert!(
        !std::fs::read_to_string(dir.path().join("old.spec"))
            .unwrap()
            .contains("0.9")
    );
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "the backup and temporary file are created new, never through a planted link"
)]
fn an_inside_link_at_the_backup_name_is_not_written_through() {
    let dir = project();
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "notes\n").unwrap();
    let backup = dir.path().join("old.spec.bak");
    std::os::unix::fs::symlink("notes.txt", &backup).unwrap();

    let outcome = run(&request(&ProjectRoot::of(dir.path()).unwrap()), None);

    assert!(outcome.ok());
    assert_eq!(std::fs::read_to_string(&notes).unwrap(), "notes\n");
    assert!(!std::fs::symlink_metadata(&backup).unwrap().is_symlink());
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), OLD);
}

#[test]
fn two_names_for_one_source_are_written_once() {
    let dir = project();
    std::os::unix::fs::symlink("old.spec", dir.path().join("link.spec")).unwrap();

    let outcome = run(&request(&ProjectRoot::of(dir.path()).unwrap()), None);

    assert!(outcome.ok(), "{outcome:?}");
    assert!(
        !std::fs::read_to_string(dir.path().join("old.spec"))
            .unwrap()
            .contains("0.9")
    );
    assert!(
        std::fs::symlink_metadata(dir.path().join("link.spec"))
            .unwrap()
            .is_symlink()
    );
}
