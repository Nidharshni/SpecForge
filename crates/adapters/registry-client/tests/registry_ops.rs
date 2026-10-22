use specforge_common::Severity;
use specforge_package::{DeclaredPackage, PackageName, Version};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry_client::registry_ops::{publish_to_registry, verify_registry_integrity};
use specforge_registry_client::testing::{Call, CallKind, MemoryClient, package};
use specforge_registry_client::{
    RegistryClient, RegistryConfig, RegistryCredential, RegistryError, SigningKey,
};
use specforge_registry_wire::{
    PackageMetadata, PackageSignature, RegistryRoot, SignedContent, address::Call as WireCall,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_registry() -> RegistryConfig {
    RegistryConfig {
        alias: "default".to_string(),
        url: RegistryRoot::parse("https://registry.specforge.dev").unwrap(),
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
    RegistryCredential::new(default_registry().url, TOKEN)
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
        &DeclaredPackage::of(&manifest).unwrap(),
        package_bytes,
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
        &DeclaredPackage::of(&manifest).unwrap(),
        b"fake-wasm-bytes",
        &registry,
        Some(&accepted()),
        &client,
        &key(),
    )
    .unwrap_err()
    .to_diagnostic();
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

    let expected = registry.url.address(&WireCall::Version(
        &PackageName::parse("@test/ext").unwrap(),
        &Version::new(1, 0, 0),
    ));
    let url = publish_to_registry(
        &DeclaredPackage::of(&manifest).unwrap(),
        b"fake-wasm-bytes",
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
        &DeclaredPackage::of(&manifest).unwrap(),
        b"fake-wasm-bytes",
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
        &DeclaredPackage::of(&manifest).unwrap(),
        b"fake-wasm-bytes",
        &registry,
        None,
        &anonymous,
        &key(),
    )
    .unwrap_err()
    .to_diagnostic();
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
        &DeclaredPackage::of(&manifest).unwrap(),
        b"wasm-bytes",
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
            &DeclaredPackage::of(&declaration).unwrap(),
            b"\0asm wasm",
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

/// A registry that answers every publish with a receipt for another binary, and everything else as `inner`.
struct WrongReceipt(MemoryClient);

impl RegistryClient for WrongReceipt {
    fn versions(
        &self,
        name: &PackageName,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<String>, RegistryError> {
        self.0.versions(name, registry, credential)
    }

    fn metadata(
        &self,
        name: &PackageName,
        version: &Version,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<PackageMetadata, RegistryError> {
        self.0.metadata(name, version, registry, credential)
    }

    fn download(
        &self,
        wasm_url: &str,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<u8>, RegistryError> {
        self.0.download(wasm_url, registry, credential)
    }

    fn search(
        &self,
        query: &specforge_registry_wire::SearchQuery,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<specforge_registry_wire::SearchHit>, RegistryError> {
        self.0.search(query, registry, credential)
    }

    fn publish(
        &self,
        package: &DeclaredPackage<'_>,
        wasm: &[u8],
        manifest_json: &str,
        signature: &PackageSignature,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<specforge_registry_wire::PublishReceipt, RegistryError> {
        let mut receipt = self.0.publish(
            package,
            wasm,
            manifest_json,
            signature,
            registry,
            credential,
        )?;
        receipt.sha256 = "0".repeat(64);
        Ok(receipt)
    }

    fn authenticate(
        &self,
        registry: &RegistryConfig,
        credential: &RegistryCredential,
    ) -> Result<Option<String>, RegistryError> {
        self.0.authenticate(registry, credential)
    }
}

#[specforge_test_macros::test(
    behavior = "publish_to_registry",
    verify = "publish checks the registry's receipt against what it uploaded"
)]
fn a_receipt_for_another_upload_is_refused() {
    let error = publish_to_registry(
        &DeclaredPackage::of(&minimal_manifest()).unwrap(),
        b"fake-wasm-bytes",
        &default_registry(),
        Some(&accepted()),
        &WrongReceipt(client()),
        &key(),
    )
    .unwrap_err()
    .to_diagnostic();
    assert_eq!(error.code, "R009", "{error:?}");
    assert!(error.message.contains("receipt"), "{error:?}");
}

#[specforge_test_macros::test(
    behavior = "verify_publisher_signature",
    verify = "a signature is verified over the digests the reply describes, before any download"
)]
fn a_signature_is_verified_over_the_digests_the_reply_describes() {
    use specforge_registry_client::{TrustCheck, verify_package_signature};
    let wasm = b"fake-wasm-bytes";
    let reply = package("@test/ext", "1.0.0", wasm, "{}", Some(&key()));

    // No bytes at hand: the reply's own SHA-256 is what the signature covers.
    assert_eq!(
        verify_package_signature(&reply).unwrap(),
        TrustCheck::Verified {
            key_id: key().key_id()
        }
    );

    let other = PackageMetadata {
        sha256: specforge_registry_wire::sha256_hex(b"other"),
        ..reply
    };
    assert_eq!(
        verify_package_signature(&other).unwrap_err().code,
        "R-TRUST-002"
    );
}
