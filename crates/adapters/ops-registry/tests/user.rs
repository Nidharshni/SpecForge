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
        r#"{"registries":{"local":{}}}"#,
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
