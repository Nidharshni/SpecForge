//! A `credentials.json` that cannot be read is R012 for every reader (ADR 0045, amended in round 16):
//! login and logout refuse before they write or delete anything.

use crate::support::registry::project_on;
use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;

/// A trailing comma: the file exists, and does not parse.
const BROKEN: &str = r#"{"registries": {"local": {"token_env": "P11_LOGIN_TOKEN"}, "other": {"token_file": "/x"},}}"#;

fn write_broken_store(dir: &TestProject) {
    let path = dir.home().join(".specforge/credentials.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, BROKEN).unwrap();
}

fn stored(dir: &TestProject) -> String {
    std::fs::read_to_string(dir.home().join(".specforge/credentials.json")).unwrap()
}

#[specforge_test(
    behavior = "validate_registry_credentials",
    verify = "login refuses an unreadable credential store before any request, and keeps it"
)]
fn login_refuses_an_unreadable_credential_store_and_keeps_it() {
    let registry = crate::support::registry::serve(vec![]);
    let dir = project_on(&registry);
    write_broken_store(&dir);

    let output = Cli::on(&dir)
        .command(&[
            "login",
            "--registry",
            "local",
            "--token-env",
            "P11_LOGIN_TOKEN",
            "--format",
            "json",
        ])
        .env("P11_LOGIN_TOKEN", registry.token())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["code"], "R012", "{json}");
    assert_eq!(stored(&dir), BROKEN);
}

#[specforge_test(
    behavior = "logout_registry",
    verify = "logout refuses an unreadable credential store and deletes no secret"
)]
fn logout_refuses_an_unreadable_credential_store_and_keeps_the_secret() {
    // A secret kept by a real `login --token`; then the store breaks.
    let registry = crate::support::registry::serve(vec![]);
    let dir = project_on(&registry);
    let login = Cli::on(&dir)
        .command(&["login", "--token", registry.token()])
        .output()
        .unwrap();
    assert!(login.status.success(), "{login:?}");
    let secrets = dir.home().join(".specforge/secrets");
    let kept = || std::fs::read_dir(&secrets).unwrap().count();
    assert_eq!(kept(), 1);
    write_broken_store(&dir);

    let output = Cli::on(&dir)
        .bare_command(&[
            "logout",
            "--url",
            registry.root().as_str(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["code"], "R012", "{json}");
    assert_eq!(stored(&dir), BROKEN);
    assert_eq!(kept(), 1, "the secret was deleted");
}
