#![allow(clippy::result_large_err)]

use specforge_common::{Diagnostic, codes};

use super::registry_client::{RegistryClient, RegistryError};
use super::registry_config::{RegistryConfig, RegistryCredential};
use specforge_package::{PackageName, Version};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry_wire::{PackageMetadata, PublishReceipt, SignedContent, path, sha256_hex};

/// Why a publish did not reach the registry, or what the registry answered.
#[derive(Debug, Clone, PartialEq)]
pub enum PublishError {
    /// The declaration is no package (E072) or cannot be serialized (R-OPS-003).
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
    package: &[u8],
    declaration: &ExtensionDeclaration,
    registry: &RegistryConfig,
    credential: Option<&RegistryCredential>,
    client: &dyn RegistryClient,
    key: &crate::SigningKey,
) -> Result<String, PublishError> {
    // What is published is a package: a name and a version (ADR 0036).
    let invalid = |message: String| {
        PublishError::Invalid(RegistryError::InvalidPackage { message }.to_diagnostic())
    };
    let name = PackageName::declared_by(declaration).map_err(|why| invalid(why.to_string()))?;
    let version = Version::parse(declaration.version()).map_err(|why| {
        invalid(format!(
            "'{}' is not a SemVer version: {why}",
            declaration.version()
        ))
    })?;

    let manifest_json = serde_json::to_string(declaration).map_err(|e| {
        PublishError::Invalid(Diagnostic::new(
            codes::R_OPS_003,
            format!("failed to serialize the declaration: {}", e),
        ))
    })?;

    // The signature binds the exact uploaded manifest bytes and the wasm hash to the publisher key.
    let content = SignedContent::of(
        declaration.name(),
        declaration.version(),
        package,
        &manifest_json,
    );
    let signature = key.sign(&content);

    let receipt = client
        .publish(
            package,
            declaration,
            &manifest_json,
            &signature,
            registry,
            credential,
        )
        .map_err(PublishError::Registry)?;
    let expected = PublishReceipt {
        name: declaration.name().to_string(),
        version: declaration.version().to_string(),
        sha256: content.wasm_sha256().to_string(),
        size_bytes: package.len() as u64,
        key_id: signature.key_id(),
    };
    if receipt != expected {
        return Err(PublishError::Registry(RegistryError::InvalidReply {
            subject: format!("{name}@{version}"),
            message: format!("its receipt describes another upload than the one sent: {receipt:?}"),
        }));
    }
    Ok(format!(
        "{}{}",
        registry.url.trim_end_matches('/'),
        path::version(&name, &version)
    ))
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

/// Verify a downloaded package's publisher signature against served metadata.
///
/// Uses only data the client holds or the registry serves: the downloaded wasm
/// bytes, the served manifest JSON, and the typed signature — the registry is
/// not the trust anchor (spec #21). Fails (R-TRUST-002) when the signature
/// doesn't verify: tampered wasm, tampered manifest or a wrong key. A signature
/// that isn't one never gets here: the reply isn't readable (R009).
pub fn verify_package_signature(
    response: &PackageMetadata,
    wasm_bytes: &[u8],
) -> Result<TrustCheck, Diagnostic> {
    let Some(signature) = &response.signature else {
        return Ok(TrustCheck::Unsigned);
    };
    let content = SignedContent::of(
        &response.name,
        &response.version,
        wasm_bytes,
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
