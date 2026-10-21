//! `ConfiguredRegistry::publish`: the registry for the name, the credential, the signing key, then the
//! signed upload (ADR 0045). Each test runs the adapter over a recording in-memory client (ADR 0044) as a
//! user whose files live in a temporary directory, never in `~/.specforge`.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_ops::OpErrorKind;
use specforge_ops::asking::Asking;
use specforge_ops::registry::testing::declaration;
use specforge_ops::registry::{Registry, TrustPolicy};
use specforge_ops_registry::{ConfiguredRegistry, User};
use specforge_package::DeclaredPackage;
use specforge_project::testing::TestProject;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry_client::RegistryError;
use specforge_registry_client::credentials::{CredentialEntry, CredentialStore};
use specforge_registry_client::testing::{CallKind, MemoryClient};
use specforge_registry_wire::SignedContent;
use specforge_test_macros::test as specforge_test;
use tempfile::TempDir;

/// A project, the user's home, and the registry both talk to.
struct World {
    project: TestProject,
    client: MemoryClient,
}

impl World {
    /// A project configuring `registries`; the client accepts the token `"t"`.
    fn with(registries: serde_json::Value) -> Self {
        let project = TestProject::new().config(|c| {
            c["name"] = "p".into();
            c["registries"] = registries;
        });
        World {
            project,
            client: MemoryClient::new().accepting("t"),
        }
    }

    /// One registry, `acme`, serving `@acme`.
    fn acme() -> Self {
        Self::with(serde_json::json!([
            { "alias": "acme", "url": "https://acme.invalid", "scope_filter": "@acme" }
        ]))
    }

    fn registry(&self, token: Option<&str>) -> ConfiguredRegistry {
        ConfiguredRegistry::for_project(&self.project.project_root(), "publish")
            .as_user(User::at(self.project.home(), token.map(str::to_string)))
            .with_client(self.client.clone())
    }

    fn signing_key(&self) -> std::path::PathBuf {
        self.project.home().join("signing-key.json")
    }

    fn store_credential(&self, root: &str, token: &str, expires_at: Option<&str>) {
        let mut store = CredentialStore::empty(self.project.home());
        store.registries.insert(
            specforge_registry_wire::RegistryRoot::parse(root).unwrap(),
            CredentialEntry::Token {
                token: token.to_string(),
                expires_at: expires_at.map(str::to_string),
            },
        );
        store.write().unwrap();
    }
}

/// The binary every publish here uploads.
const WASM: &[u8] = b"\0asm publish test";

/// A package to publish.
struct Pkg {
    declaration: ExtensionDeclaration,
}

fn pkg(name: &str, version: &str) -> Pkg {
    Pkg {
        declaration: declaration(name, version, &[]),
    }
}

impl Pkg {
    fn package(&self) -> DeclaredPackage<'_> {
        DeclaredPackage::of(&self.declaration).unwrap()
    }
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "publish refuses in one order, each refusal before anything after it is read or asked"
)]
fn the_adapter_refuses_before_any_request_in_this_order() {
    // 1. no specforge.json: E063.
    let empty = TestProject::new().without_config();
    let home = TempDir::new().unwrap();
    let client = MemoryClient::new().accepting("t");
    let error = ConfiguredRegistry::for_project(&empty.project_root(), "publish")
        .as_user(User::at(home.path(), Some("t".into())))
        .with_client(client.clone())
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();
    assert!(
        error.is(specforge_ops::registry::NO_REGISTRY),
        "1: {error:?}"
    );
    assert!(client.calls().is_empty());

    // 2. no registry serves @other: R-OPS-001.
    let world = World::acme();
    let error = world
        .registry(Some("t"))
        .publish(&pkg("@other/x", "1.0.0").package(), WASM)
        .unwrap_err();
    assert_eq!(error.code, "R-OPS-001", "2: {error:?}");
    assert!(world.client.calls().is_empty());
    assert!(!world.signing_key().exists());

    // 3. acme serves it, there is no credential: R001, naming the registry and how to log in.
    let error = world
        .registry(None)
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();
    assert_eq!(error.code, "R001", "3: {error:?}");
    assert_eq!(error.kind, OpErrorKind::PreconditionFailed, "3: {error:?}");
    assert!(error.message.contains("'acme'"), "3: {error:?}");
    let suggestion = error.suggestion.clone().unwrap_or_default();
    assert!(
        suggestion.contains("specforge login --registry acme --token"),
        "3: {suggestion}"
    );
    assert!(
        suggestion.contains("SPECFORGE_REGISTRY_TOKEN"),
        "3: {suggestion}"
    );
    assert!(world.client.calls().is_empty());
    assert!(!world.signing_key().exists());

    // 4. credentials.json is not JSON and there is no token: R012.
    std::fs::write(world.project.home().join("credentials.json"), "{").unwrap();
    let error = world
        .registry(None)
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();
    assert_eq!(error.code, "R012", "4: {error:?}");
    assert!(world.client.calls().is_empty());
    assert!(!world.signing_key().exists());

    // 5. a stored token that expired, and no token: R-AUTH-020.
    world.store_credential(
        "https://acme.invalid",
        "stored",
        Some("2000-01-01T00:00:00Z"),
    );
    let error = world
        .registry(None)
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();
    assert_eq!(error.code, "R-AUTH-020", "5: {error:?}");
    assert!(world.client.calls().is_empty());
    assert!(!world.signing_key().exists());

    // 6. a token, and a signing key that is not a key: E074, the file left as it was.
    std::fs::write(world.signing_key(), "not json").unwrap();
    let error = world
        .registry(Some("t"))
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();
    assert_eq!(error.code, "E074", "6: {error:?}");
    assert_eq!(error.kind, OpErrorKind::PreconditionFailed, "6: {error:?}");
    assert!(world.client.calls().is_empty());
    assert_eq!(
        std::fs::read_to_string(world.signing_key()).unwrap(),
        "not json"
    );
    std::fs::remove_file(world.signing_key()).unwrap();

    // 7. all good: one publish call to acme, the user's first key created; the second reuses it.
    let first = world
        .registry(Some("t"))
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap();
    assert_eq!(first.registry, "acme");
    assert!(first.key_created);
    assert!(world.signing_key().exists());
    let calls = world.client.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].kind, CallKind::Publish);
    assert_eq!(calls[0].registry.as_deref(), Some("acme"));
    let second = world
        .registry(Some("t"))
        .publish(&pkg("@acme/x", "1.0.1").package(), WASM)
        .unwrap();
    assert!(!second.key_created);
    assert_eq!(second.key_id, first.key_id);
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "with no credential for its registry, publish refuses with R001 before any network call and creates no signing key"
)]
fn with_no_credential_the_adapter_sends_nothing_and_creates_no_key() {
    let world = World::acme();
    let error = world
        .registry(None)
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();
    assert_eq!(error.code, "R001", "{error:?}");
    assert!(world.client.calls().is_empty());
    assert!(!world.signing_key().exists(), "no key is created");
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "the environment token wins over the stored credential"
)]
fn the_environment_token_wins_over_the_stored_credential() {
    let published_with = |world: &World, token: Option<&str>, version: &str| {
        world
            .registry(token)
            .publish(&pkg("@acme/x", version).package(), WASM)
            .map(|_| {
                world
                    .client
                    .calls()
                    .last()
                    .unwrap()
                    .credential
                    .clone()
                    .unwrap()
            })
    };

    let world = World::acme();
    world.client.clone().accepting("env").accepting("stored");
    world.store_credential("https://acme.invalid", "stored", None);

    let credential = published_with(&world, Some("env"), "1.0.0").unwrap();
    assert_eq!(credential.root.as_str(), "https://acme.invalid");
    assert_eq!(credential.token(), "env");

    let credential = published_with(&world, Some("  "), "1.0.1").unwrap();
    assert_eq!(credential.token(), "stored");

    let credential = published_with(&world, None, "1.0.2").unwrap();
    assert_eq!(credential.token(), "stored");

    // Only another registry's credential is stored: there is none for acme.
    let other = World::acme();
    other.store_credential("https://other.invalid", "stored", None);
    let error = published_with(&other, None, "1.0.0").unwrap_err();
    assert_eq!(error.code, "R001", "{error:?}");
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "a signing key that can't be read is refused with E074 before any network call"
)]
fn a_corrupt_signing_key_is_refused_with_e074_before_any_request() {
    let world = World::acme();
    std::fs::write(world.signing_key(), "not json").unwrap();
    let error = world
        .registry(Some("t"))
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();
    assert_eq!(error.code, "E074", "{error:?}");
    assert_eq!(error.kind, OpErrorKind::PreconditionFailed);
    assert!(
        error
            .suggestion
            .unwrap_or_default()
            .contains("signing-key.json"),
        "the suggestion names the file"
    );
    assert!(world.client.calls().is_empty());
}

#[test]
fn the_e074_suggestion_reads_as_one_sentence() {
    let world = World::acme();
    std::fs::write(world.signing_key(), "not json").unwrap();
    let error = world
        .registry(Some("t"))
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();
    assert_eq!(error.code, "E074", "{error:?}");
    let suggestion = error.suggestion.unwrap_or_default();
    assert!(!suggestion.contains("  "), "{suggestion}");
    assert!(
        suggestion.ends_with("whoever pinned the old one then sees a changed key (R-TRUST-003)"),
        "{suggestion}"
    );
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "publish asks the registry add fetches the same name from"
)]
fn publish_asks_the_registry_fetch_asks() {
    let world = World::with(serde_json::json!([
        { "alias": "acme", "url": "https://acme.invalid", "scope_filter": "@acme" },
        { "alias": "main", "url": "https://main.invalid", "default_registry": true }
    ]));
    let registry = world.registry(Some("t"));
    let acme = pkg("@acme/x", "1.0.0");
    let acme = acme.package();
    registry.publish(&acme, WASM).unwrap();
    registry
        .fetch(
            acme.name(),
            acme.version(),
            TrustPolicy::new(false, Asking::Nobody),
        )
        .unwrap();
    let other = pkg("@other/y", "1.0.0");
    let other = other.package();
    registry.publish(&other, WASM).unwrap();
    registry.versions(other.name()).unwrap();

    let asked: Vec<(CallKind, String)> = world
        .client
        .calls()
        .into_iter()
        .map(|c| (c.kind, c.registry.unwrap_or_default()))
        .collect();
    assert_eq!(
        asked,
        [
            (CallKind::Publish, "acme".to_string()),
            (CallKind::Metadata, "acme".to_string()),
            (CallKind::Download, "acme".to_string()),
            (CallKind::Publish, "main".to_string()),
            (CallKind::Versions, "main".to_string()),
        ]
    );
}

#[test]
fn a_package_is_signed_with_the_users_key() {
    let world = World::acme();
    let package = pkg("@acme/x", "1.0.0");
    let published = world
        .registry(Some("t"))
        .publish(&package.package(), WASM)
        .unwrap();

    let call = world.client.calls().remove(0);
    let signature = call.signature.unwrap();
    assert_eq!(signature.key_id(), published.key_id);
    signature
        .verify(&SignedContent::of(
            "@acme/x",
            "1.0.0",
            WASM,
            &call.manifest.unwrap(),
        ))
        .unwrap();
}

#[test]
fn a_version_the_registry_holds_is_r007() {
    let world = World::acme();
    world.client.fail_next(
        CallKind::Publish,
        None,
        RegistryError::DuplicateVersion {
            name: "@acme/x".into(),
            version: "1.0.0".into(),
        },
    );
    let error = world
        .registry(Some("t"))
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();
    assert_eq!(error.code, "R007", "{error:?}");
    assert_eq!(error.kind, OpErrorKind::Conflict);
}

#[test]
fn a_credential_the_registry_refuses_is_a_permission_error() {
    let world = World::acme();
    let error = world
        .registry(Some("not accepted"))
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();
    assert_eq!(error.code, "R001", "{error:?}");
    assert_eq!(error.kind, OpErrorKind::PermissionDenied);
}

/// A project on `server`, and the user's home, for a publish over HTTP.
fn on_server(server: &specforge_registry_server::testing::LocalRegistry) -> (TestProject, TempDir) {
    let project = TestProject::new().config(|c| {
        c["name"] = "p".into();
        c["registries"] = server.config_entry();
    });
    (project, TempDir::new().unwrap())
}

#[specforge_test(
    behavior = "retry_registry_request",
    verify = "a Retry-After longer than the longest backoff is not waited for"
)]
fn a_retry_after_beyond_the_longest_backoff_is_not_waited_for() {
    use specforge_registry_server::state::PublishLimits;
    let server = specforge_registry_server::testing::LocalRegistry::start_with(PublishLimits {
        per_token: 1,
        per_ip: 100,
        window: std::time::Duration::from_secs(60),
    });
    let (project, home) = on_server(&server);
    let registry = ConfiguredRegistry::for_project(&project.project_root(), "publish")
        .as_user(User::at(home.path(), Some(server.token().to_string())));

    registry
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .expect("the first publish is within the limit");
    let started = std::time::Instant::now();
    let error = registry
        .publish(&pkg("@acme/x", "1.0.1").package(), WASM)
        .unwrap_err();

    assert_eq!(error.code, "R003", "{error:?}");
    // The 60 s window's Retry-After exceeds the longest backoff: no wait, no second request.
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    let puts = server
        .requests()
        .iter()
        .filter(|request| request.starts_with("PUT"))
        .count();
    assert_eq!(puts, 2, "{:?}", server.requests());
}

#[specforge_test(
    behavior = "retry_registry_request",
    verify = "a rate-limited publish is sent again once the registry's wait has passed"
)]
fn a_rate_limited_publish_is_sent_again_after_its_wait() {
    use specforge_registry_server::state::PublishLimits;
    let server = specforge_registry_server::testing::LocalRegistry::start_with(PublishLimits {
        per_token: 1,
        per_ip: 100,
        window: std::time::Duration::from_secs(1),
    });
    let (project, home) = on_server(&server);
    let registry = ConfiguredRegistry::for_project(&project.project_root(), "publish")
        .as_user(User::at(home.path(), Some(server.token().to_string())));

    registry
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .expect("the first publish is within the limit");
    registry
        .publish(&pkg("@acme/x", "1.0.1").package(), WASM)
        .expect("the second is sent again after the window");

    let puts = server
        .requests()
        .iter()
        .filter(|request| request.starts_with("PUT"))
        .count();
    assert_eq!(puts, 3, "{:?}", server.requests());
}

/// `home` with `credentials.json` holding `json`.
fn home_with_credentials(json: &str) -> TempDir {
    let home = TempDir::new().unwrap();
    std::fs::write(home.path().join("credentials.json"), json).unwrap();
    home
}

#[specforge_test(
    behavior = "authenticate_registry_request",
    verify = "missing token source produces ExtensionError"
)]
fn an_unset_token_variable_is_r010_before_any_request() {
    let world = World::acme();
    let home = home_with_credentials(
        r#"{"registries":{"https://acme.invalid":{"token_env":"P16_UNSET_TOKEN_VARIABLE"}}}"#,
    );
    let registry = ConfiguredRegistry::for_project(&world.project.project_root(), "publish")
        .as_user(User::at(home.path(), None))
        .with_client(world.client.clone());

    let error = registry
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();

    assert_eq!(error.code, "R010", "{error:?}");
    assert_eq!(error.kind, specforge_ops::OpErrorKind::PreconditionFailed);
    assert!(
        error.message.contains("P16_UNSET_TOKEN_VARIABLE"),
        "{error:?}"
    );
    assert!(world.client.calls().is_empty());
}

#[specforge_test(
    behavior = "authenticate_registry_request",
    verify = "token resolved from environment variable"
)]
#[expect(
    unsafe_code,
    reason = "set_var before any thread of this test reads the environment; the test runs in its own process under nextest"
)]
fn a_token_variable_is_sent_as_the_bearer_token() {
    let world = World::acme();
    let home = home_with_credentials(
        r#"{"registries":{"https://acme.invalid":{"token_env":"P16_TOKEN_A"}}}"#,
    );
    // SAFETY: the variable is named by this test only.
    unsafe { std::env::set_var("P16_TOKEN_A", "t") };
    let registry = ConfiguredRegistry::for_project(&world.project.project_root(), "publish")
        .as_user(User::at(home.path(), None))
        .with_client(world.client.clone());

    registry
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap();

    let calls = world.client.calls();
    assert_eq!(calls[0].credential.as_ref().unwrap().token(), "t");
}

#[specforge_test(
    behavior = "authenticate_registry_request",
    verify = "token resolved from token file"
)]
fn a_token_file_is_sent_as_the_bearer_token() {
    let world = World::acme();
    let file = world.project.home().join("tok");
    std::fs::write(
        &file, "t
",
    )
    .unwrap();
    let home = home_with_credentials(
        &serde_json::json!({"registries": {"https://acme.invalid": {"token_file": file}}})
            .to_string(),
    );
    let registry = ConfiguredRegistry::for_project(&world.project.project_root(), "publish")
        .as_user(User::at(home.path(), None))
        .with_client(world.client.clone());

    registry
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap();

    let calls = world.client.calls();
    assert_eq!(calls[0].credential.as_ref().unwrap().token(), "t");
}

#[specforge_test(
    behavior = "authenticate_registry_request",
    verify = "a token file that can't be read is R011 before any request"
)]
fn an_unreadable_token_file_is_r011_before_any_request() {
    let world = World::acme();
    let home = home_with_credentials(
        r#"{"registries":{"https://acme.invalid":{"token_file":"/nonexistent/p16-token"}}}"#,
    );
    let registry = ConfiguredRegistry::for_project(&world.project.project_root(), "publish")
        .as_user(User::at(home.path(), None))
        .with_client(world.client.clone());

    let error = registry
        .publish(&pkg("@acme/x", "1.0.0").package(), WASM)
        .unwrap_err();

    assert_eq!(error.code, "R011", "{error:?}");
    assert_eq!(error.kind, specforge_ops::OpErrorKind::PreconditionFailed);
    assert!(world.client.calls().is_empty());
}
