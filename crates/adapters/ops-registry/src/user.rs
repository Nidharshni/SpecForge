//! The user's registry files: where they are, and what a registry call carries from them (ADR 0178).

use crate::{ConfiguredRegistry, TOKEN_VARIABLE, unusable_credential};
use specforge_common::Diagnostic;
use specforge_ops::registry::{NOT_AUTHENTICATED, UNUSABLE_SIGNING_KEY};
use specforge_ops::{OpError, OpErrorKind};
use specforge_registry_client::credentials::{read_credentials, user_dir};
use specforge_registry_client::signing::load_or_create_signing_key_at;
use specforge_registry_client::{RegistryConfig, RegistryCredential, SigningKey};
use std::path::PathBuf;

/// The user SpecForge runs as, as a registry sees them: the directory that
/// holds their registry files (`credentials.json`, `signing-key.json`,
/// `known-keys.json`) and the token their environment gives.
#[derive(Debug, Clone)]
pub struct User {
    dir: PathBuf,
    token: Option<String>,
}

impl User {
    /// `~/.specforge`, and `SPECFORGE_REGISTRY_TOKEN` when it is set.
    pub fn current() -> Self {
        Self::at(user_dir(), std::env::var(TOKEN_VARIABLE).ok())
    }

    /// Registry files in `dir`, and `token` as the environment's token (a
    /// test, a custom home).
    pub fn at(dir: impl Into<PathBuf>, token: Option<String>) -> Self {
        Self {
            dir: dir.into(),
            token,
        }
    }

    pub(crate) fn credentials(&self) -> PathBuf {
        self.dir.join("credentials.json")
    }

    pub(crate) fn signing_key(&self) -> PathBuf {
        self.dir.join("signing-key.json")
    }

    pub(crate) fn known_keys(&self) -> PathBuf {
        self.dir.join("known-keys.json")
    }

    /// The environment's token unless it is blank.
    pub(crate) fn token(&self) -> Option<&str> {
        self.token.as_deref().filter(|t| !t.trim().is_empty())
    }
}

impl ConfiguredRegistry {
    /// The credential the user keeps for `registry`, for a read: the stored one
    /// ([`specforge_registry_client::CredentialStore::credential`]), `None` when there is none. The
    /// environment's `SPECFORGE_REGISTRY_TOKEN` is not used: it is the token of the registry a publish
    /// goes to, and a read may ask every registry (search). A stored credential that can't be used
    /// refuses the read before any request.
    pub(crate) fn read_credential(
        &self,
        registry: &RegistryConfig,
    ) -> Result<Option<RegistryCredential>, OpError> {
        self.stored_credential(registry)
            .map_err(unusable_credential)
    }

    /// [`ConfiguredRegistry::read_credential`] as the diagnostic that refuses it.
    pub(crate) fn stored_credential(
        &self,
        registry: &RegistryConfig,
    ) -> Result<Option<RegistryCredential>, Diagnostic> {
        read_credentials(&self.user.credentials())?.credential(&registry.alias)
    }

    /// The credential a publish to `registry` authenticates with: the
    /// environment's token when set and not blank (the store is not read),
    /// else the one stored for the registry's alias.
    pub(crate) fn credential_for(
        &self,
        registry: &RegistryConfig,
    ) -> Result<RegistryCredential, OpError> {
        if let Some(token) = self.user.token() {
            return Ok(RegistryCredential::new(registry.alias.clone(), token));
        }
        let store = read_credentials(&self.user.credentials()).map_err(unusable_credential)?;
        store
            .credential(&registry.alias)
            .map_err(unusable_credential)?
            .ok_or_else(|| {
                OpError::coded(
                    OpErrorKind::PreconditionFailed,
                    NOT_AUTHENTICATED,
                    format!("no credential for registry '{}'", registry.alias),
                )
                .with_suggestion(format!(
                    "log in with `specforge login --registry {} --token <TOKEN>`, or set {TOKEN_VARIABLE}",
                    registry.alias
                ))
            })
    }

    /// The user's publisher key, and whether it was created just now.
    pub(crate) fn signing_key(&self) -> Result<(SigningKey, bool), OpError> {
        let path = self.user.signing_key();
        load_or_create_signing_key_at(&path).map_err(|why| {
            OpError::coded(OpErrorKind::PreconditionFailed, UNUSABLE_SIGNING_KEY, why)
                .with_suggestion(format!(
                    "move {} aside: the next publish creates a new key, and whoever pinned the old one then sees a changed key (R-TRUST-003)",
                    path.display()
                ))
        })
    }
}
