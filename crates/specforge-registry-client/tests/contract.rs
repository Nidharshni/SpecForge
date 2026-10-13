//! Every adapter of the `RegistryClient` seam keeps one contract (ADR 0044): the HTTP client against the
//! real server in process, and the in-memory client.

use specforge_package::{PackageName, Version};
use specforge_registry_client::registry_config::{RegistryConfig, RegistryCredential};
use specforge_registry_client::registry_ops::PublishError;
use specforge_registry_client::testing::{
    MemoryClient, assert_client_contract, assert_private_client_contract,
};
use specforge_registry_client::{
    HttpRegistryClient, RegistryClient, RegistryError, SigningKey, publish_to_registry,
};
use specforge_registry_server::testing::LocalRegistry;
use specforge_registry_wire::SearchQuery;

#[specforge_test_macros::test(
    port = "RegistryClient",
    verify = "RegistryClient contract is satisfied"
)]
fn the_http_client_keeps_the_client_contract() {
    let server = LocalRegistry::start();
    let registry = RegistryConfig {
        alias: "local".to_string(),
        url: server.url().to_string(),
        scope_filter: None,
        default_registry: true,
    };
    let credential = RegistryCredential::new("local", server.token());
    assert_client_contract(&HttpRegistryClient::new(), &registry, &credential);
}

#[specforge_test_macros::test(
    port = "RegistryClient",
    verify = "RegistryClient contract is satisfied"
)]
fn the_memory_client_keeps_the_client_contract() {
    let registry = RegistryConfig {
        alias: "memory".to_string(),
        url: "memory://registry/v1".to_string(),
        scope_filter: None,
        default_registry: true,
    };
    let credential = RegistryCredential::new("memory", "contract-token");
    assert_client_contract(
        &MemoryClient::new().accepting("contract-token"),
        &registry,
        &credential,
    );
}

#[specforge_test_macros::test(
    behavior = "support_private_registries",
    verify = "a private registry refuses an anonymous read and answers an authenticated one"
)]
fn the_private_contract_holds_over_http() {
    let server = LocalRegistry::start_private();
    let registry = RegistryConfig {
        alias: "local".to_string(),
        url: server.url().to_string(),
        scope_filter: None,
        default_registry: true,
    };
    let credential = RegistryCredential::new("local", server.token());
    assert_private_client_contract(&HttpRegistryClient::new(), &registry, &credential);
}

#[specforge_test_macros::test(
    behavior = "support_private_registries",
    verify = "a private registry refuses an anonymous read and answers an authenticated one"
)]
fn the_private_contract_holds_in_memory() {
    let registry = RegistryConfig {
        alias: "memory".to_string(),
        url: "memory://registry/v1".to_string(),
        scope_filter: None,
        default_registry: true,
    };
    let credential = RegistryCredential::new("memory", "contract-token");
    let client = MemoryClient::new()
        .accepting("contract-token")
        .private(&registry);
    assert_private_client_contract(&client, &registry, &credential);
}

/// The registry's root: what `LocalRegistry::url` names without the protocol's version prefix.
fn root_of(server: &LocalRegistry) -> String {
    server
        .url()
        .strip_suffix("/v1")
        .expect("the local registry's URL ends in /v1 today")
        .to_string()
}

#[specforge_test_macros::test(
    port = "RegistryClient",
    verify = "RegistryClient contract is satisfied"
)]
#[ignore = "fixed by T3"]
fn the_http_client_keeps_the_contract_at_a_registry_root() {
    let server = LocalRegistry::start();
    let registry = RegistryConfig {
        alias: "local".to_string(),
        url: root_of(&server),
        scope_filter: None,
        default_registry: true,
    };
    let credential = RegistryCredential::new("local", server.token());
    assert_client_contract(&HttpRegistryClient::new(), &registry, &credential);
}

#[specforge_test_macros::test(
    behavior = "check_registry_reply",
    verify = "a reply that is not the protocol's JSON is refused with R009, even with --allow-unsigned"
)]
#[ignore = "fixed by T4"]
fn an_address_no_registry_serves_is_not_a_registry() {
    let server = LocalRegistry::start();
    let root = format!("{}/elsewhere", root_of(&server));
    let registry = RegistryConfig {
        alias: "local".to_string(),
        url: root.clone(),
        scope_filter: None,
        default_registry: true,
    };
    let credential = RegistryCredential::new("local", server.token());
    let client = HttpRegistryClient::new();
    let tool = PackageName::parse("@contract/tool").unwrap();
    let version = Version::new(1, 0, 0);

    let declaration = serde_json::from_value(serde_json::json!({
        "handshake": {
            "protocol_version": "1.0.0",
            "name": "@contract/tool",
            "version": "1.0.0",
            "description": "Contract tool",
            "keywords": ["contract"],
            "peer_dependencies": [],
            "sandbox_policy": null
        }
    }))
    .unwrap();
    let published = publish_to_registry(
        b"\0asm contract tool",
        &declaration,
        &registry,
        Some(&credential),
        &client,
        &SigningKey::generate(),
    );
    let publish_error = match published {
        Err(PublishError::Registry(error)) => error,
        other => panic!("publish at a non-registry: {other:?}"),
    };

    let errors: Vec<(&str, RegistryError)> = vec![
        (
            "versions",
            client.versions(&tool, &registry, None).unwrap_err(),
        ),
        (
            "metadata",
            client
                .metadata(&tool, &version, &registry, None)
                .unwrap_err(),
        ),
        (
            "search",
            client
                .search(&SearchQuery::new("tool"), &registry, None)
                .unwrap_err(),
        ),
        (
            "authenticate",
            client.authenticate(&registry, &credential).unwrap_err(),
        ),
        ("publish", publish_error),
        (
            "download",
            client
                .download(&format!("{root}/x"), &registry, None)
                .unwrap_err(),
        ),
    ];
    for (call, error) in errors {
        assert!(
            matches!(error, RegistryError::InvalidReply { .. }),
            "{call} at an address no registry serves is InvalidReply, not {error:?}"
        );
        assert_eq!(error.to_diagnostic().code, "R009", "{call}");
    }
}
