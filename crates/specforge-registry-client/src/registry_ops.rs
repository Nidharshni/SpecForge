#![allow(clippy::result_large_err)]

use sha2::{Digest, Sha256};
use specforge_common::{Diagnostic, codes};

use super::registry_client::{RegistryClient, RegistryError};
use super::registry_config::{RegistryConfig, RegistryCredential};
use specforge_package::{PackageName, Version};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry_wire::PackageMetadata;

/// Compute the hex-encoded SHA256 digest of the given data.
fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Publish to a registry, optionally signing the package.
///
/// The package's manifest is its declaration (ADR 0012), serialized once —
/// the uploaded bytes, the `manifest_sha256` inside the signature payload,
/// and the stored manifest are byte-identical.
/// When `signing` is provided, the upload carries a [`PackageSignature`] over
/// `{name, version, wasm_sha256, manifest_sha256, signed_at}`.
/// `credential`, when provided, authenticates the upload.
/// A version the registry already holds is refused by the registry (R007): a
/// published version is immutable. Returns the registry URL on success.
pub fn publish_to_registry(
    package: &[u8],
    declaration: &ExtensionDeclaration,
    registry: &RegistryConfig,
    credential: Option<&RegistryCredential>,
    client: &dyn RegistryClient,
    signing: Option<&crate::SigningKey>,
) -> Result<String, Diagnostic> {
    // What is published is a package: a name and a version (ADR 0036).
    let invalid = |message: String| RegistryError::InvalidPackage { message }.to_diagnostic();
    PackageName::declared_by(declaration).map_err(|why| invalid(why.to_string()))?;
    Version::parse(declaration.version()).map_err(|why| {
        invalid(format!(
            "'{}' is not a SemVer version: {why}",
            declaration.version()
        ))
    })?;

    let manifest_json = serde_json::to_string(declaration).map_err(|e| {
        Diagnostic::new(
            codes::R_OPS_003,
            format!("failed to serialize the declaration: {}", e),
        )
    })?;

    // Sign when a key is provided: the payload binds the exact uploaded
    // manifest bytes and the wasm hash to the publisher key.
    let signature = signing.map(|key| {
        let signature = key.sign_package(
            declaration.name(),
            declaration.version(),
            &hex_sha256(package),
            &hex_sha256(manifest_json.as_bytes()),
            &chrono::Utc::now().to_rfc3339(),
        );
        serde_json::to_string(&signature).expect("signature serialization cannot fail")
    });

    client
        .publish(
            package,
            declaration,
            &manifest_json,
            signature.as_deref(),
            registry,
            credential,
        )
        .map_err(|e| e.to_diagnostic())
}

/// Verify SHA256 integrity of downloaded bytes against an expected hash.
pub fn verify_registry_integrity(data: &[u8], expected_sha256: &str) -> Result<(), Diagnostic> {
    let actual = hex_sha256(data);
    if actual == expected_sha256 {
        Ok(())
    } else {
        Err(Diagnostic::new(
            codes::R_OPS_002,
            format!("SHA256 integrity check failed. Expected '{expected_sha256}', got '{actual}'."),
        )
        .with_suggestion(
            "The downloaded package may be corrupted or tampered with. Try downloading again."
                .to_string(),
        ))
    }
}

/// Outcome of checking a registry package's publisher signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustCheck {
    /// Signature present and valid; carries the verified key id.
    Verified { key_id: String },
    /// No signature on the package (caller decides whether that is allowed).
    Unsigned,
}

/// Verify a downloaded package's publisher signature against served metadata.
///
/// Uses only data the client holds or the registry serves: the downloaded wasm
/// bytes, the served manifest JSON, and the signature wire object — the
/// registry is not the trust anchor (spec #21). Fails on: tampered wasm,
/// tampered manifest, wrong key, or metadata inconsistency between the
/// server-extracted key id and the signature object.
pub fn verify_package_signature(
    response: &PackageMetadata,
    wasm_bytes: &[u8],
) -> Result<TrustCheck, Diagnostic> {
    if response.signature.is_empty() {
        return Ok(TrustCheck::Unsigned);
    }

    let code = codes::R_TRUST_002;
    let signature: crate::PackageSignature =
        serde_json::from_str(&response.signature).map_err(|e| {
            Diagnostic::new(
                code,
                format!(
                    "unparseable package signature for '{}': {}",
                    response.name, e
                ),
            )
            .with_suggestion("refuse this package; the registry response is malformed".to_string())
        })?;

    // Cross-check the server-extracted key id against the signature object:
    // a mismatch means registry metadata was edited independently of the
    // signature (or is stale).
    if !response.key_id.is_empty() && response.key_id != signature.key_id {
        return Err(Diagnostic::new(
            codes::R_TRUST_004,
            format!(
                "metadata inconsistency for '{}': registry says key '{}' but signature carries '{}'",
                response.name, response.key_id, signature.key_id
            ),
        )
        .with_suggestion("refuse this package and verify the registry".to_string()));
    }

    let manifest_sha256 = hex_sha256(response.manifest.as_bytes());
    let wasm_sha256 = hex_sha256(wasm_bytes);
    crate::verify_signature(
        &response.name,
        &response.version,
        &wasm_sha256,
        &manifest_sha256,
        &signature,
    )
    .map_err(|message| {
        Diagnostic::new(
            code,
            format!(
                "signature verification failed for '{}': {}",
                response.name, message
            ),
        )
        .with_suggestion(
            "the package does not match its publisher signature; do not install it".to_string(),
        )
    })?;

    Ok(TrustCheck::Verified {
        key_id: signature.key_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_sha256_produces_correct_hash() {
        // Known SHA256 of empty byte slice
        let hash = hex_sha256(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hex_sha256_deterministic() {
        let data = b"hello world";
        assert_eq!(hex_sha256(data), hex_sha256(data));
    }
}
