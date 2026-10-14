use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specforge_common::{Diagnostic, codes};

use super::registry_config::RegistryCredential;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CredentialStore {
    pub registries: HashMap<String, CredentialEntry>,
}

/// One registry's entry in `~/.specforge/credentials.json`: where its token comes from. Never in
/// `specforge.json`.
///
/// Untagged: the variants with a required key come first, so `{"token_env": ...}` is a reference
/// and not a `Token` whose every field has a default.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CredentialEntry {
    /// The value of an environment variable (`login --token-env VAR`).
    EnvVar { token_env: String },
    /// The trimmed content of a file (`login --token-file PATH`).
    File { token_file: PathBuf },
    /// The secret `specforge login --token` stored: in the OS keyring (`in_keyring`), else in a 0600
    /// file under `~/.specforge/secrets/`; `token` holds a plaintext token only in a file written by
    /// hand (or by a test).
    Token {
        /// The raw token. Empty when the secret lives in the OS keyring or the secrets file.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        token: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<String>,
        /// Secret stored in the OS keyring, not in this file.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        in_keyring: bool,
    },
}

/// What `login --token-env` / `--token-file` keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenReference {
    Env(String),
    File(PathBuf),
}

impl CredentialStore {
    /// The credential the user keeps for `alias`, resolved: `None` when there is no entry. Refused:
    /// R-AUTH-020 (the stored token expired), R-AUTH-021 (the keyring entry is unreadable), R010 (the
    /// variable is unset or blank), R011 (the file can't be read, or is empty). Each refusal's
    /// suggestion names `specforge login --registry <alias>`.
    pub fn credential(&self, alias: &str) -> Result<Option<RegistryCredential>, Diagnostic> {
        let Some(entry) = self.registries.get(alias) else {
            return Ok(None);
        };
        let relogin = || format!("run: specforge login --registry {alias} --token <NEW_TOKEN>");
        let token = match entry {
            CredentialEntry::EnvVar { token_env } => match std::env::var(token_env) {
                Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
                _ => {
                    return Err(Diagnostic::new(
                        codes::R010,
                        format!(
                            "environment variable '{token_env}' is not set for registry '{alias}'"
                        ),
                    )
                    .with_suggestion(format!(
                        "set it (export {token_env}=<token>), or log in again with `specforge login --registry {alias}`"
                    )));
                }
            },
            CredentialEntry::File { token_file } => match std::fs::read_to_string(token_file) {
                Ok(text) if !text.trim().is_empty() => text.trim().to_string(),
                Ok(_) => {
                    return Err(Diagnostic::new(
                        codes::R011,
                        format!(
                            "token file '{}' for registry '{alias}' is empty",
                            token_file.display()
                        ),
                    )
                    .with_suggestion(format!(
                        "put the token in the file, or log in again with `specforge login --registry {alias}`"
                    )));
                }
                Err(e) => {
                    return Err(Diagnostic::new(
                        codes::R011,
                        format!(
                            "cannot read token file '{}' for registry '{alias}': {e}",
                            token_file.display()
                        ),
                    )
                    .with_suggestion(format!(
                        "check that the file exists and is readable, or log in again with `specforge login --registry {alias}`"
                    )));
                }
            },
            CredentialEntry::Token {
                token,
                expires_at,
                in_keyring,
            } => {
                // Expiry is known metadata: a clear re-login error beats a
                // cryptic 401 from the server later.
                if let Some(expires_at) = expires_at
                    .as_deref()
                    .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
                    .filter(|deadline| chrono::Utc::now() > *deadline)
                {
                    return Err(Diagnostic::new(
                        codes::R_AUTH_020,
                        format!(
                            "stored token for registry '{}' expired at {}",
                            alias, expires_at
                        ),
                    )
                    .with_suggestion(relogin()));
                }
                if *in_keyring {
                    match super::secrets::load_secret(alias) {
                        Ok(Some(secret)) if !secret.is_empty() => secret,
                        Ok(_) => {
                            return Err(Diagnostic::new(
                                codes::R_AUTH_021,
                                format!(
                                    "keyring credential for '{}' is unreadable or missing",
                                    alias
                                ),
                            )
                            .with_suggestion(relogin()));
                        }
                        Err(message) => {
                            return Err(Diagnostic::new(codes::R_AUTH_021, message)
                                .with_suggestion(relogin()));
                        }
                    }
                } else if !token.is_empty() {
                    // A plaintext entry written by hand.
                    token.clone()
                } else {
                    // The file fallback written when the keyring round-trip failed at login.
                    match super::secrets::load_secret(alias) {
                        Ok(Some(secret)) if !secret.is_empty() => secret,
                        _ => token.clone(),
                    }
                }
            }
        };
        Ok(Some(RegistryCredential::new(alias, token)))
    }

    /// Keep `alias`'s token as a reference to `source` (no secret is stored).
    pub fn set_reference(&mut self, alias: &str, source: TokenReference) {
        let entry = match source {
            TokenReference::Env(token_env) => CredentialEntry::EnvVar { token_env },
            TokenReference::File(token_file) => CredentialEntry::File { token_file },
        };
        self.registries.insert(alias.to_string(), entry);
    }

    /// Store a login: the secret goes to the OS keyring when available
    /// (fallback: 0600 file), the file keeps only metadata. Plaintext
    /// entries migrate to the keyring on the next login through this path.
    pub fn set_token(
        &mut self,
        alias: &str,
        token: String,
        expires_at: Option<String>,
    ) -> Result<(), String> {
        let mut backend = super::secrets::store_secret(alias, &token)?;
        // Self-verify the round trip. Some platform keychain domains accept a
        // write but subsequent reads return NoEntry (observed on macOS with
        // keyring-rs v3's data-protection domain), which would lock the user
        // out on the next command. When the keyring claims success but the
        // read-back fails, force the file backend.
        if backend == super::secrets::SecretBackend::Keyring {
            let readable = super::secrets::load_secret(alias)
                .ok()
                .flatten()
                .is_some_and(|s| s == token);
            if !readable {
                // drop the orphaned keyring item (best-effort) and use the
                // file backend instead
                super::secrets::delete_secret(alias);
                backend = super::secrets::store_secret_file(alias, &token)?;
            }
        }
        self.registries.insert(
            alias.to_string(),
            CredentialEntry::Token {
                token: String::new(),
                expires_at,
                in_keyring: backend == super::secrets::SecretBackend::Keyring,
            },
        );
        Ok(())
    }

    pub fn remove(&mut self, alias: &str) -> bool {
        // Best-effort: also drop any keyring/file secret for this alias.
        super::secrets::delete_secret(alias);
        self.registries.remove(alias).is_some()
    }
}

/// The directory that holds the user's registry files (`credentials.json`,
/// `signing-key.json`, `known-keys.json`): `$HOME/.specforge`.
pub fn user_dir() -> PathBuf {
    dirs_home().join(".specforge")
}

pub fn credentials_path() -> PathBuf {
    user_dir().join("credentials.json")
}

pub fn read_credentials(path: &Path) -> Result<CredentialStore, Diagnostic> {
    if !path.exists() {
        return Ok(CredentialStore::default());
    }

    let content = std::fs::read_to_string(path).map_err(|e| {
        Diagnostic::new(
            codes::R012,
            format!("failed to read credentials file: {}", e),
        )
        .with_suggestion(format!("check permissions on '{}'", path.display()))
    })?;

    serde_json::from_str(&content).map_err(|e| {
        Diagnostic::new(
            codes::R012,
            format!("invalid credentials file format: {}", e),
        )
        .with_suggestion(format!(
            "delete '{}' and run `specforge login` again",
            path.display()
        ))
    })
}

pub fn write_credentials(path: &Path, store: &CredentialStore) -> Result<(), Diagnostic> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Diagnostic::new(
                codes::R013,
                format!("failed to create credentials directory: {}", e),
            )
        })?;
    }

    let json = serde_json::to_string_pretty(store).map_err(|e| {
        Diagnostic::new(
            codes::R013,
            format!("failed to serialize credentials: {}", e),
        )
    })?;

    std::fs::write(path, json).map_err(|e| {
        Diagnostic::new(
            codes::R013,
            format!("failed to write credentials file: {}", e),
        )
        .with_suggestion(format!("check write permissions on '{}'", path.display()))
    })?;
    restrict_permissions(path);
    Ok(())
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

pub(crate) fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_mock_keyring() {
        use std::sync::OnceLock;
        static ONCE: OnceLock<()> = OnceLock::new();
        ONCE.get_or_init(|| {
            keyring::set_default_credential_builder(Box::new(crate::secrets::tests::mock::Builder));
        });
    }

    /// The in-memory keyring mock is process-global: tests that exercise
    /// multi-step set/read sequences on the same alias must hold this lock
    /// or they interleave and observe each other's tokens.
    static MOCK_KEYRING_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock_mock_keyring() -> std::sync::MutexGuard<'static, ()> {
        MOCK_KEYRING_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn set_token_keeps_secret_out_of_the_file() {
        install_mock_keyring();
        let _keyring_guard = lock_mock_keyring();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");

        let mut store = CredentialStore::default();
        store
            .set_token(
                "default",
                "sfr_super_secret".to_string(),
                Some("2027-01-01T00:00:00+00:00".to_string()),
            )
            .unwrap();
        write_credentials(&path, &store).unwrap();

        // The file on disk must not contain the token.
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(
            !on_disk.contains("sfr_super_secret"),
            "plaintext token leaked to file"
        );
        assert!(on_disk.contains("\"in_keyring\": true"));

        // The store reconstructs the credential from the keyring.
        let loaded = read_credentials(&path).unwrap();
        let cred = loaded.credential("default").unwrap().unwrap();
        assert_eq!(cred.token(), "sfr_super_secret");
    }

    #[test]
    fn expired_token_is_refused_with_relogin_hint() {
        install_mock_keyring();
        let _keyring_guard = lock_mock_keyring();
        let mut store = CredentialStore::default();
        store
            .set_token(
                "default",
                "sfr_old".to_string(),
                Some("2020-01-01T00:00:00+00:00".to_string()),
            )
            .unwrap();

        let err = store.credential("default").unwrap_err();
        assert_eq!(err.code, "R-AUTH-020");
        assert!(err.message.contains("expired"));
        assert!(
            err.suggestion
                .unwrap_or_default()
                .contains("specforge login")
        );
    }

    #[test]
    fn a_reference_entry_is_not_read_as_a_stored_token() {
        let store: CredentialStore = serde_json::from_str(
            r#"{"registries":{"a":{"token_env":"X"},"b":{"token_file":"/t"}}}"#,
        )
        .unwrap();
        assert!(matches!(
            store.registries["a"],
            CredentialEntry::EnvVar { .. }
        ));
        assert!(matches!(
            store.registries["b"],
            CredentialEntry::File { .. }
        ));
    }

    #[test]
    fn legacy_plaintext_entries_still_resolve() {
        let mut store = CredentialStore::default();
        store.registries.insert(
            "default".to_string(),
            CredentialEntry::Token {
                token: "sfr_legacy".to_string(),
                expires_at: None,
                in_keyring: false,
            },
        );
        let cred = store.credential("default").unwrap().unwrap();
        assert_eq!(cred.token(), "sfr_legacy");
    }
}
