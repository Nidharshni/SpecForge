//! Every adapter of the `Registry` port keeps one contract (ADR 0044): the configured registry over the
//! real server in process, and over the in-memory client. `MemoryRegistry` keeps it in `specforge-ops`.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_ops::registry::testing::{Published, assert_registry_contract, contract_packages};
use specforge_ops_registry::{ConfiguredRegistry, User};
use specforge_project::testing::TestProject;
use specforge_registry_client::testing::{MemoryClient, package};
use specforge_registry_client::{RegistryConfig, SigningKey};
use specforge_registry_server::testing::LocalRegistry;
use specforge_registry_wire::{PackageMetadata, RegistryRoot};

/// What a registry stores for `published`, signed by the one of `keys` whose id it names as its signer.
fn stored(published: &Published, keys: &[&SigningKey]) -> PackageMetadata {
    let manifest = serde_json::to_string(&published.declaration).unwrap();
    let signer = published
        .key_id
        .as_ref()
        .map(|id| *keys.iter().find(|key| &key.key_id() == id).unwrap());
    package(
        published.name().as_str(),
        &published.version().to_string(),
        &published.wasm,
        &manifest,
        signer,
    )
}

/// The user's `known-keys.json`, denying `key`: what the contract's precondition asks of the adapter.
fn denying(home: &std::path::Path, key: &SigningKey) {
    let mut known = specforge_registry_client::KnownKeys::default();
    known.denied_keys.push(key.key_id());
    specforge_registry_client::save_known_keys_at(&home.join("known-keys.json"), &known).unwrap();
}

/// A project whose only registry is `registry`, and a temporary known-keys store.
fn project(registry: serde_json::Value) -> TestProject {
    TestProject::new().config(|c| {
        c["name"] = "p".into();
        c["registries"] = registry;
    })
}

#[specforge_test_macros::test(port = "Registry", verify = "Registry contract is satisfied")]
fn the_configured_registry_over_http_keeps_the_registry_contract() {
    let server = LocalRegistry::start();
    let (key, denied) = (SigningKey::generate(), SigningKey::generate());
    let published = contract_packages(&key.key_id(), &denied.key_id());
    for p in &published {
        server.store(&stored(p, &[&key, &denied]), &p.wasm);
    }
    let dir = project(server.config_entry());
    denying(dir.home(), &denied);
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add")
        .as_user(User::at(dir.home(), Some(server.token().to_string())));

    assert_registry_contract(&registry, &published);
}

#[specforge_test_macros::test(port = "Registry", verify = "Registry contract is satisfied")]
fn the_configured_registry_over_memory_keeps_the_registry_contract() {
    let config = RegistryConfig {
        alias: "local".to_string(),
        url: RegistryRoot::parse("https://local.invalid").unwrap(),
        scope_filter: None,
        default_registry: true,
    };
    let client = MemoryClient::new().accepting("token");
    let (key, denied) = (SigningKey::generate(), SigningKey::generate());
    let published = contract_packages(&key.key_id(), &denied.key_id());
    for p in &published {
        client.store(&config, stored(p, &[&key, &denied]), p.wasm.clone());
    }
    let dir = project(serde_json::json!([
        { "alias": "local", "url": "https://local.invalid", "default_registry": true }
    ]));
    denying(dir.home(), &denied);
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add")
        .with_client(client)
        .as_user(User::at(dir.home(), Some("token".to_string())));

    assert_registry_contract(&registry, &published);
}

#[test]
fn a_published_manifest_is_read_without_downloading_the_binary() {
    use specforge_ops::asking::Asking;
    use specforge_ops::registry::{Registry, TrustPolicy};
    use specforge_package::{PackageName, Version};
    use specforge_registry_client::testing::CallKind;

    let config = RegistryConfig {
        alias: "local".to_string(),
        url: RegistryRoot::parse("https://local.invalid").unwrap(),
        scope_filter: None,
        default_registry: true,
    };
    let client = MemoryClient::new().accepting("token");
    let (key, denied) = (SigningKey::generate(), SigningKey::generate());
    let published = contract_packages(&key.key_id(), &denied.key_id());
    for p in &published {
        client.store(&config, stored(p, &[&key, &denied]), p.wasm.clone());
    }
    let dir = project(serde_json::json!([
        { "alias": "local", "url": "https://local.invalid", "default_registry": true }
    ]));
    denying(dir.home(), &denied);
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add")
        .with_client(client.clone())
        .as_user(User::at(dir.home(), Some("token".to_string())));
    let base = PackageName::parse("@contract/base").unwrap();

    let manifest = registry
        .published(
            &base,
            &Version::parse("1.1.0").unwrap(),
            TrustPolicy::new(false, Asking::Nobody),
        )
        .unwrap();

    assert_eq!(manifest.name, base);
    assert!(!manifest.sha256.is_empty());
    let kinds: Vec<CallKind> = client.calls().iter().map(|call| call.kind).collect();
    assert_eq!(kinds, [CallKind::Metadata], "no download: {kinds:?}");
}
