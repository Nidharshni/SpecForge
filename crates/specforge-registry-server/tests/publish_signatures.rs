//! Signed-publish round trip through the real HTTP boundary (spec #21, T1).
//!
//! Spawns the actual axum router on an ephemeral port, publishes a signed
//! package through `HttpRegistryClient` + `publish_to_registry` (the same
//! orchestration the CLI runs), then asserts the signature survives storage,
//! is served in metadata, and verifies offline — the registry is not needed
//! as a trust anchor.

use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry_client::registry_config::{RegistryConfig, RegistryCredential};
use specforge_registry_client::{
    HttpRegistryClient, PackageSignature, SigningKey, publish_to_registry, verify_signature,
};
use specforge_registry_server::state::PublishLimits;
use specforge_registry_server::testing::LocalRegistry;

fn minimal_manifest() -> ExtensionDeclaration {
    manifest_of("1.0.0")
}

fn manifest_of(version: &str) -> ExtensionDeclaration {
    serde_json::from_value(serde_json::json!({
        "handshake": {
            "protocol_version": "1.0.0",
            "name": "@test/signed-ext",
            "version": version,
            "peer_dependencies": [],
            "sandbox_policy": null
        }
    }))
    .unwrap()
}

/// How the CLI names `server` in `specforge.json`, and the credential a publish sends.
fn client_of(server: &LocalRegistry) -> (RegistryConfig, RegistryCredential) {
    (
        RegistryConfig {
            alias: "test".to_string(),
            url: server.url().to_string(),
            scope_filter: None,
            default_registry: true,
        },
        RegistryCredential::new("test", server.token()),
    )
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[test]
fn signed_publish_round_trips_through_http_boundary() {
    let server = LocalRegistry::start();
    let (registry, credential) = client_of(&server);

    let manifest = minimal_manifest();
    let wasm_bytes: &[u8] = b"\0asm-fake-extension-bytes";
    let key = SigningKey::generate();

    // Publish exactly as the CLI orchestrates it.
    let url = publish_to_registry(
        wasm_bytes,
        &manifest,
        &registry,
        Some(&credential),
        &HttpRegistryClient::new(),
        Some(&key),
    )
    .expect("signed publish should succeed");
    assert!(url.contains("signed-ext"), "{}", url);

    // Metadata serves the signature object, the key id, and the manifest.
    let metadata_url = format!("{}/packages/%40test%2Fsigned-ext/1.0.0", server.url());
    let body: serde_json::Value = reqwest::blocking::get(&metadata_url)
        .expect("metadata request")
        .json()
        .expect("metadata json");
    assert_eq!(body["key_id"], key.key_id().as_str());
    let signature: PackageSignature = serde_json::from_str(
        body["signature"]
            .as_str()
            .expect("signature served as string"),
    )
    .expect("signature wire object");
    assert_eq!(signature.key_id, key.key_id());
    assert_eq!(signature.public_key, key.public_key_hex());
    assert!(!signature.signed_at.is_empty());
    let served_manifest = body["manifest"].as_str().expect("manifest served");
    assert_eq!(served_manifest, serde_json::to_string(&manifest).unwrap());

    // Offline verification from served data alone: wasm bytes (as downloaded)
    // + served manifest + timestamp inside the signature object. The registry
    // is not the trust anchor.
    verify_signature(
        manifest.name(),
        manifest.version(),
        &sha256_hex(wasm_bytes),
        &sha256_hex(served_manifest.as_bytes()),
        &signature,
    )
    .expect("served signature must verify offline against served metadata");
}

#[specforge_test_macros::test(
    behavior = "retry_registry_request",
    verify = "a rate-limited answer is R003 on every registry call"
)]
fn a_rate_limited_publish_is_r003() {
    let server = LocalRegistry::start_with(PublishLimits {
        per_token: 1,
        per_ip: 100,
        window: std::time::Duration::from_secs(60),
    });
    let (registry, credential) = client_of(&server);
    let key = SigningKey::generate();
    let client = HttpRegistryClient::new();
    let publish = |version: &str| {
        publish_to_registry(
            b"\0asm-fake-extension-bytes",
            &manifest_of(version),
            &registry,
            Some(&credential),
            &client,
            Some(&key),
        )
    };

    publish("1.0.0").expect("the first publish is inside the window");
    let diagnostic = publish("1.0.1").expect_err("the second is rate limited");
    assert_eq!(diagnostic.code, "R003", "{diagnostic:?}");
    assert!(
        diagnostic.message.contains("rate limited"),
        "{diagnostic:?}"
    );
}

/// A signature text no client accepts: the round-3 envelope with values that are not hex.
const MALFORMED: &str = r#"{"sig":"x","keyId":"y","pubkey":"z","signedAt":"now"}"#;

// bug: plan 12 §3 R1, flipped by T4
#[test]
fn a_signature_the_client_refuses_is_stored_and_burns_the_version_today() {
    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
    use specforge_package::{PackageName, Version};
    use specforge_registry_client::RegistryClient;
    use specforge_registry_wire::form;

    let server = LocalRegistry::start();
    let (registry, credential) = client_of(&server);
    let declaration = minimal_manifest();
    let manifest = serde_json::to_string(&declaration).unwrap();
    let wasm: &[u8] = b"\0asm-fake-extension-bytes";

    // 1. What no client sends is stored.
    let response = reqwest::blocking::Client::new()
        .put(format!(
            "{}/packages/%40test%2Fsigned-ext/1.0.0",
            server.url()
        ))
        .header(AUTHORIZATION, format!("Bearer {}", server.token()))
        .header(CONTENT_TYPE, form::content_type("p12"))
        .body(form::body("p12", &manifest, wasm, Some(MALFORMED)))
        .send()
        .expect("the local registry answers");
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);

    // 2. Every client refuses it, even with --allow-unsigned.
    let client = HttpRegistryClient::new();
    let name = PackageName::parse("@test/signed-ext").unwrap();
    let version = Version::parse("1.0.0").unwrap();
    let metadata = client.metadata(&name, &version, &registry, None).unwrap();
    let bytes = client
        .download(&metadata.wasm_url, &registry, None)
        .unwrap();
    let home = tempfile::tempdir().unwrap();
    let error = specforge_registry_client::trust_flow::check_and_pin(
        "@test/signed-ext",
        &metadata,
        &bytes,
        true,
        &|_| true,
        &home.path().join("known-keys.json"),
    )
    .unwrap_err();
    assert_eq!(error.code, "R-TRUST-002", "{error:?}");

    // 3. The version is burned: a correctly signed publish of it is refused.
    let error = publish_to_registry(
        wasm,
        &declaration,
        &registry,
        Some(&credential),
        &client,
        Some(&SigningKey::generate()),
    )
    .unwrap_err();
    assert_eq!(error.code, "R007", "{error:?}");
}

#[test]
fn tampered_wasm_fails_offline_verification() {
    // A signature over one wasm hash must not verify against different bytes.
    let key = SigningKey::generate();
    let manifest = minimal_manifest();
    let declaration_json = serde_json::to_string(&manifest).unwrap();
    let wasm = b"\0asm-real-bytes";
    let signature = key.sign_package(
        manifest.name(),
        manifest.version(),
        &sha256_hex(wasm),
        &sha256_hex(declaration_json.as_bytes()),
        "2026-09-24T00:00:00+00:00",
    );

    let tampered = b"\0asm-swapped-bytes";
    let err = verify_signature(
        manifest.name(),
        manifest.version(),
        &sha256_hex(tampered),
        &sha256_hex(declaration_json.as_bytes()),
        &signature,
    )
    .unwrap_err();
    assert!(err.contains("verification failed"), "{}", err);
}

#[test]
fn tampered_manifest_fails_offline_verification() {
    // Swapping the manifest (e.g. to relax the sandbox policy) breaks the
    // signature even when the wasm bytes are untouched.
    let key = SigningKey::generate();
    let manifest = minimal_manifest();
    let declaration_json = serde_json::to_string(&manifest).unwrap();
    let wasm = b"\0asm-bytes";
    let signature = key.sign_package(
        manifest.name(),
        manifest.version(),
        &sha256_hex(wasm),
        &sha256_hex(declaration_json.as_bytes()),
        "2026-09-24T00:00:00+00:00",
    );

    let mut swapped: ExtensionDeclaration = minimal_manifest();
    swapped.handshake.version = "9.9.9".to_string();
    let swapped_json = serde_json::to_string(&swapped).unwrap();
    let err = verify_signature(
        manifest.name(),
        manifest.version(),
        &sha256_hex(wasm),
        &sha256_hex(swapped_json.as_bytes()),
        &signature,
    )
    .unwrap_err();
    assert!(err.contains("verification failed"), "{}", err);
}
