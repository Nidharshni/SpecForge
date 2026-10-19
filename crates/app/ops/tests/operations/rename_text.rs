//! A rename applies its edits only to the text it planned on (ADR 0116).

use specforge_ops::OpErrorKind;
use specforge_ops::authoring::rename;

const LIMIT: &str = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";
const LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";

#[test]
fn apply_refuses_a_file_changed_since_the_plan() {
    let p = crate::navigate::compile(
        crate::navigate::SOFTWARE,
        &[("limit.spec", LIMIT), ("login.spec", LOGIN)],
    );
    let root = p.project.environment().spec_root().to_path_buf();
    let plan = rename::plan(&p.navigator(), "session_limit", "session_cap").unwrap();

    // The file moves after the plan: its edits are positions in another text.
    std::fs::write(root.join("login.spec"), format!("\n{LOGIN}")).unwrap();
    let before = crate::writes::files_under(&root);

    let error = rename::apply(&plan, specforge_ops::testing::caller()).unwrap_err();

    assert_eq!(error.kind, OpErrorKind::Conflict, "{error:?}");
    assert_eq!(error.code, specforge_ops::FILE_CHANGED, "{error:?}");
    assert!(error.message.contains("login.spec"), "{}", error.message);
    assert!(crate::writes::changed_since(&root, &before).is_empty());
}

/// A write the OS refuses is `file_unwritable`, worded by the change (ADR 0155).
#[cfg(unix)]
#[test]
#[ignore = "flipped by T4"]
fn a_rename_that_cannot_write_a_file_is_unwritable() {
    use std::os::unix::fs::PermissionsExt;
    let p = crate::navigate::compile(
        crate::navigate::SOFTWARE,
        &[("limit.spec", LIMIT), ("login.spec", LOGIN)],
    );
    let root = p.project.environment().spec_root().to_path_buf();
    let plan = rename::plan(&p.navigator(), "session_limit", "session_cap").unwrap();
    let locked = root.join("login.spec");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o444)).unwrap();
    if std::fs::OpenOptions::new()
        .append(true)
        .open(&locked)
        .is_ok()
    {
        return; // permissions do not bind (root)
    }

    let error = rename::apply(&plan, specforge_ops::testing::caller()).unwrap_err();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(error.code, "file_unwritable", "{error:?}");
    assert_eq!(error.kind, OpErrorKind::PermissionDenied, "{error:?}");
    assert!(
        error
            .message
            .starts_with("cannot rename 'session_limit': failed to write login.spec: "),
        "{}",
        error.message
    );
    assert!(
        error.message.ends_with("; nothing was written"),
        "{}",
        error.message
    );
    assert!(error.writes.is_empty());
    assert_eq!(
        std::fs::read_to_string(root.join("limit.spec")).unwrap(),
        LIMIT
    );
}
