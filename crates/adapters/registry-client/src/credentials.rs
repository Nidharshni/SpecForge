use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specforge_common::{Diagnostic, codes};

use specforge_registry_wire::{RegistryRoot, sha256_hex};

use super::registry_config::RegistryCredential;

/// The user's credentials: `credentials.json` in one directory, keyed by the registry root each
/// credential was kept for (ADR 0178). The alias is a project's word for a registry, never a key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialStore {
    /// Where it was read from; the secrets `login --token` keeps live beside it.
    #[serde(skip)]
    dir: PathBuf,
    pub registries: HashMap<RegistryRoot, CredentialEntry>,
}

/// One registry's entry in `credentials.json`: where its token comes from. Never in `specforge.json`.
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
    /// The secret `specforge login --token` stored: a 0600 file under `<dir>/secrets/`; `token`
    /// holds a plaintext token only in a file written by hand (or by a test).
    Token {
        /// The raw token. Empty when the secret lives in the secrets file.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        token: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<String>,
    },
}

/// What `login --token-env` / `--token-file` keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenReference {
    Env(String),
    File(PathBuf),
}

impl CredentialStore {
    /// An empty store whose files live in `dir`.
    pub fn empty(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            registries: HashMap::new(),
        }
    }

    /// `<dir>/credentials.json`, or an empty store when there is none. R012 when it can't be read or
    /// parsed (a file keyed by aliases, from before ADR 0178, is one: its keys are not roots).
    pub fn read(dir: &Path) -> Result<Self, Diagnostic> {
        let path = dir.join(FILE);
        if !path.exists() {
            return Ok(Self::empty(dir));
        }

        let content = std::fs::read_to_string(&path).map_err(|e| {
            Diagnostic::new(
                codes::R012,
                format!("failed to read credentials file: {}", e),
            )
            .with_suggestion(format!("check permissions on '{}'", path.display()))
        })?;

        let mut store: Self = serde_json::from_str(&content).map_err(|e| {
            Diagnostic::new(
                codes::R012,
                format!("invalid credentials file format: {}", e),
            )
            .with_suggestion(format!(
                "delete '{}' and run `specforge login` again",
                path.display()
            ))
        })?;
        store.dir = dir.to_path_buf();
        Ok(store)
    }

    /// Written whole to `<dir>/credentials.json`, 0600. R013.
    pub fn write(&self) -> Result<(), Diagnostic> {
        let path = self.dir.join(FILE);
        std::fs::create_dir_all(&self.dir).map_err(|e| {
            Diagnostic::new(
                codes::R013,
                format!("failed to create credentials directory: {}", e),
            )
        })?;

        let json = serde_json::to_string_pretty(self).map_err(|e| {
            Diagnostic::new(
                codes::R013,
                format!("failed to serialize credentials: {}", e),
            )
        })?;

        std::fs::write(&path, json).map_err(|e| {
            Diagnostic::new(
                codes::R013,
                format!("failed to write credentials file: {}", e),
            )
            .with_suggestion(format!("check write permissions on '{}'", path.display()))
        })?;
        restrict_permissions(&path);
        Ok(())
    }

    /// The credential the user keeps for `root`, resolved: `None` when there is no entry. Refused:
    /// R-AUTH-020 (the stored token expired), R-AUTH-021 (the kept secret is missing or unreadable), R010 (the
    /// variable is unset or blank), R011 (the file can't be read, or is empty). Each refusal's
    /// suggestion names `specforge login --registry <alias>`.
    pub fn credential(
        &self,
        root: &RegistryRoot,
    ) -> Result<Option<RegistryCredential>, Diagnostic> {
        let Some(entry) = self.registries.get(root) else {
            return Ok(None);
        };
        let relogin =
            || format!("run: specforge login --registry <alias> --token <NEW_TOKEN> (for {root})");
        let token = match entry {
            CredentialEntry::EnvVar { token_env } => match std::env::var(token_env) {
                Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
                _ => {
                    return Err(Diagnostic::new(
                        codes::R010,
                        format!(
                            "environment variable '{token_env}' is not set for registry '{root}'"
                        ),
                    )
                    .with_suggestion(format!(
                        "set it (export {token_env}=<token>), or log in again with `specforge login --registry <alias>`"
                    )));
                }
            },
            CredentialEntry::File { token_file } => match std::fs::read_to_string(token_file) {
                Ok(text) if !text.trim().is_empty() => text.trim().to_string(),
                Ok(_) => {
                    return Err(Diagnostic::new(
                        codes::R011,
                        format!(
                            "token file '{}' for registry '{root}' is empty",
                            token_file.display()
                        ),
                    )
                    .with_suggestion(
                        "put the token in the file, or log in again with `specforge login --registry <alias>`",
                    ));
                }
                Err(e) => {
                    return Err(Diagnostic::new(
                        codes::R011,
                        format!(
                            "cannot read token file '{}' for registry '{root}': {e}",
                            token_file.display()
                        ),
                    )
                    .with_suggestion(
                        "check that the file exists and is readable, or log in again with `specforge login --registry <alias>`",
                    ));
                }
            },
            CredentialEntry::Token { token, expires_at } => {
                // Expiry is known metadata: a clear re-login error beats a
                // cryptic 401 from the server later.
                if let Some(expires_at) = expires_at
                    .as_deref()
                    .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
                    .filter(|deadline| chrono::Utc::now() > *deadline)
                {
                    return Err(Diagnostic::new(
                        codes::R_AUTH_020,
                        format!("stored token for registry '{root}' expired at {expires_at}"),
                    )
                    .with_suggestion(relogin()));
                }
                if !token.is_empty() {
                    // A plaintext entry written by hand.
                    token.clone()
                } else {
                    // The secret `login --token` kept: a file, or one that is gone or corrupt.
                    match self.read_secret(root) {
                        Ok(Some(secret)) if !secret.is_empty() => secret,
                        Ok(_) => {
                            return Err(Diagnostic::new(
                                codes::R_AUTH_021,
                                format!(
                                    "the secret kept for registry '{root}' is missing or unreadable"
                                ),
                            )
                            .with_suggestion(relogin()));
                        }
                        Err(message) => {
                            return Err(Diagnostic::new(codes::R_AUTH_021, message)
                                .with_suggestion(relogin()));
                        }
                    }
                }
            }
        };
        Ok(Some(RegistryCredential::new(root.clone(), token)))
    }

    /// Keep `root`'s token as a reference to `source` (no secret is stored).
    pub fn keep_reference(&mut self, root: RegistryRoot, source: TokenReference) {
        let entry = match source {
            TokenReference::Env(token_env) => CredentialEntry::EnvVar { token_env },
            TokenReference::File(token_file) => CredentialEntry::File { token_file },
        };
        self.registries.insert(root, entry);
    }

    /// Store a login: the secret goes to a 0600 file under `<dir>/secrets/`, the entry keeps only its
    /// expiry. Any legacy plaintext entry for `root` is replaced.
    pub fn keep_secret(
        &mut self,
        root: RegistryRoot,
        token: String,
        expires_at: Option<String>,
    ) -> Result<(), String> {
        #[derive(Serialize)]
        struct Stored<'a> {
            secret: &'a str,
        }
        let path = self.secret_path(&root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
        }
        let json = serde_json::to_string(&Stored { secret: &token })
            .map_err(|e| format!("failed to serialize secret: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        restrict_permissions(&path);
        self.registries.insert(
            root,
            CredentialEntry::Token {
                token: String::new(),
                expires_at,
            },
        );
        Ok(())
    }

    /// Drop the entry for `root`. The caller writes the store, then calls [`Self::forget_secret`]: a
    /// failed write never leaves an entry whose secret is gone.
    pub fn forget(&mut self, root: &RegistryRoot) -> bool {
        self.registries.remove(root).is_some()
    }

    /// Delete the secret kept for `root` (best-effort).
    pub fn forget_secret(&self, root: &RegistryRoot) {
        let _ = std::fs::remove_file(self.secret_path(root));
    }

    /// `<dir>/secrets/<first 16 hex of SHA-256 of the root>.json`: a root is not a file name.
    fn secret_path(&self, root: &RegistryRoot) -> PathBuf {
        let id = sha256_hex(root.as_str().as_bytes());
        self.dir.join("secrets").join(format!("{}.json", &id[..16]))
    }

    /// `Ok(None)` = no secret kept for `root`.
    fn read_secret(&self, root: &RegistryRoot) -> Result<Option<String>, String> {
        #[derive(Deserialize)]
        struct Stored {
            secret: String,
        }
        let path = self.secret_path(root);
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Ok(None);
        };
        serde_json::from_str::<Stored>(&content)
            .map(|stored| Some(stored.secret))
            .map_err(|e| format!("corrupt secret file {}: {}", path.display(), e))
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

const FILE: &str = "credentials.json";

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> RegistryRoot {
        RegistryRoot::parse(&format!("https://{name}.invalid")).unwrap()
    }

    #[test]
    fn a_kept_secret_is_a_0600_file_beside_the_store() {
        let dir = tempfile::tempdir().unwrap();

        let mut store = CredentialStore::empty(dir.path());
        store
            .keep_secret(
                root("default"),
                "sfr_super_secret".to_string(),
                Some("2027-01-01T00:00:00+00:00".to_string()),
            )
            .unwrap();
        store.write().unwrap();

        // The store on disk must not contain the token.
        let on_disk = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert!(
            !on_disk.contains("sfr_super_secret"),
            "plaintext token leaked to file"
        );

        // The store reconstructs the credential from the secret file.
        let loaded = CredentialStore::read(dir.path()).unwrap();
        let cred = loaded.credential(&root("default")).unwrap().unwrap();
        assert_eq!(cred.token(), "sfr_super_secret");

        let secret = loaded.secret_path(&root("default"));
        assert!(secret.starts_with(dir.path().join("secrets")));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&secret).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "a secret file must be 0600");
        }

        loaded.forget_secret(&root("default"));
        assert!(!secret.exists());
    }

    #[test]
    fn expired_token_is_refused_with_relogin_hint() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = CredentialStore::empty(dir.path());
        store
            .keep_secret(
                root("default"),
                "sfr_old".to_string(),
                Some("2020-01-01T00:00:00+00:00".to_string()),
            )
            .unwrap();

        let err = store.credential(&root("default")).unwrap_err();
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
            r#"{"registries":{"https://a.invalid":{"token_env":"X"},"https://b.invalid":{"token_file":"/t"}}}"#,
        )
        .unwrap();
        assert!(matches!(
            store.registries[&root("a")],
            CredentialEntry::EnvVar { .. }
        ));
        assert!(matches!(
            store.registries[&root("b")],
            CredentialEntry::File { .. }
        ));
    }

    #[test]
    fn legacy_plaintext_entries_still_resolve() {
        let mut store = CredentialStore::empty("unused");
        store.registries.insert(
            root("default"),
            CredentialEntry::Token {
                token: "sfr_legacy".to_string(),
                expires_at: None,
            },
        );
        let cred = store.credential(&root("default")).unwrap().unwrap();
        assert_eq!(cred.token(), "sfr_legacy");
        assert_eq!(cred.root, root("default"));
    }
}
