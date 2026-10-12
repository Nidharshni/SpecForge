//! Every adapter of the `Registry` port keeps one contract (ADR 0044): the configured registry over the
//! real server in process, and over the in-memory client. `MemoryRegistry` keeps it in `specforge-ops`.

use specforge_ops::registry::testing::{Published, assert_registry_contract, contract_packages};
use specforge_ops_registry::{ConfiguredRegistry, User};
use specforge_registry_client::testing::{MemoryClient, package};
use specforge_registry_client::{RegistryConfig, SigningKey};
use specforge_registry_server::testing::LocalRegistry;
use specforge_registry_wire::PackageMetadata;
use tempfile::TempDir;

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
fn project(registry: serde_json::Value) -> TempDir {
    let dir = TempDir::new().unwrap();
    let config = serde_json::json!({ "name": "p", "version": "0.1.0", "registries": registry });
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    dir
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
    let registry = ConfiguredRegistry::for_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        "add",
    )
    .as_user(User::at(dir.path(), Some(server.token().to_string())));

    assert_registry_contract(&registry, &published);
}

#[specforge_test_macros::test(port = "Registry", verify = "Registry contract is satisfied")]
fn the_configured_registry_over_memory_keeps_the_registry_contract() {
    let config = RegistryConfig {
        alias: "local".to_string(),
        url: "memory://local".to_string(),
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
        { "alias": "local", "url": "memory://local", "default_registry": true }
    ]));
    let registry = ConfiguredRegistry::for_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        "add",
    )
    .with_client(client)
    .as_user(User::at(dir.path(), Some("token".to_string())));

    assert_registry_contract(&registry, &published);
}
