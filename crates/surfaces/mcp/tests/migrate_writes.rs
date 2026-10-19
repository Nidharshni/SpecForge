//! `specforge.migrate` writes through the project change (ADR 0171): a source the process may not write
//! refuses the call as a typed edit does, and a temporary name linked outside is unlinked, not followed.

use serde_json::json;
use specforge_mcp::testing::Served;
use specforge_project::testing::TestProject;
use std::fs;
use tempfile::TempDir;

const OLD: &str = "// specforge-format: 0.9\nbehavior alpha \"Alpha\" {\n}\n";

fn old_project() -> TestProject {
    TestProject::new()
        .enabling(&["@specforge/software"])
        .file("spec/main.spec", OLD)
}

#[cfg(unix)]
#[specforge_test_macros::test(
    behavior = "migrate_spec_files_in_place",
    verify = "a read-only source refuses the migration and nothing is written"
)]
fn a_read_only_source_refuses_the_migration_over_mcp() {
    use std::os::unix::fs::PermissionsExt as _;
    let project = old_project();
    let source = project.root().join("spec/main.spec");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o444)).unwrap();
    // A process that may write anyway (root) cannot show the refusal.
    if fs::OpenOptions::new().write(true).open(&source).is_ok() {
        return;
    }
    let mut served = Served::rooted(project);

    let resp = served.call_tool("specforge.migrate", json!({}));

    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "permission_denied", "{error}");
    assert!(error.get("files_written").is_none(), "{error}");
    assert_eq!(fs::read_to_string(&source).unwrap(), OLD);
    assert_eq!(
        fs::metadata(&source).unwrap().permissions().mode() & 0o777,
        0o444
    );
    assert!(!served.root().join("spec/main.spec.bak").exists());
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation reads and writes no file through a symlink that leaves the project"
)]
fn a_temporary_name_linked_outside_is_unlinked_not_followed() {
    let project = old_project();
    let outside = TempDir::new().unwrap();
    let target = outside.path().join("tmp");
    fs::write(&target, "TMP").unwrap();
    let link = project.root().join("spec/main.spec.tmp");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let mut served = Served::rooted(project);

    let reply = served.tool("specforge.migrate", json!({}));

    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "TMP");
    assert!(fs::symlink_metadata(&link).is_err(), "the link is gone");
}
