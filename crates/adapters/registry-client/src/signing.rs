//! The publisher's signing key (wayfinder map #8, spec #21 / ticket T1): the user's Ed25519 secret key and the file
//! it lives in. What a signature is, the content it covers, and how it is parsed and verified are the wire
//! contract's (`specforge_registry_wire::signature`, ADR 0061), which the registry runs too.

use serde::{Deserialize, Serialize};
use specforge_registry_wire::{PackageSignature, SignedContent};
use std::path::{Path, PathBuf};

/// A publisher signing key: the user's Ed25519 secret key (`~/.specforge/signing-key.json`).
#[derive(Debug, Clone)]
pub struct SigningKey {
    inner: ed25519_dalek::SigningKey,
}

impl SigningKey {
    /// Generate a fresh random signing key from OS entropy.
    pub fn generate() -> Self {
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).expect("OS entropy unavailable");
        Self::from_bytes(&secret)
    }

    pub fn from_bytes(secret: &[u8; 32]) -> Self {
        Self {
            inner: ed25519_dalek::SigningKey::from_bytes(secret),
        }
    }

    /// [`specforge_registry_wire::signature::key_id`] of its public key.
    pub fn key_id(&self) -> String {
        specforge_registry_wire::signature::key_id(&self.inner.verifying_key())
    }

    /// Its signature over `content`, now.
    pub fn sign(&self, content: &SignedContent) -> PackageSignature {
        self.sign_at(content, chrono::Utc::now())
    }

    /// Its signature over `content` at `at` (a test that needs a fixed time).
    pub fn sign_at(
        &self,
        content: &SignedContent,
        at: chrono::DateTime<chrono::Utc>,
    ) -> PackageSignature {
        PackageSignature::sign(&self.inner, content, at)
    }
}

/// Default on-disk location of the publisher signing key.
pub fn signing_key_path() -> PathBuf {
    crate::credentials::user_dir().join("signing-key.json")
}

/// Load the signing key at `path`, generating and persisting one on first use.
///
/// Returns the key and whether it was newly created.
pub fn load_or_create_signing_key_at(path: &Path) -> Result<(SigningKey, bool), String> {
    if let Ok(content) = std::fs::read_to_string(path) {
        #[derive(Deserialize)]
        struct StoredKey {
            #[serde(rename = "secretKey")]
            secret_key: String,
        }
        let stored: StoredKey = serde_json::from_str(&content)
            .map_err(|e| format!("corrupt signing key file {}: {}", path.display(), e))?;
        let secret: [u8; 32] = hex::decode(stored.secret_key.trim())
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| {
                format!(
                    "corrupt signing key file {}: bad secret encoding",
                    path.display()
                )
            })?;
        return Ok((SigningKey::from_bytes(&secret), false));
    }

    let key = SigningKey::generate();
    #[derive(Serialize)]
    struct StoredKey {
        #[serde(rename = "secretKey")]
        secret_key: String,
        #[serde(rename = "createdAt")]
        created_at: String,
    }
    let stored = StoredKey {
        secret_key: hex::encode(key.inner.to_bytes()),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }
    let json = serde_json::to_string_pretty(&stored)
        .map_err(|e| format!("failed to encode signing key: {e}"))?;
    std::fs::write(path, json)
        .map_err(|e| format!("failed to write signing key {}: {}", path.display(), e))?;
    restrict_permissions(path);
    Ok((key, true))
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn signing_key_persists_and_reloads_identically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("signing-key.json");
        let (key, created) = load_or_create_signing_key_at(&path).unwrap();
        assert!(created);
        let (reloaded, created_again) = load_or_create_signing_key_at(&path).unwrap();
        assert!(!created_again);
        assert_eq!(key.key_id(), reloaded.key_id());
        // The reloaded key signs identically.
        let content = SignedContent::of("@acme/greeter", "1.2.0", b"\0asm", "{}");
        let at = chrono::Utc.with_ymd_and_hms(2026, 9, 24, 0, 0, 0).unwrap();
        assert_eq!(key.sign_at(&content, at), reloaded.sign_at(&content, at));
    }
}
