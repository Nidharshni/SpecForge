use assert_cmd::Command;
use predicates::prelude::*;
use specforge_test_macros::test as specforge_test;
use std::fs;
use tempfile::TempDir;

fn setup_project(dir: &std::path::Path) {
    fs::write(
        dir.join("specforge.json"),
        r#"{"name":"test","version":"0.1.0"}"#,
    )
    .unwrap();
    let spec_dir = dir.join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
}

fn write_spec(dir: &std::path::Path, name: &str, content: &str) {
    let spec_dir = dir.join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
    fs::write(spec_dir.join(name), content).unwrap();
}

// ===================================================================
// Phase A: CLI Skeleton + Types
// ===================================================================

// A1: `specforge migrate` exits 0 on project with no version mismatches
#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "files already at target version are skipped with skippedCount incremented"
)]
fn migrate_exits_zero_on_current_version_project() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    // Files without a version header default to current version → skipped
    write_spec(
        root,
        "test.spec",
        "behavior foo \"Foo\" {\n  contract \"does stuff\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("0 migrated, 1 skipped, 0 failed"));

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["pending"], false, "{json}");
    assert_eq!(json["files_skipped"], 1, "{json}");
    assert_eq!(json["files_migrated"], 0, "{json}");
    assert_eq!(json["results"][0]["status"], "skipped", "{json}");
    assert_eq!(
        fs::read_to_string(root.join("spec/test.spec")).unwrap(),
        "behavior foo \"Foo\" {\n  contract \"does stuff\"\n}\n",
        "a skipped file is left alone"
    );
}

// A2: MigrationSummary serializes to JSON
#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "summary reports migrated, failed, and skipped counts"
)]
fn migrate_json_output_contains_summary_fields() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    // One file per outcome: an old header migrates, a current one is
    // skipped, an unsupported one fails.
    write_spec(
        root,
        "old.spec",
        "// specforge-format: 0.1\nbehavior old_one \"Old\" {\n  contract \"stuff\"\n}\n",
    );
    write_spec(
        root,
        "current.spec",
        "// specforge-format: 1.0\nbehavior current_one \"Current\" {\n  contract \"stuff\"\n}\n",
    );
    write_spec(
        root,
        "future.spec",
        "// specforge-format: 99.0\nbehavior future_one \"Future\" {\n  contract \"stuff\"\n}\n",
    );

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("failed to parse JSON: {e}\nstdout: {stdout}"));

    assert_eq!(json["files_migrated"], 1, "{json}");
    assert_eq!(json["files_skipped"], 1, "{json}");
    assert_eq!(json["files_failed"], 1, "{json}");
    assert_eq!(status_of(&json, "old.spec"), "migrated");
    assert_eq!(status_of(&json, "current.spec"), "skipped");
    assert_eq!(status_of(&json, "future.spec"), "failed");
    assert_eq!(json["backups"].as_array().unwrap().len(), 1, "{json}");
    assert_eq!(output.status.code(), Some(1), "a failed file fails the run");
}

/// The result entry of the file whose path ends with `file`.
fn result_for(json: &serde_json::Value, file: &str) -> serde_json::Value {
    json["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["file_path"].as_str().unwrap().ends_with(file))
        .cloned()
        .unwrap_or_else(|| panic!("no result for {file}: {json}"))
}

fn status_of(json: &serde_json::Value, file: &str) -> String {
    result_for(json, file)["status"]
        .as_str()
        .unwrap()
        .to_string()
}

// A3: Unknown --target-version produces error
#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "unsupported format version produces E019 with upgrade guidance"
)]
fn migrate_unknown_target_version_produces_error() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "test.spec",
        "behavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--target-version=99.0",
            "--path",
            root.to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "error[E019]: unsupported target version 99.0",
        ))
        .stderr(predicate::str::contains(
            "Use a format version between 1.0 and 1.0.",
        ));
    assert_eq!(
        fs::read_to_string(root.join("spec/test.spec")).unwrap(),
        "behavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
        "nothing is migrated"
    );
}

// A target version that doesn't parse is the same E019, not another
// owner's code (it used to print E015, @specforge/product's milestone cycle).
#[test]
fn migrate_unparseable_target_version_reports_e019() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--target-version=not-a-version",
            "--path",
            root.to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "error[E019]: invalid target version 'not-a-version'",
        ));
}

// ===================================================================
// Phase B: Format Version Detection
// ===================================================================

// B1: Header comment detected
#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "header comment format version detected correctly"
)]
fn detect_format_version_from_header() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "current.spec",
        "// specforge-format: 1.0\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );
    write_spec(
        root,
        "old.spec",
        "// specforge-format: 0.1\nbehavior bar \"Bar\" {\n  contract \"stuff\"\n}\n",
    );

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // Each file's header is read: 1.0 is current, 0.1 needs migrating.
    let current = result_for(&json, "current.spec");
    assert_eq!(
        current["from_version"],
        serde_json::json!({"major": 1, "minor": 0})
    );
    assert_eq!(current["status"], "skipped");
    let old = result_for(&json, "old.spec");
    assert_eq!(
        old["from_version"],
        serde_json::json!({"major": 0, "minor": 1})
    );
    assert_eq!(old["status"], "migrated");
}

// B2: Missing version defaults to current (no migration needed)
#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "missing format version treated as the current version"
)]
fn missing_version_header_defaults_to_current() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "test.spec",
        "behavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    // No header = current version → skipped
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .map(|o| {
            let stdout = String::from_utf8(o.stdout).unwrap();
            let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
            assert_eq!(
                json["files_skipped"], 1,
                "file without header should be skipped"
            );
        })
        .unwrap();
}

// B3: Unsupported version in header → E019
#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "unsupported format version produces E019 with upgrade guidance"
)]
fn unsupported_version_in_header() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    let original = "// specforge-format: 99.0\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
    write_spec(root, "test.spec", original);

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["files_failed"], 1, "{json}");
    assert_eq!(json["files_skipped"], 0, "{json}");
    let error = json["results"][0]["error"].as_str().unwrap_or_default();
    assert!(error.contains("E019"), "{json}");
    assert!(error.contains("Use a format version between"), "{json}");
    assert_eq!(
        std::fs::read_to_string(root.join("spec/test.spec")).unwrap(),
        original,
        "the file is left alone"
    );
}

// ===================================================================
// Phase C: In-Place Migration with Backups
// ===================================================================

// C1: Files at target version are skipped
#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "files already at target version are skipped with skippedCount incremented"
)]
fn files_at_target_version_are_skipped() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "test.spec",
        "// specforge-format: 1.0\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["files_skipped"], 1);
    assert_eq!(json["files_migrated"], 0);

    // No .bak file should exist
    assert!(
        !root.join("spec/test.spec.bak").exists(),
        "skipped files should not create backups"
    );
}

// C2: Backup created before modification (simulated with version 0.1 if we had a transform)
// Since v1 is current and no older versions exist yet, we test the backup mechanism
// by using a file that requires header addition.
#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "backup created before modification"
)]
fn backup_created_before_modification() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    // A file with version 0.1 (older than current) will need migration
    let original = "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
    write_spec(root, "test.spec", original);

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success();

    // Backup should exist with original content
    let bak_path = root.join("spec/test.spec.bak");
    assert!(bak_path.exists(), ".spec.bak should exist");
    let bak_content = fs::read_to_string(&bak_path).unwrap();
    assert_eq!(
        bak_content, original,
        "backup should contain original content byte-for-byte"
    );
}

// C3: Atomic write via temp+rename — no .spec.tmp remains
#[test]
fn no_tmp_file_remains_after_migration() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success();

    assert!(
        !root.join("spec/test.spec.tmp").exists(),
        "temp file should not remain"
    );

    // Migrated file should have updated header
    let content = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert!(
        content.starts_with("// specforge-format: 1.0"),
        "migrated file should have v1.0 header: {content}"
    );
}

// C4: File failure isolation + --no-backup
#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "--no-backup skips backup creation"
)]
fn no_backup_flag_skips_backup_creation() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--no-backup", "--path", root.to_str().unwrap()])
        .assert()
        .success();

    assert!(
        !root.join("spec/test.spec.bak").exists(),
        "--no-backup should prevent .bak creation"
    );
    assert_eq!(
        fs::read_to_string(root.join("spec/test.spec")).unwrap(),
        "// specforge-format: 1.0\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
        "the file is still migrated"
    );
}

// ===================================================================
// Phase D: Dry-Run Diff
// ===================================================================

// D1: --dry-run shows diff without modifying files
#[specforge_test(
    behavior = "generate_migration_diff",
    verify = "dry-run shows unified diff without modifying files"
)]
fn dry_run_shows_diff_without_modifying_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    let original = "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
    write_spec(root, "test.spec", original);

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--dry-run", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("---"))
        .stdout(predicate::str::contains("+++"))
        .stdout(predicate::str::contains("@@"));

    // File should be unchanged
    let after = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert_eq!(after, original, "dry-run should not modify files");

    // No backup should exist
    assert!(
        !root.join("spec/test.spec.bak").exists(),
        "dry-run should not create backups"
    );
}

// D2: Diff uses a/b/ prefix convention
#[specforge_test(
    behavior = "generate_migration_diff",
    verify = "each file diff labeled with file path"
)]
fn diff_uses_posix_prefix_convention() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--dry-run", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let label = |prefix: &str| {
        stdout
            .lines()
            .find(|l| l.starts_with(prefix))
            .unwrap_or_else(|| panic!("no {prefix} line: {stdout}"))
            .to_string()
    };
    let old_label = label("--- a/");
    let new_label = label("+++ b/");
    assert!(
        old_label.ends_with("spec/test.spec"),
        "the --- label names the file: {old_label}"
    );
    assert!(
        new_label.ends_with("spec/test.spec"),
        "the +++ label names the file: {new_label}"
    );
    assert_eq!(
        old_label.strip_prefix("--- a/"),
        new_label.strip_prefix("+++ b/"),
        "both labels name the same file"
    );
}

// D3: --dry-run --format=json → structured MigrationDiff
#[specforge_test(
    behavior = "generate_migration_diff",
    verify = "json format diff produces structured output with file-level entries"
)]
fn dry_run_json_produces_structured_diff() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--dry-run",
            "--format=json",
            "--path",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let diffs = json["diffs"].as_array().expect("diffs should be an array");
    assert!(!diffs.is_empty(), "should have at least one diff");

    let first = &diffs[0];
    assert!(first.get("file_path").is_some(), "missing file_path");
    assert!(first.get("before_hash").is_some(), "missing before_hash");
    assert!(first.get("after_hash").is_some(), "missing after_hash");
    assert!(first.get("unified_text").is_some(), "missing unified_text");
}

// ===================================================================
// Phase E: Rollback
// ===================================================================

// E1: --rollback restores from .bak files
#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "restores migrated files from .bak backups"
)]
fn rollback_restores_from_bak_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    let original = "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
    write_spec(root, "test.spec", original);

    // Migrate first (creates .bak)
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success();

    // Verify migration happened
    let migrated = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert!(migrated.contains("1.0"), "file should be migrated");

    // Now rollback
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--rollback", "--path", root.to_str().unwrap()])
        .assert()
        .success();

    // File should be restored to original
    let restored = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert_eq!(
        restored, original,
        "rollback should restore original content"
    );

    // .bak should still exist (we don't delete it)
    assert!(
        root.join("spec/test.spec.bak").exists(),
        ".bak should still exist"
    );
}

// E2: Missing .bak → skip
#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "missing .bak file produces warning and skips"
)]
fn rollback_missing_bak_skips() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success();
    let migrated = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    // The backup the record names is gone.
    fs::remove_file(root.join("spec/test.spec.bak")).unwrap();

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--rollback",
            "--format=json",
            "--path",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["skipped_count"], 1, "should skip files without .bak");
    assert_eq!(json["restored_count"], 0, "{json}");
    assert_eq!(json["failed_count"], 0, "{json}");
    let warnings = json["warnings"].as_array().expect("warnings array");
    assert_eq!(warnings.len(), 1, "{json}");
    assert!(
        warnings[0].as_str().unwrap().contains("test.spec.bak"),
        "the warning names the missing backup: {json}"
    );

    // The human output shows the same warning.
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--rollback", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("warning:"))
        .stderr(predicate::str::contains("test.spec.bak"))
        .stderr(predicate::str::contains("0 restored, 1 skipped, 0 failed"));
    assert_eq!(
        fs::read_to_string(root.join("spec/test.spec")).unwrap(),
        migrated,
        "a skipped file is left alone"
    );
}

// E3: Single restore failure doesn't block others
#[cfg(unix)]
#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "rollback failure for one file does not block others"
)]
fn rollback_failure_isolation() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    let original_a = "// specforge-format: 0.1\nbehavior a \"A\" {\n  contract \"a\"\n}\n";
    let original_b = "// specforge-format: 0.1\nbehavior b \"B\" {\n  contract \"b\"\n}\n";
    write_spec(root, "a.spec", original_a);
    write_spec(root, "b.spec", original_b);

    // Migrate both
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success();

    let migrated_a = fs::read_to_string(root.join("spec/a.spec")).unwrap();
    assert_ne!(migrated_a, original_a);

    // Make a's restore fail: its backup can't be read.
    let bak_a = root.join("spec/a.spec.bak");
    fs::set_permissions(&bak_a, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_to_string(&bak_a).is_ok() {
        // Running as root: permissions don't stop the read.
        return;
    }

    // Rollback — a fails, b is still restored
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--rollback",
            "--format=json",
            "--path",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    fs::set_permissions(&bak_a, fs::Permissions::from_mode(0o644)).unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["failed_count"], 1, "{json}");
    assert_eq!(json["restored_count"], 1, "{json}");
    assert_eq!(json["skipped_count"], 0, "{json}");
    let failed = result_for(&json, "a.spec");
    assert_eq!(failed["status"], "failed", "{json}");
    assert!(
        failed["error"].as_str().unwrap().contains("backup"),
        "{json}"
    );
    assert_eq!(status_of(&json, "b.spec"), "restored");
    assert_eq!(
        output.status.code(),
        Some(1),
        "a failed restore fails the run"
    );

    // b is restored; a keeps its migrated content.
    let b_content = fs::read_to_string(root.join("spec/b.spec")).unwrap();
    assert_eq!(b_content, original_b, "b.spec should be restored");
    assert_eq!(
        fs::read_to_string(root.join("spec/a.spec")).unwrap(),
        migrated_a,
        "a.spec is untouched by its failed restore"
    );
}

// ===================================================================
// Phase F: Graph Validation
// ===================================================================

// ===================================================================
// Phase H: Integration
// ===================================================================

// H1: Full pipeline — v0.1→v1.0 migration end-to-end
#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "files migrated from source to target version"
)]
fn full_pipeline_migration() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    let files = [
        (
            "a.spec",
            "// specforge-format: 0.1\nbehavior a \"A\" {\n  contract \"a\"\n}\n",
        ),
        (
            "b.spec",
            "// specforge-format: 0.1\nbehavior b \"B\" {\n  contract \"b\"\n}\n",
        ),
        (
            "c.spec",
            "// specforge-format: 0.1\nbehavior c \"C\" {\n  contract \"c\"\n}\n",
        ),
    ];

    for (name, content) in &files {
        write_spec(root, name, content);
    }

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("3 migrated"));

    // All files should have v1.0 header
    for (name, _) in &files {
        let content = fs::read_to_string(root.join("spec").join(name)).unwrap();
        assert!(
            content.starts_with("// specforge-format: 1.0"),
            "{name} should have v1.0 header: {content}"
        );
    }
}

// H2: Idempotency — double migrate → no changes
#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "files already at target version are skipped with skippedCount incremented"
)]
fn double_migrate_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    // First migration
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("1 migrated"));

    let after_first = fs::read_to_string(root.join("spec/test.spec")).unwrap();

    // Second migration — should skip
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["files_skipped"], 1, "{json}");
    assert_eq!(json["files_migrated"], 0, "{json}");
    assert_eq!(json["files_failed"], 0, "{json}");
    assert_eq!(status_of(&json, "test.spec"), "skipped");

    let after_second = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert_eq!(
        after_first, after_second,
        "second migrate should not change files"
    );
}

// H3: Failure in one file does not block others
#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "failure in one file does not block others"
)]
fn failure_in_one_file_does_not_block_others() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    let good = "// specforge-format: 0.1\nbehavior good \"Good\" {\n  contract \"ok\"\n}\n";
    let bad = "// specforge-format: 9.0\nbehavior bad \"Bad\" {\n  contract \"ok\"\n}\n";
    write_spec(root, "good.spec", good);
    write_spec(root, "bad.spec", bad);

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(output.status.code(), Some(1), "{json}");
    assert_eq!(json["files_migrated"], 1, "{json}");
    assert_eq!(json["files_failed"], 1, "{json}");
    let failed = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["file_path"].as_str().unwrap().ends_with("bad.spec"))
        .unwrap();
    assert_eq!(failed["status"], "failed", "{json}");
    assert!(
        failed["error"].as_str().unwrap().starts_with("E019"),
        "{json}"
    );
    assert!(
        fs::read_to_string(root.join("spec/good.spec"))
            .unwrap()
            .starts_with("// specforge-format: 1.0"),
        "good.spec is migrated despite bad.spec"
    );
    assert_eq!(
        fs::read_to_string(root.join("spec/bad.spec")).unwrap(),
        bad,
        "bad.spec is left alone"
    );
}

// H3b: JSON summary output
#[test]
fn json_summary_contains_results_and_backups() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(json["results"].is_array(), "results should be an array");
    assert!(json["backups"].is_array(), "backups should be an array");
    assert_eq!(json["files_migrated"], 1);
    assert!(
        !json["backups"].as_array().unwrap().is_empty(),
        "backups should be populated"
    );
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "migrate --format json prints the migration document specforge.migrate answers, with files_written"
)]
fn migrate_json_is_the_migration_document() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "old.spec",
        "// specforge-format: 0.1\nbehavior old_one \"Old\" {\n  contract \"stuff\"\n}\n",
    );

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true, "{json}");
    assert_eq!(json["pending"], true, "{json}");
    assert_eq!(json["dry_run"], false, "{json}");
    assert_eq!(json["files_migrated"], 1, "{json}");
    let backups = json["backups"].as_array().unwrap();
    assert_eq!(backups.len(), 1, "{json}");
    let written: Vec<&str> = json["files_written"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file.as_str().unwrap())
        .collect();
    assert!(written.contains(&"spec/old.spec.bak"), "{json}");
    assert!(written.contains(&".specforge/migration.json"), "{json}");
}

// ===================================================================
// Phase I: detect_format_version_mismatch — the compile reports it
// ===================================================================

/// `specforge check --format json` over the project at `root`: the exit code
/// and the diagnostics it reports.
fn check_json(root: &std::path::Path) -> (Option<i32>, Vec<serde_json::Value>) {
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["check", "--format", "json"])
        .arg(root)
        .output()
        .unwrap();
    let diagnostics = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "check printed no JSON ({e}): {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.code(), diagnostics)
}

fn reported<'a>(diagnostics: &'a [serde_json::Value], code: &str) -> Vec<&'a serde_json::Value> {
    diagnostics.iter().filter(|d| d["code"] == code).collect()
}

#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "older format version detected and reported as I007"
)]
fn older_format_version_produces_i007() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "old.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"x\"\n}\n",
    );
    write_spec(
        root,
        "current.spec",
        "// specforge-format: 1.0\nbehavior bar \"Bar\" {\n  contract \"x\"\n}\n",
    );
    write_spec(
        root,
        "bare.spec",
        "behavior baz \"Baz\" {\n  contract \"x\"\n}\n",
    );

    let (code, diagnostics) = check_json(root);

    // `check` reports the older file once, on its header line; the current
    // file and the one with no header report nothing. It is an info: the
    // project still checks.
    assert_eq!(code, Some(0), "{diagnostics:?}");
    let i007 = reported(&diagnostics, "I007");
    assert_eq!(i007.len(), 1, "{diagnostics:?}");
    assert_eq!(i007[0]["file"], "spec/old.spec", "{}", i007[0]);
    assert_eq!(i007[0]["line"], 1, "{}", i007[0]);
    assert_eq!(i007[0]["severity"], "Info", "{}", i007[0]);
    assert!(
        i007[0]["message"].as_str().unwrap().contains("0.1"),
        "{}",
        i007[0]
    );
    assert!(reported(&diagnostics, "E019").is_empty(), "{diagnostics:?}");

    // Migrating the file clears it: the header is current.
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success();
    let (code, diagnostics) = check_json(root);
    assert_eq!(code, Some(0), "{diagnostics:?}");
    assert!(reported(&diagnostics, "I007").is_empty(), "{diagnostics:?}");
}

#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "Detect Format Version Mismatch: format version detection holds — spec_file_available, version_mismatch_reported, unsupported_version_rejected, parsing_continues"
)]
fn detect_format_version_contract() {
    // Requires (spec_file_available): .spec files are being compiled.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "older.spec",
        "// specforge-format: 0.9\nbehavior older_one \"Older\" {\n  contract \"x\"\n}\n",
    );
    write_spec(
        root,
        "newer.spec",
        "// specforge-format: 9.0\nbehavior newer_one \"Newer\" {\n  contract \"x\"\n}\n",
    );
    write_spec(
        root,
        "garbled.spec",
        "// specforge-format: abc\nbehavior garbled_one \"Garbled\" {\n  contract \"x\"\n}\n",
    );

    let (code, diagnostics) = check_json(root);

    // version_mismatch_reported: I007 for the older file.
    let i007 = reported(&diagnostics, "I007");
    assert_eq!(i007.len(), 1, "{diagnostics:?}");
    assert_eq!(i007[0]["file"], "spec/older.spec");
    // unsupported_version_rejected: E019 with upgrade guidance for the newer
    // and for the unreadable header, which fails the check.
    let e019 = reported(&diagnostics, "E019");
    assert_eq!(e019.len(), 2, "{diagnostics:?}");
    let newer = e019
        .iter()
        .find(|d| d["file"] == "spec/newer.spec")
        .unwrap();
    assert_eq!(newer["severity"], "Error");
    assert!(
        newer["suggestion"]
            .as_str()
            .unwrap()
            .contains("Use a format version between"),
        "{newer}"
    );
    assert!(e019.iter().any(|d| d["file"] == "spec/garbled.spec"));
    assert_eq!(code, Some(1), "an unsupported format version fails check");

    // parsing_continues: every file is still in the graph.
    let export = Command::cargo_bin("specforge")
        .unwrap()
        .args(["export", "--format", "graph", "--no-schema"])
        .arg(root)
        .output()
        .unwrap();
    let graph: serde_json::Value = serde_json::from_slice(&export.stdout).unwrap();
    let mut ids: Vec<&str> = graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    ids.sort();
    assert_eq!(ids, ["garbled_one", "newer_one", "older_one"]);
}

#[test]
fn a_file_with_no_header_reports_no_version_diagnostic() {
    // Almost every file has none (no file of this repository's own specs
    // carries a header): none is the current version, so nothing to report.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "bare.spec",
        "behavior baz \"Baz\" {\n  contract \"x\"\n}\n",
    );

    let (code, diagnostics) = check_json(root);

    assert_eq!(code, Some(0), "{diagnostics:?}");
    assert!(reported(&diagnostics, "I007").is_empty(), "{diagnostics:?}");
    assert!(reported(&diagnostics, "E019").is_empty(), "{diagnostics:?}");
}

// ===================================================================
// Phase J: migrate_spec_files_in_place — additional coverage
// ===================================================================

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "Migrate Spec Files In Place: in-place migration holds — semantic_preservation"
)]
fn migrate_in_place_contract() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    // Requires: files exist, valid target version
    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    // Ensures: files at target, summary emitted, complete event
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // files_at_target
    assert_eq!(json["files_migrated"], 1);
    assert_eq!(json["files_failed"], 0);

    // summary_emitted
    assert!(json.get("files_migrated").is_some());
    assert!(json.get("files_skipped").is_some());
    assert!(json.get("files_failed").is_some());

    // Verify file is now at target version
    let content = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert!(content.starts_with("// specforge-format: 1.0"));
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "summary reports migrated, failed, and skipped counts"
)]
fn summary_reports_all_three_counts() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    // One file to migrate, one already at current
    write_spec(
        root,
        "old.spec",
        "// specforge-format: 0.1\nbehavior old \"Old\" {\n  contract \"old\"\n}\n",
    );
    write_spec(
        root,
        "current.spec",
        "// specforge-format: 1.0\nbehavior cur \"Cur\" {\n  contract \"cur\"\n}\n",
    );

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--format=json", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["files_migrated"], 1, "one file migrated");
    assert_eq!(json["files_skipped"], 1, "one file skipped");
    assert_eq!(json["files_failed"], 0, "no failures");
}

// ===================================================================
// Phase K: generate_migration_diff — additional coverage
// ===================================================================

#[specforge_test(
    behavior = "generate_migration_diff",
    verify = "diff format is compatible with patch(1)"
)]
fn diff_format_compatible_with_patch() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--dry-run", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();

    // POSIX unified diff format requirements:
    // 1. Starts with --- a/... and +++ b/...
    assert!(stdout.contains("--- a/"), "missing --- a/ header");
    assert!(stdout.contains("+++ b/"), "missing +++ b/ header");
    // 2. Has hunk headers @@ -N,M +N,M @@
    assert!(stdout.contains("@@"), "missing @@ hunk header");
    // 3. Changed lines start with + or -
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with('-') && !l.starts_with("---")),
        "missing - removed lines"
    );
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with('+') && !l.starts_with("+++")),
        "missing + added lines"
    );

    // 4. patch(1) applies it from the project root.
    let mut patch = std::process::Command::new("patch")
        .args(["-p1", "--dry-run"])
        .current_dir(root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("patch(1) is installed");
    std::io::Write::write_all(&mut patch.stdin.take().unwrap(), stdout.as_bytes()).unwrap();
    let applied = patch.wait_with_output().unwrap();
    assert!(
        applied.status.success(),
        "patch -p1 --dry-run: {}{}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
}

#[specforge_test(
    behavior = "generate_migration_diff",
    verify = "failure in one file does not block diff generation for others"
)]
fn dry_run_failure_isolation() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "good.spec",
        "// specforge-format: 0.1\nbehavior good \"Good\" {\n  contract \"ok\"\n}\n",
    );
    write_spec(
        root,
        "bad.spec",
        "// specforge-format: 9.0\nbehavior bad \"Bad\" {\n  contract \"ok\"\n}\n",
    );
    let before = crate::written::files_under(root);

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--dry-run",
            "--format=json",
            "--path",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let diffs = json["diffs"].as_array().expect("diffs should be array");
    assert_eq!(diffs.len(), 1, "{json}");
    assert!(
        diffs[0]["file_path"]
            .as_str()
            .unwrap()
            .ends_with("good.spec"),
        "{json}"
    );
    assert_eq!(json["files_failed"], 1, "{json}");
    assert_eq!(
        crate::written::changed_since(root, &before),
        Vec::<String>::new(),
        "a dry run writes nothing"
    );
}

#[specforge_test(
    behavior = "generate_migration_diff",
    verify = "Generate Migration Diff: migration diff generation holds — spec_files_available, dry_run_flag_set, diff_produced, no_files_modified, migration_diff_generated_emitted"
)]
fn migration_diff_contract() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    let original = "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
    write_spec(root, "test.spec", original);

    // Requires: files available, dry-run flag set
    // Ensures: diff produced, no files modified, event emitted

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--dry-run",
            "--format=json",
            "--path",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // diff_produced
    let diffs = json["diffs"].as_array().expect("diffs array");
    assert!(!diffs.is_empty(), "diff should be produced");

    // no_files_modified
    let after = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert_eq!(after, original, "dry-run must not modify files");
    assert!(
        !root.join("spec/test.spec.bak").exists(),
        "dry-run must not create backups"
    );
}

// ===================================================================
// Phase L: validate_post_migration_integrity — additional coverage
// ===================================================================

// ===================================================================
// Phase N: verify_graph_protocol_compatibility — additional coverage
// ===================================================================

// ===================================================================
// Phase O: rollback_failed_migration — additional coverage
// ===================================================================

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "restore is atomic per file"
)]
fn rollback_restore_is_atomic() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    let original = "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
    write_spec(root, "test.spec", original);

    // Migrate
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success();

    let migrated = fs::read_to_string(root.join("spec/test.spec")).unwrap();

    // Block the temp-file step: a directory sits where the restore's temp
    // file goes. An atomic restore (write temp, then rename) fails before
    // touching the target; a direct write would overwrite it.
    let blocker = root.join("spec/test.spec.restore.tmp");
    fs::create_dir(&blocker).unwrap();
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--rollback",
            "--format=json",
            "--path",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["failed_count"], 1, "{json}");
    assert_eq!(json["restored_count"], 0, "{json}");
    assert_eq!(
        fs::read_to_string(root.join("spec/test.spec")).unwrap(),
        migrated,
        "a failed restore leaves the target exactly as it was"
    );

    // Unblocked, the restore completes and leaves no temp file behind.
    fs::remove_dir(&blocker).unwrap();
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--rollback", "--path", root.to_str().unwrap()])
        .assert()
        .success();
    assert!(
        !blocker.exists(),
        "no .restore.tmp should remain after atomic rollback"
    );
    let content = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert_eq!(content, original, "file fully restored atomically");
}

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "summary reports restored, skipped, and failed counts"
)]
fn rollback_summary_counts() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    write_spec(
        root,
        "a.spec",
        "// specforge-format: 0.1\nbehavior a \"A\" {\n  contract \"a\"\n}\n",
    );
    write_spec(
        root,
        "c.spec",
        "// specforge-format: 0.1\nbehavior c \"C\" {\n  contract \"c\"\n}\n",
    );

    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success();
    // c.spec is edited after the migration: a rollback leaves it as it is.
    let c = root.join("spec/c.spec");
    let edited = format!("{}// edited\n", fs::read_to_string(&c).unwrap());
    fs::write(&c, &edited).unwrap();

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--rollback",
            "--format=json",
            "--path",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(
        json.get("restored_count").is_some(),
        "missing restored_count"
    );
    assert!(json.get("skipped_count").is_some(), "missing skipped_count");
    assert!(json.get("failed_count").is_some(), "missing failed_count");

    let restored = json["restored_count"].as_u64().unwrap_or(0);
    let skipped = json["skipped_count"].as_u64().unwrap_or(0);
    assert_eq!(restored, 1, "a.spec should be restored");
    assert_eq!(skipped, 1, "c.spec was edited since: skipped");
    assert_eq!(fs::read_to_string(&c).unwrap(), edited);
}

#[specforge_test(
    behavior = "rollback_failed_migration",
    verify = "Rollback Failed Migration: migration rollback holds — migration_recorded, files_restored, edited_files_kept, rollback_event_emitted, backup_file_preservation"
)]
fn rollback_contract() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);

    let original = "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
    write_spec(root, "test.spec", original);

    // Requires: migration_started (backups exist)
    Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .assert()
        .success();

    assert!(
        root.join("spec/test.spec.bak").exists(),
        "backup must exist before rollback"
    );

    // Ensures: files restored, event emitted
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--rollback",
            "--format=json",
            "--path",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["restored_count"], 1);
    assert_eq!(json["failed_count"], 0);

    // File restored
    let content = fs::read_to_string(root.join("spec/test.spec")).unwrap();
    assert_eq!(content, original);

    // Maintains: backup_file_preservation
    assert!(
        root.join("spec/test.spec.bak").exists(),
        ".bak preserved after rollback"
    );
}

// ===================================================================
// Phase Q: Remaining uncovered verify statements
// ===================================================================

use crate::written::{changed_since, files_under, files_written};

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "migrate --format json lists each migrated file and its backup in files_written"
)]
fn migrate_json_lists_each_file_and_its_backup() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    let old = "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
    write_spec(root, "a.spec", old);
    write_spec(root, "b.spec", &old.replace("foo", "bar"));
    let migrate = |extra: &[&str]| -> serde_json::Value {
        let output = Command::cargo_bin("specforge")
            .unwrap()
            .args([
                "migrate",
                "--path",
                root.to_str().unwrap(),
                "--format",
                "json",
            ])
            .args(extra)
            .output()
            .unwrap();
        serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {output:?}"))
    };

    // A dry run writes nothing and lists nothing.
    let before = files_under(root);
    let preview = migrate(&["--dry-run"]);
    assert!(preview.get("files_written").is_none(), "{preview}");
    assert_eq!(changed_since(root, &before), Vec::<String>::new());

    let migrated = migrate(&[]);
    let written = [
        ".specforge/migration.json",
        "spec/a.spec",
        "spec/a.spec.bak",
        "spec/b.spec",
        "spec/b.spec.bak",
    ];
    assert_eq!(files_written(&migrated), written);
    assert_eq!(changed_since(root, &before), written);

    // --rollback restores each file from its backup: those are listed.
    let before = files_under(root);
    let restored = migrate(&["--rollback"]);
    assert_eq!(
        files_written(&restored),
        [".specforge/migration.json", "spec/a.spec", "spec/b.spec"]
    );
    assert_eq!(changed_since(root, &before), files_written(&restored));
}

// ===================================================================
// Plan 14, T0 pins: what migrate prints today
// ===================================================================

/// The migration document's `diagnostics` are the problems of `specforge.json`
/// the run went around (plan 03): none for a usable config, but the key is
/// there (the summary's own list stays gone, plan 14 D4).
#[test]
fn migrate_json_diagnostics_are_the_configs_problems() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--dry-run",
            "--format=json",
            "--path",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["diagnostics"], serde_json::json!([]), "{json}");
}

/// A dry-run diff labels each file with its path from the project root, so
/// that `patch -p1` applies it there (plan 14 D14).
#[test]
fn dry_run_diff_labels_are_relative_to_the_project_root() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );

    let text = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--dry-run", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8(text.stdout).unwrap();
    let mut lines = stdout.lines();
    assert_eq!(lines.next(), Some("--- a/spec/test.spec"), "{stdout}");
    assert_eq!(lines.next(), Some("+++ b/spec/test.spec"), "{stdout}");

    // MCP and --format json carry the same text.
    let json = Command::cargo_bin("specforge")
        .unwrap()
        .args([
            "migrate",
            "--dry-run",
            "--format=json",
            "--path",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert!(
        json["diffs"][0]["unified_text"]
            .as_str()
            .unwrap()
            .starts_with("--- a/spec/test.spec\n+++ b/spec/test.spec\n"),
        "{json}"
    );
}

// The binary half of the migration crate's `non_breaking_schema_change_no_w053`
// (tests/compare.rs): a real migration of an entity-bearing project passes
// silently.
#[specforge_test(
    behavior = "verify_graph_protocol_compatibility_after_migration",
    verify = "non-breaking graph change passes silently"
)]
fn a_migration_of_an_entity_bearing_project_prints_only_the_summary() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    setup_project(root);
    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    );
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(
        stderr.trim(),
        "1 migrated, 0 skipped, 0 failed",
        "nothing but the summary"
    );
}

// The binary half of the migration crate's `graph_protocol_compatibility_contract`
// (tests/compare.rs): `specforge migrate` snapshots the schema before touching
// files (pre_migration_snapshot_available), runs the extension hooks
// (extension_hooks_complete), then compares and finishes
// (graph_protocol_compatibility_emitted). A format-only migration keeps the
// schema, so it passes with nothing but the summary.
#[specforge_test(
    behavior = "verify_graph_protocol_compatibility_after_migration",
    verify = "Verify Graph Protocol Compatibility After Migration: graph protocol compatibility verification holds — pre_migration_snapshot_available, extension_hooks_complete, compatibility_verified, breaking_changes_warned, graph_protocol_compatibility_emitted"
)]
fn a_format_only_migration_passes_graph_protocol_compatibility_silently() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("specforge.json"),
        r#"{"name":"test","version":"0.1.0","extensions":["@specforge/software"]}"#,
    )
    .unwrap();
    write_spec(
        root,
        "test.spec",
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n  invariants [bar]\n}\n\ninvariant bar \"Bar\" {\n  guarantee \"holds\"\n}\n",
    );
    let output = Command::cargo_bin("specforge")
        .unwrap()
        .args(["migrate", "--path", root.to_str().unwrap()])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert_eq!(stderr.trim(), "1 migrated, 0 skipped, 0 failed");
    assert!(
        fs::read_to_string(root.join("spec/test.spec"))
            .unwrap()
            .starts_with("// specforge-format: 1.0\n"),
        "the file was migrated"
    );
}

// --- plan 03: a specforge.json that hides the project's sources ---

/// The `0.9`-header text of a spec file, to migrate.
const OLD: &str = "// specforge-format: 0.9\nbehavior alpha \"Alpha\" {\n}\n";

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a specforge.json that does not say which files are the project's sources refuses the migration, dry run included, and nothing is written"
)]
fn migrate_refuses_a_config_that_does_not_say_which_files_are_its_sources() {
    for config in crate::format::HIDES_SOURCES {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        crate::format::project_with_config(root, config);
        fs::write(root.join("spec/a.spec"), OLD).unwrap();
        fs::write(root.join("vendor/v.spec"), OLD.replace("alpha", "vend")).unwrap();
        let before = crate::written::files_under(root);

        for args in [&["migrate"][..], &["migrate", "--dry-run"][..]] {
            let (code, json) = crate::format::json_of(root, args);
            assert_eq!(code, Some(1), "{config} {args:?}: {json}");
            assert_eq!(json["code"], "config_invalid", "{config} {args:?}: {json}");
            assert_eq!(
                crate::written::files_under(root),
                before,
                "{config} {args:?}: no rewrite, no .bak, no migration record"
            );
        }
    }
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a problem in specforge.json the migration goes around is an E069 in its document and fails it"
)]
fn migrate_reports_a_config_problem_it_goes_around() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    crate::format::project_with_config(root, r#"{"spec_root":"spec","exclude":["["]}"#);
    fs::write(root.join("spec/a.spec"), "behavior alpha \"Alpha\" {\n}\n").unwrap();

    let (code, json) = crate::format::json_of(root, &["migrate", "--dry-run"]);
    assert_eq!(code, Some(1), "{json}");
    assert_eq!(json["ok"], false, "{json}");
    assert_eq!(json["pending"], false, "{json}");
    assert_eq!(json["diagnostics"][0]["code"], "E069", "{json}");
}
