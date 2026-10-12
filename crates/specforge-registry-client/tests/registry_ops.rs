use specforge_common::Severity;
use specforge_package::PackageName;
use specforge_package::Version;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry_client::registry_ops::{publish_to_registry, verify_registry_integrity};
use specforge_registry_client::testing::{Call, CallKind, MemoryClient, package};
use specforge_registry_client::{
    RegistryClient, RegistryConfig, RegistryCredential, RegistryError, SigningKey,
};
use specforge_registry_wire::{PackageMetadata, PackageSignature, SignedContent, path};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_registry() -> RegistryConfig {
    RegistryConfig {
        alias: "default".to_string(),
        url: "https://registry.specforge.dev".to_string(),
        scope_filter: None,
        default_registry: true,
    }
}

fn minimal_manifest() -> ExtensionDeclaration {
    serde_json::from_value(serde_json::json!({
        "handshake": {
            "protocol_version": "1.0.0",
            "name": "@test/ext",
            "version": "1.0.0",
            "peer_dependencies": [],
            "sandbox_policy": null
        }
    }))
    .unwrap()
}

/// A token the registry accepts, and the credential that sends it.
const TOKEN: &str = "publisher-token";

fn accepted() -> RegistryCredential {
    RegistryCredential::new("default", TOKEN)
}

/// The publisher's signing key.
fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

fn client() -> MemoryClient {
    MemoryClient::new().accepting(TOKEN)
}

/// `registry` holds `name@version`.
fn store(client: &MemoryClient, registry: &RegistryConfig, name: &str, version: &str, desc: &str) {
    client.store(
        registry,
        PackageMetadata {
            name: name.to_string(),
            version: version.to_string(),
            description: desc.to_string(),
            ..Default::default()
        },
        Vec::new(),
    );
}

fn publishes(client: &MemoryClient) -> Vec<Call> {
    client
        .calls()
        .into_iter()
        .filter(|call| call.kind == CallKind::Publish)
        .collect()
}

// ---------------------------------------------------------------------------
// Tests: publish_to_registry
// ---------------------------------------------------------------------------

// B:publish_to_registry — verify unit "computes SHA256, includes in upload"
#[test]
fn publish_computes_sha256() {
    let registry = default_registry();
    let manifest = minimal_manifest();
    let package_bytes = b"fake-wasm-bytes";
    let client = client();

    let url = publish_to_registry(
        package_bytes,
        &manifest,
        &registry,
        Some(&accepted()),
        &client,
        &key(),
    )
    .unwrap();
    assert!(url.contains("@test"));

    let stored = client
        .metadata(
            &PackageName::parse("@test/ext").unwrap(),
            &Version::new(1, 0, 0),
            &registry,
            None,
        )
        .unwrap();
    assert_eq!(
        stored.sha256,
        package("@test/ext", "1.0.0", package_bytes, "", None).sha256
    );
    assert_eq!(stored.size_bytes, package_bytes.len() as u64);
}

// B:publish_to_registry — verify unit "a version already published is refused with R007"
#[test]
fn a_version_the_registry_holds_is_refused_with_r007() {
    let registry = default_registry();
    let manifest = minimal_manifest();
    let client = client();
    // the version is already there
    store(&client, &registry, "@test/ext", "1.0.0", "");

    let err = publish_to_registry(
        b"fake-wasm-bytes",
        &manifest,
        &registry,
        Some(&accepted()),
        &client,
        &key(),
    )
    .unwrap_err();
    assert_eq!(err.severity, Severity::Error);
    assert_eq!(err.code, "R007");
    assert!(
        client
            .calls()
            .iter()
            .all(|call| call.kind != CallKind::Metadata),
        "publish asked nothing before it uploaded"
    );
}

// B:publish_to_registry — verify unit "successful publish returns registry URL"
#[test]
fn publish_returns_registry_url_on_success() {
    let registry = default_registry();
    let manifest = minimal_manifest();
    let client = client();

    let expected = format!(
        "{}{}",
        registry.url,
        path::version(
            &PackageName::parse("@test/ext").unwrap(),
            &Version::new(1, 0, 0)
        )
    );
    let url = publish_to_registry(
        b"fake-wasm-bytes",
        &manifest,
        &registry,
        Some(&accepted()),
        &client,
        &key(),
    )
    .unwrap();
    assert_eq!(url, expected);
}

// B:publish_to_registry — verify unit "threads credential into client.publish"
#[test]
fn publish_threads_credential_to_client() {
    let registry = default_registry();
    let manifest = minimal_manifest();
    let credential = accepted();

    let client = client();
    publish_to_registry(
        b"fake-wasm-bytes",
        &manifest,
        &registry,
        Some(&credential),
        &client,
        &key(),
    )
    .unwrap();
    assert_eq!(
        publishes(&client)
            .into_iter()
            .map(|call| call.credential)
            .collect::<Vec<_>>(),
        vec![Some(credential.clone())]
    );

    // An anonymous publish carries no credential, and the registry refuses it.
    let anonymous = self::client();
    let err = publish_to_registry(
        b"fake-wasm-bytes",
        &manifest,
        &registry,
        None,
        &anonymous,
        &key(),
    )
    .unwrap_err();
    assert_eq!(err.code, "R001");
    assert_eq!(
        publishes(&anonymous)
            .into_iter()
            .map(|call| call.credential)
            .collect::<Vec<_>>(),
        vec![None]
    );
}

// ---------------------------------------------------------------------------
// Tests: verify_registry_integrity
// ---------------------------------------------------------------------------

// B:verify_registry_integrity — verify unit "matching SHA256 passes"
#[test]
fn verify_integrity_matching_sha256_passes() {
    let data = b"hello world";
    // Pre-computed SHA256 of "hello world"
    let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

    let result = verify_registry_integrity(data, expected);
    assert!(result.is_ok());
}

// B:verify_registry_integrity — verify unit "mismatched SHA256 → hard error"
#[test]
fn verify_integrity_mismatched_sha256_is_error() {
    let data = b"hello world";
    let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

    let err = verify_registry_integrity(data, wrong_hash).unwrap_err();
    assert_eq!(err.severity, Severity::Error);
    assert_eq!(err.code, "R-OPS-002");
    assert!(err.message.contains("integrity check failed"));
    assert!(err.message.contains(wrong_hash));
}

// B:support_private_registries — verify unit "error messages don't leak auth details"
#[test]
fn error_messages_do_not_leak_auth_details() {
    let raw_token = "ghp_super_secret_token_12345";

    // Network error with a message that could contain a token — verify the RegistryError
    // variants never include raw tokens in their diagnostic output
    let errors = vec![
        RegistryError::Unauthorized {
            guidance: "invalid credentials".into(),
        },
        RegistryError::Forbidden {
            guidance: "access denied".into(),
        },
        RegistryError::NetworkError {
            message: "connection refused".into(),
        },
    ];

    for err in errors {
        let diag = err.to_diagnostic();
        assert!(
            !diag.message.contains(raw_token),
            "diagnostic message must not contain raw token"
        );
        if let Some(ref suggestion) = diag.suggestion {
            assert!(
                !suggestion.contains(raw_token),
                "diagnostic suggestion must not contain raw token"
            );
        }
    }
}

// B:publish_to_registry — verify unit "signed publish carries verifiable signature"
#[test]
fn publish_signs_package_when_key_provided() {
    let registry = default_registry();
    let manifest = minimal_manifest();
    let key = SigningKey::generate();
    let client = client();

    publish_to_registry(
        b"wasm-bytes",
        &manifest,
        &registry,
        Some(&accepted()),
        &client,
        &key,
    )
    .unwrap();

    let calls = publishes(&client);
    assert_eq!(calls.len(), 1);
    let sig: &PackageSignature = calls[0].signature.as_ref().expect("signature present");
    let manifest_json = calls[0].manifest.as_deref().expect("manifest sent");
    sig.verify(&SignedContent::of(
        "@test/ext",
        "1.0.0",
        b"wasm-bytes",
        manifest_json,
    ))
    .expect("the signature verifies over what was uploaded");
    assert_eq!(sig.key_id(), key.key_id());
}

#[test]
fn the_memory_client_refuses_a_signature_that_does_not_verify() {
    let registry = default_registry();
    let declaration = minimal_manifest();
    let manifest = serde_json::to_string(&declaration).unwrap();
    let signature = key().sign(&SignedContent::of(
        "@test/ext",
        "1.0.0",
        b"\0asm other",
        &manifest,
    ));
    let client = client();

    let error = client
        .publish(
            b"\0asm wasm",
            &declaration,
            &manifest,
            &signature,
            &registry,
            Some(&accepted()),
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            RegistryError::Refused {
                code: specforge_registry_wire::ErrorCode::InvalidSignature,
                ..
            }
        ),
        "{error:?}"
    );
    let error = client
        .metadata(
            &PackageName::parse("@test/ext").unwrap(),
            &Version::new(1, 0, 0),
            &registry,
            None,
        )
        .unwrap_err();
    assert!(matches!(error, RegistryError::NotFound { .. }), "{error:?}");
}
