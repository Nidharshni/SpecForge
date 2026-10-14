//! Every adapter of the `Registry` port keeps one contract (ADR 0044): the configured registry over the
//! real server in process, and over the in-memory client. `MemoryRegistry` keeps it in `specforge-ops`.

use specforge_ops::registry::testing::{Published, assert_registry_contract, contract_packages};
use specforge_ops_registry::{ConfiguredRegistry, User};
use specforge_project::testing::TestProject;
use specforge_registry_client::testing::{MemoryClient, package};
use specforge_registry_client::{RegistryConfig, SigningKey};
use specforge_registry_server::testing::LocalRegistry;
use specforge_registry_wire::{PackageMetadata, RegistryRoot};

/// What a registry stores for `published`, signed by `key` when it names a signer.
fn stored(published: &Published, key: &SigningKey) -> PackageMetadata {
    let manifest = serde_json::to_string(&published.declaration).unwrap();
    package(
        published.name().as_str(),
        &published.version().to_string(),
        &published.wasm,
        &manifest,
        published.key_id.is_some().then_some(key),
    )
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
    let key = SigningKey::generate();
    let published = contract_packages(&key.key_id());
    for p in &published {
        server.store(&stored(p, &key), &p.wasm);
    }
    let dir = project(server.config_entry());
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
    let key = SigningKey::generate();
    let published = contract_packages(&key.key_id());
    for p in &published {
        client.store(&config, stored(p, &key), p.wasm.clone());
    }
    let dir = project(serde_json::json!([
        { "alias": "local", "url": "https://local.invalid", "default_registry": true }
    ]));
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add")
        .with_client(client)
        .as_user(User::at(dir.home(), Some("token".to_string())));

    assert_registry_contract(&registry, &published);
}

#[test]
fn a_published_manifest_is_read_without_downloading_the_binary() {
    use specforge_ops::registry::Registry;
    use specforge_package::{PackageName, Version};
    use specforge_registry_client::testing::CallKind;

    let config = RegistryConfig {
        alias: "local".to_string(),
        url: RegistryRoot::parse("https://local.invalid").unwrap(),
        scope_filter: None,
        default_registry: true,
    };
    let client = MemoryClient::new().accepting("token");
    let key = SigningKey::generate();
    let published = contract_packages(&key.key_id());
    for p in &published {
        client.store(&config, stored(p, &key), p.wasm.clone());
    }
    let dir = project(serde_json::json!([
        { "alias": "local", "url": "https://local.invalid", "default_registry": true }
    ]));
    let registry = ConfiguredRegistry::for_project(&dir.project_root(), "add")
        .with_client(client.clone())
        .as_user(User::at(dir.home(), Some("token".to_string())));
    let base = PackageName::parse("@contract/base").unwrap();

    let manifest = registry
        .published(&base, &Version::parse("1.1.0").unwrap())
        .unwrap();

    assert_eq!(manifest.name, base);
    assert!(!manifest.sha256.is_empty());
    let kinds: Vec<CallKind> = client.calls().iter().map(|call| call.kind).collect();
    assert_eq!(kinds, [CallKind::Metadata], "no download: {kinds:?}");
}
