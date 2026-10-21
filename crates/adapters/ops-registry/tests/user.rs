//! `User`, the user's registry files (ADR 0178): what a read of a registry carries from them.

use specforge_ops::registry::Registry;
use specforge_ops_registry::{ConfiguredRegistry, User};
use specforge_package::PackageName;
use specforge_project::testing::TestProject;
use specforge_registry_client::testing::MemoryClient;
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "authenticate_registry_request",
    verify = "a kept token whose secret is missing is R-AUTH-021 before any request"
)]
#[ignore = "flipped by T6"]
fn a_kept_token_whose_secret_is_gone_refuses_the_read() {
    let project = TestProject::new().config(|c| {
        c["name"] = "p".into();
        c["registries"] = serde_json::json!([
            { "alias": "local", "url": "https://local.invalid", "default_registry": true }
        ]);
    });
    let client = MemoryClient::new();
    // An entry whose secret `login --token` kept is gone: nothing under secrets/.
    std::fs::create_dir_all(project.home()).unwrap();
    std::fs::write(
        project.home().join("credentials.json"),
        r#"{"registries":{"https://local.invalid":{}}}"#,
    )
    .unwrap();
    let registry = ConfiguredRegistry::for_project(&project.project_root(), "add")
        .with_client(client.clone())
        .as_user(User::at(project.home(), None));

    let outcome = registry.versions(&PackageName::parse("@acme/tool").unwrap());

    let calls = client.calls();
    let error = outcome.expect_err("a lost secret is not an empty bearer token");
    assert_eq!(error.code, "R-AUTH-021", "{error:?} (calls: {calls:?})");
    assert!(calls.is_empty(), "{calls:?}");
}

#[specforge_test(
    behavior = "support_private_registries",
    verify = "a read carries the credential kept for its registry's root, and none kept for another root"
)]
fn a_read_carries_the_credential_kept_for_its_root_only() {
    // Two entries with one alias and two roots (W140): they do not share a credential.
    let project = TestProject::new().config(|c| {
        c["name"] = "p".into();
        c["registries"] = serde_json::json!([
            { "alias": "main", "url": "https://a.invalid", "scope_filter": "@a" },
            { "alias": "main", "url": "https://b.invalid", "default_registry": true }
        ]);
    });
    std::fs::create_dir_all(project.home()).unwrap();
    std::fs::write(
        project.home().join("credentials.json"),
        r#"{"registries":{"https://a.invalid":{"token":"ta"}}}"#,
    )
    .unwrap();
    let client = MemoryClient::new();
    let registry = ConfiguredRegistry::for_project(&project.project_root(), "add")
        .with_client(client.clone())
        .as_user(User::at(project.home(), None));

    let _ = registry.versions(&PackageName::parse("@a/x").unwrap());
    let _ = registry.versions(&PackageName::parse("@b/x").unwrap());

    let sent: Vec<Option<String>> = client
        .calls()
        .iter()
        .map(|call| call.credential.as_ref().map(|c| c.token().to_string()))
        .collect();
    assert_eq!(sent, [Some("ta".to_string()), None]);
}
