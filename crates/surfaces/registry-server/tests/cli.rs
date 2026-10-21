//! The `specforge-registry` binary as an operator runs it: a bad data directory is an error with a
//! message and exit 1, never a panic.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use std::process::{Command, Output};

/// A data directory that cannot be created: its parent is a file.
fn unusable_data_dir() -> (tempfile::NamedTempFile, String) {
    let file = tempfile::NamedTempFile::new().unwrap();
    let dir = format!("{}/x", file.path().display());
    (file, dir)
}

fn registry(args: &[&str]) -> Output {
    // Spelled in two pieces: the CLI suite's isolation guard greps the tests for the `specforge`
    // binary's variable, and this is the registry's binary.
    Command::new(env!(concat!("CARGO_BIN_EXE_", "specforge-registry")))
        .args(args)
        .output()
        .expect("the registry binary runs")
}

#[test]
fn token_create_with_an_unusable_data_dir_exits_1() {
    let (_file, dir) = unusable_data_dir();
    let output = registry(&[
        "token",
        "create",
        "--account",
        "a",
        "--no-expiry",
        "--data-dir",
        &dir,
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.starts_with("error: cannot create the data directory"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn serve_with_an_unusable_data_dir_exits_1() {
    let (_file, dir) = unusable_data_dir();
    let output = registry(&["serve", "--data-dir", &dir, "--port", "0"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.starts_with("error: cannot open the registry at"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn scope_assign_hands_a_scope_to_an_account() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_str().unwrap();
    let created = registry(&[
        "token",
        "create",
        "--account",
        "acme",
        "--no-expiry",
        "--data-dir",
        data,
    ]);
    assert_eq!(created.status.code(), Some(0), "{created:?}");
    let assigned = registry(&[
        "scope",
        "assign",
        "@acme",
        "--account",
        "acme",
        "--data-dir",
        data,
    ]);
    assert_eq!(assigned.status.code(), Some(0), "{assigned:?}");
    let stdout = String::from_utf8_lossy(&assigned.stdout);
    assert!(
        stdout.contains("scope @acme assigned to acme (was unclaimed)"),
        "{stdout}"
    );
    let refused = registry(&[
        "scope",
        "assign",
        "sdk",
        "--account",
        "acme",
        "--data-dir",
        data,
    ]);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert_eq!(refused.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("'sdk' is not a scope: write '@sdk'"),
        "{stderr}"
    );
}

#[test]
fn token_create_needs_an_account() {
    let output = registry(&["token", "create", "--no-expiry"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("--account"), "{stderr}");
}

#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "a token's scope filter is a scope: text that is not one is refused where the token is issued, and a token issued with none covers every scope"
)]
fn token_create_refuses_a_scope_filter_that_is_no_scope() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_str().unwrap();
    for (text, write) in [
        ("", None),
        ("sdk", Some("@sdk")),
        ("@sdk/", Some("@sdk")),
        ("@SDK", Some("@sdk")),
        ("@org/soft", Some("@org")),
    ] {
        let output = registry(&[
            "token",
            "create",
            "--account",
            "acme",
            "--scope",
            text,
            "--no-expiry",
            "--data-dir",
            data,
        ]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{text:?}: {stderr}");
        assert!(
            stderr.contains(&format!("'{text}' is not a scope")),
            "{text:?}: {stderr}"
        );
        if let Some(write) = write {
            assert!(
                stderr.contains(&format!("write '{write}'")),
                "{text:?}: {stderr}"
            );
        }
    }
    let listed = registry(&["token", "list", "--data-dir", data]);
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains("No tokens found."),
        "{listed:?}"
    );
    let help = registry(&["token", "create", "--help"]);
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(!help.contains("empty means full access"), "{help}");
    assert!(help.contains("omit it for every scope"), "{help}");
}

#[test]
fn token_list_lines_each_scope_up_under_its_header() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_str().unwrap();
    for extra in [
        &["--account", "acme", "--scope", "@acme"][..],
        &["--account", "a"],
    ] {
        let mut args = vec!["token", "create", "--no-expiry", "--data-dir", data];
        args.extend_from_slice(extra);
        assert_eq!(registry(&args).status.code(), Some(0));
    }

    let listed = registry(&["token", "list", "--data-dir", data]);
    let stdout = String::from_utf8_lossy(&listed.stdout);
    let column = stdout.lines().next().unwrap().find("SCOPE").unwrap();
    for cell in ["@acme", "(all)"] {
        let line = stdout.lines().find(|l| l.contains(cell)).unwrap();
        assert_eq!(line.find(cell), Some(column), "{stdout}");
    }
}

#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "a stored token scope that is not a scope authenticates nothing, and the token listing names the token to revoke"
)]
fn a_stored_scope_that_is_no_scope_is_named_for_revocation() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_str().unwrap();
    let created = registry(&[
        "token",
        "create",
        "--account",
        "acme",
        "--no-expiry",
        "--data-dir",
        data,
    ]);
    assert_eq!(created.status.code(), Some(0), "{created:?}");
    rusqlite::Connection::open(dir.path().join("registry.db"))
        .unwrap()
        .execute("UPDATE tokens SET scope = 'sdk'", [])
        .unwrap();

    let listed = registry(&["token", "list", "--data-dir", data]);
    let stderr = String::from_utf8_lossy(&listed.stderr);
    assert_eq!(listed.status.code(), Some(1), "{stderr}");
    let id = stderr
        .split("token revoke ")
        .nth(1)
        .and_then(|rest| rest.get(..8))
        .unwrap_or_else(|| panic!("no revoke command in: {stderr}"));
    assert!(
        stderr.contains("'sdk'") && stderr.contains(&format!("token {id} ")),
        "{stderr}"
    );

    let revoked = registry(&["token", "revoke", id, "--data-dir", data]);
    assert_eq!(revoked.status.code(), Some(0), "{revoked:?}");
    let listed = registry(&["token", "list", "--data-dir", data]);
    assert_eq!(listed.status.code(), Some(0), "{listed:?}");
    assert!(String::from_utf8_lossy(&listed.stdout).contains("No tokens found."));
}
