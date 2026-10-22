use specforge_common::{Diagnostic, codes};

use super::registry_client::{RegistryClient, RegistryError};
use super::registry_config::{RegistryConfig, RegistryCredential};
use specforge_package::DeclaredPackage;
use specforge_registry_wire::{
    PackageMetadata, PublishReceipt, SignedContent, address::Call, sha256_hex,
};

/// Why a publish did not reach the registry, or what the registry answered.
#[derive(Debug, Clone, PartialEq)]
pub enum PublishError {
    /// The declaration cannot be serialized (R-OPS-003).
    Invalid(Diagnostic),
    /// The registry's answer.
    Registry(RegistryError),
}

impl PublishError {
    /// The failure as a diagnostic.
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            Self::Invalid(diagnostic) => diagnostic.clone(),
            Self::Registry(error) => error.to_diagnostic(),
        }
    }
}

/// Publish to a registry, signed with `key`.
///
/// The package's manifest is its declaration (ADR 0012), serialized once —
/// the uploaded bytes, the manifest digest the signature covers, and the
/// stored manifest are byte-identical. The upload carries `key`'s
/// [`PackageSignature`] over its [`SignedContent`]. `credential`, when
/// provided, authenticates the upload. A version the registry already holds
/// is refused by the registry (R007): a published version is immutable.
/// Returns the registry URL on success.
pub fn publish_to_registry(
    package: &DeclaredPackage<'_>,
    wasm: &[u8],
    registry: &RegistryConfig,
    credential: Option<&RegistryCredential>,
    client: &dyn RegistryClient,
    key: &crate::SigningKey,
) -> Result<String, PublishError> {
    let (name, version) = (package.name(), package.version().to_string());
    let manifest_json = serde_json::to_string(package.declaration()).map_err(|e| {
        PublishError::Invalid(Diagnostic::new(
            codes::R_OPS_003,
            format!("failed to serialize the declaration: {}", e),
        ))
    })?;

    // The signature binds the exact uploaded manifest bytes and the wasm hash to the publisher key.
    let content = SignedContent::of(name.as_str(), &version, wasm, &manifest_json);
    let signature = key.sign(&content);

    let receipt = client
        .publish(
            package,
            wasm,
            &manifest_json,
            &signature,
            registry,
            credential,
        )
        .map_err(PublishError::Registry)?;
    let expected = PublishReceipt {
        name: name.to_string(),
        version: version.clone(),
        sha256: content.wasm_sha256().to_string(),
        size_bytes: wasm.len() as u64,
        key_id: signature.key_id(),
    };
    if receipt != expected {
        return Err(PublishError::Registry(RegistryError::InvalidReply {
            subject: format!("{name}@{version}"),
            message: format!("its receipt describes another upload than the one sent: {receipt:?}"),
        }));
    }
    Ok(registry
        .url
        .address(&Call::Version(package.name(), package.version())))
}

/// Verify SHA256 integrity of downloaded bytes against an expected hash.
pub fn verify_registry_integrity(data: &[u8], expected_sha256: &str) -> Result<(), Diagnostic> {
    let actual = sha256_hex(data);
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

/// Verify a package's publisher signature over what the reply describes.
///
/// Uses only data the registry serves: the binary's SHA-256, the manifest JSON and the typed
/// signature (the registry is not the trust anchor, spec #21). No download is needed: a download is
/// then held to `response.sha256` ([`verify_registry_integrity`]), so the signature covers its bytes.
/// Fails (R-TRUST-002) when the signature doesn't verify: other digests, a swapped manifest or a
/// wrong key. A signature that isn't one never gets here: the reply isn't readable (R009).
pub fn verify_package_signature(response: &PackageMetadata) -> Result<TrustCheck, Diagnostic> {
    let Some(signature) = &response.signature else {
        return Ok(TrustCheck::Unsigned);
    };
    let content = SignedContent::described(
        &response.name,
        &response.version,
        &response.sha256,
        &response.manifest,
    );
    signature.verify(&content).map_err(|why| {
        Diagnostic::new(
            codes::R_TRUST_002,
            format!(
                "signature verification failed for '{}': {why}",
                response.name
            ),
        )
        .with_suggestion(
            "the package does not match its publisher signature; do not install it".to_string(),
        )
    })?;
    Ok(TrustCheck::Verified {
        key_id: signature.key_id(),
    })
}
