//! `User`, the user's registry files (ADR 0178): what a read of a registry carries from them.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_ops::registry::Registry;
use specforge_ops_registry::{ConfiguredRegistry, Kept, RegistryRoot, TokenSource, User};
use specforge_package::PackageName;
use specforge_project::testing::TestProject;
use specforge_registry_client::testing::MemoryClient;
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "authenticate_registry_request",
    verify = "a kept token whose secret is missing is R-AUTH-021 before any request"
)]
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

// ── login and logout ──────────────────────────────────────────────────────────────────────────────

/// A project with one registry, alias `main`, at `https://main.invalid`; a client accepting `t`.
fn project_on_main() -> (TestProject, MemoryClient) {
    let project = TestProject::new().config(|c| {
        c["name"] = "p".into();
        c["registries"] = serde_json::json!([
            { "alias": "main", "url": "https://main.invalid", "default_registry": true }
        ]);
    });
    (project, MemoryClient::new().accepting("t"))
}

fn registry_for(
    project: &TestProject,
    client: &MemoryClient,
    operation: &str,
) -> ConfiguredRegistry {
    ConfiguredRegistry::for_project(&project.project_root(), operation)
        .with_client(client.clone())
        .as_user(User::at(project.home(), None))
}

fn root(text: &str) -> RegistryRoot {
    RegistryRoot::parse(text).unwrap()
}

fn store_of(project: &TestProject) -> serde_json::Value {
    let text = std::fs::read_to_string(project.home().join("credentials.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn kept_roots(project: &TestProject) -> Vec<String> {
    let mut roots: Vec<String> = store_of(project)["registries"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    roots.sort();
    roots
}

#[specforge_test(
    behavior = "validate_registry_credentials",
    verify = "login stores the credential under the root of the registry it validated against"
)]
fn login_keeps_the_credential_under_the_registrys_root() {
    let (project, client) = project_on_main();

    let logged_in = registry_for(&project, &client, "login")
        .log_in(None, TokenSource::Given("t".into()))
        .unwrap();

    assert_eq!(logged_in.alias, "main");
    assert_eq!(logged_in.root, root("https://main.invalid"));
    assert_eq!(kept_roots(&project), ["https://main.invalid"]);
}

#[specforge_test(
    behavior = "validate_registry_credentials",
    verify = "raw token never stored in specforge.json"
)]
fn login_with_a_token_keeps_the_secret_out_of_the_store() {
    let (project, client) = project_on_main();

    let logged_in = registry_for(&project, &client, "login")
        .log_in(Some("main"), TokenSource::Given("t".into()))
        .unwrap();

    assert_eq!(logged_in.kept, Kept::Secret);
    let store = std::fs::read_to_string(project.home().join("credentials.json")).unwrap();
    assert!(!store.contains("\"t\""), "{store}");
    let secrets: Vec<_> = std::fs::read_dir(project.home().join("secrets"))
        .unwrap()
        .collect();
    assert_eq!(secrets.len(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = secrets[0]
            .as_ref()
            .unwrap()
            .metadata()
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    // And the kept secret is what a read then carries.
    let read = registry_for(&project, &client, "add");
    let _ = read.versions(&PackageName::parse("@acme/x").unwrap());
    let sent = client.calls().last().unwrap().credential.clone().unwrap();
    assert_eq!(sent.token(), "t");
}

#[specforge_test(
    behavior = "validate_registry_credentials",
    verify = "login refuses an unreadable credential store before any request, and keeps it"
)]
fn login_refuses_an_unreadable_store_before_any_request_and_keeps_it() {
    let (project, client) = project_on_main();
    std::fs::create_dir_all(project.home()).unwrap();
    let broken = r#"{"registries": {},}"#;
    std::fs::write(project.home().join("credentials.json"), broken).unwrap();

    let error = registry_for(&project, &client, "login")
        .log_in(None, TokenSource::Given("t".into()))
        .unwrap_err();

    assert_eq!(error.code, "R012", "{error:?}");
    assert!(client.calls().is_empty());
    assert_eq!(
        std::fs::read_to_string(project.home().join("credentials.json")).unwrap(),
        broken
    );
}

#[specforge_test(
    behavior = "validate_registry_credentials",
    verify = "invalid credentials produce error with guidance"
)]
fn a_token_the_registry_refuses_writes_nothing() {
    let (project, client) = project_on_main();

    let error = registry_for(&project, &client, "login")
        .log_in(None, TokenSource::Given("x".into()))
        .unwrap_err();

    assert_eq!(error.code, "R001", "{error:?}");
    assert!(!project.home().join("credentials.json").exists());
}

#[specforge_test(
    behavior = "validate_registry_credentials",
    verify = "two registries with one alias keep two credentials, one per root"
)]
fn two_entries_with_one_alias_keep_two_credentials() {
    let (project, client) = project_on_main();
    registry_for(&project, &client, "login")
        .log_in(Some("main"), TokenSource::Given("t".into()))
        .unwrap();
    // Another project, with another root under the same alias, for the same user.
    let other = TestProject::new().config(|c| {
        c["name"] = "q".into();
        c["registries"] = serde_json::json!([
            { "alias": "main", "url": "https://other.invalid", "default_registry": true }
        ]);
    });
    ConfiguredRegistry::for_project(&other.project_root(), "login")
        .with_client(client.clone())
        .as_user(User::at(project.home(), None))
        .log_in(Some("main"), TokenSource::Given("t".into()))
        .unwrap();

    assert_eq!(
        kept_roots(&project),
        ["https://main.invalid", "https://other.invalid"]
    );
}

#[specforge_test(
    behavior = "logout_registry",
    verify = "credentials for other registry roots remain untouched"
)]
fn logout_forgets_only_its_roots_credential() {
    let (project, client) = project_on_main();
    registry_for(&project, &client, "login")
        .log_in(None, TokenSource::Given("t".into()))
        .unwrap();
    // A second root's credential, kept beside it.
    let mut store = store_of(&project);
    store["registries"]["https://other.invalid"] = serde_json::json!({"token_env": "R20_OTHER"});
    std::fs::write(project.home().join("credentials.json"), store.to_string()).unwrap();

    let out = User::at(project.home(), None)
        .log_out(&root("https://main.invalid"))
        .unwrap();

    assert!(out.removed);
    assert_eq!(kept_roots(&project), ["https://other.invalid"]);
    let left = std::fs::read_dir(project.home().join("secrets")).unwrap();
    assert_eq!(left.count(), 0, "its secret file is gone");
}

#[specforge_test(
    behavior = "logout_registry",
    verify = "no credential for the root succeeds silently"
)]
fn logout_of_a_root_with_nothing_kept_succeeds_silently() {
    let (project, _) = project_on_main();

    let out = User::at(project.home(), None)
        .log_out(&root("https://main.invalid"))
        .unwrap();

    assert!(!out.removed);
    assert!(!project.home().join("credentials.json").exists());
}
