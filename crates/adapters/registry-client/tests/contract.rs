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
use specforge_registry_wire::{ErrorCode, RegistryRoot, SearchQuery, SignedContent};

#[specforge_test_macros::test(
    port = "RegistryClient",
    verify = "RegistryClient contract is satisfied"
)]
fn the_http_client_keeps_the_client_contract() {
    let server = LocalRegistry::start();
    let registry = RegistryConfig {
        alias: "local".to_string(),
        url: server.root().clone(),
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
        url: RegistryRoot::parse("https://memory.invalid").unwrap(),
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

fn declaration_at(version: &str) -> specforge_protocol_types::ExtensionDeclaration {
    serde_json::from_value(serde_json::json!({
        "handshake": {
            "protocol_version": "1.0.0",
            "name": "@contract/tool",
            "version": version,
            "description": "Contract tool",
            "keywords": ["contract"],
            "peer_dependencies": [],
            "sandbox_policy": null
        }
    }))
    .unwrap()
}

fn refuses_a_manifest_naming_another_version(
    client: &dyn RegistryClient,
    registry: &RegistryConfig,
    credential: &RegistryCredential,
) {
    let routed = declaration_at("4.0.0");
    let manifest = serde_json::to_string(&declaration_at("3.0.0")).unwrap();
    let wasm = b"\0asm four";
    let signature = SigningKey::generate().sign(&SignedContent::of(
        "@contract/tool",
        "4.0.0",
        wasm,
        &manifest,
    ));
    let error = client
        .publish(
            wasm,
            &routed,
            &manifest,
            &signature,
            registry,
            Some(credential),
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            RegistryError::Refused {
                code: ErrorCode::NameMismatch,
                ..
            }
        ),
        "a manifest naming another version is Refused(NameMismatch), not {error:?}"
    );
    let tool = PackageName::parse("@contract/tool").unwrap();
    let stored = client.metadata(&tool, &Version::new(4, 0, 0), registry, None);
    assert!(
        matches!(stored, Err(RegistryError::NotFound { .. })),
        "{stored:?}"
    );
}

#[test]
fn over_http_a_manifest_naming_another_version_is_refused() {
    let server = LocalRegistry::start();
    let registry = RegistryConfig {
        alias: "local".to_string(),
        url: server.root().clone(),
        scope_filter: None,
        default_registry: true,
    };
    let credential = RegistryCredential::new("local", server.token());
    refuses_a_manifest_naming_another_version(&HttpRegistryClient::new(), &registry, &credential);
}

#[test]
#[ignore = "flipped by the next commit"]
fn in_memory_a_manifest_naming_another_version_is_refused() {
    let registry = RegistryConfig {
        alias: "memory".to_string(),
        url: RegistryRoot::parse("https://memory.invalid").unwrap(),
        scope_filter: None,
        default_registry: true,
    };
    let credential = RegistryCredential::new("memory", "contract-token");
    refuses_a_manifest_naming_another_version(
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
        url: server.root().clone(),
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
        url: RegistryRoot::parse("https://memory.invalid").unwrap(),
        scope_filter: None,
        default_registry: true,
    };
    let credential = RegistryCredential::new("memory", "contract-token");
    let client = MemoryClient::new()
        .accepting("contract-token")
        .private(&registry);
    assert_private_client_contract(&client, &registry, &credential);
}

#[specforge_test_macros::test(
    behavior = "check_registry_reply",
    verify = "a reply that is not the protocol's JSON is refused with R009, even with --allow-unsigned"
)]
fn an_address_no_registry_serves_is_not_a_registry() {
    let server = LocalRegistry::start();
    let root = format!("{}/elsewhere", server.root());
    let registry = RegistryConfig {
        alias: "local".to_string(),
        url: RegistryRoot::parse(&root).unwrap(),
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
