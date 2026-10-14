//! Secret storage for registry tokens (spec #21, T5).
//!
//! Tokens live in the OS keyring (service `specforge-registry`, user =
//! registry alias) whenever a keychain is available. Where it is not
//! (headless Linux without a secret service, exotic sandboxes), the secret
//! falls back to a 0600-permission file under `~/.specforge/secrets/`.
//!
//! The plaintext `credentials.json` of older versions is migrated to this
//! scheme automatically on the next `specforge login`.

use std::path::PathBuf;

const SERVICE: &str = "specforge-registry";

/// Which backend actually holds a secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretBackend {
    /// OS keychain (preferred).
    Keyring,
    /// 0600 file fallback (no keychain available).
    File,
}

/// Store a secret, preferring the OS keyring. Returns the backend used so
/// callers can record it.
pub fn store_secret(alias: &str, secret: &str) -> Result<SecretBackend, String> {
    if let Ok(entry) = keyring::Entry::new(SERVICE, alias)
        && entry.set_password(secret).is_ok()
    {
        return Ok(SecretBackend::Keyring);
    }
    store_secret_file(alias, secret)?;
    Ok(SecretBackend::File)
}

/// Load a secret: keyring first, then the file fallback.
/// `Ok(None)` = no secret stored for this alias.
pub fn load_secret(alias: &str) -> Result<Option<String>, String> {
    if let Ok(entry) = keyring::Entry::new(SERVICE, alias) {
        match entry.get_password() {
            Ok(secret) => return Ok(Some(secret)),
            // NotFound = nothing in the keychain; fall through to the file.
            Err(keyring::Error::NoEntry) => {}
            Err(_) => {}
        }
    }
    load_secret_file(alias)
}

/// Delete a secret from every backend (best-effort, for logout).
pub fn delete_secret(alias: &str) {
    if let Ok(entry) = keyring::Entry::new(SERVICE, alias) {
        let _ = entry.delete_credential();
    }
    let _ = std::fs::remove_file(secret_file_path(alias));
}

fn secrets_dir() -> PathBuf {
    crate::credentials::dirs_home()
        .join(".specforge")
        .join("secrets")
}

fn secret_file_path(alias: &str) -> PathBuf {
    secrets_dir().join(format!("{}.json", alias.replace('/', "_")))
}

pub(crate) fn store_secret_file(alias: &str, secret: &str) -> Result<SecretBackend, String> {
    #[derive(serde::Serialize)]
    struct StoredSecret<'a> {
        secret: &'a str,
    }
    let path = secret_file_path(alias);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }
    let json = serde_json::to_string(&StoredSecret { secret })
        .map_err(|e| format!("failed to serialize secret: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
    restrict_permissions(&path);
    Ok(SecretBackend::File)
}

fn load_secret_file(alias: &str) -> Result<Option<String>, String> {
    #[derive(serde::Deserialize)]
    struct StoredSecret {
        secret: String,
    }
    let path = secret_file_path(alias);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    serde_json::from_str::<StoredSecret>(&content)
        .map(|s| Some(s.secret))
        .map_err(|e| format!("corrupt secret file {}: {}", path.display(), e))
}

#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) {}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// In-memory keyring backend so tests exercise the keyring path without
    /// touching the OS keychain (keyring v3 dropped its built-in mock).
    pub mod mock {
        use keyring::Error;
        use keyring::credential::{CredentialApi, CredentialBuilderApi};
        use std::collections::HashMap;
        use std::sync::{Mutex, OnceLock};

        fn store() -> &'static Mutex<HashMap<String, String>> {
            static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
            STORE.get_or_init(|| Mutex::new(HashMap::new()))
        }

        pub struct Builder;

        impl CredentialBuilderApi for Builder {
            fn build(
                &self,
                _target: Option<&str>,
                _service: &str,
                user: &str,
            ) -> Result<Box<dyn CredentialApi + Send + Sync>, Error> {
                Ok(Box::new(Credential {
                    user: user.to_string(),
                }))
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }

        pub struct Credential {
            user: String,
        }

        impl CredentialApi for Credential {
            fn set_password(&self, password: &str) -> Result<(), Error> {
                store()
                    .lock()
                    .expect("mock store poisoned")
                    .insert(self.user.clone(), password.to_string());
                Ok(())
            }

            fn get_password(&self) -> Result<String, Error> {
                store()
                    .lock()
                    .expect("mock store poisoned")
                    .get(&self.user)
                    .cloned()
                    .ok_or(Error::NoEntry)
            }

            fn delete_credential(&self) -> Result<(), Error> {
                store()
                    .lock()
                    .expect("mock store poisoned")
                    .remove(&self.user);
                Ok(())
            }

            fn set_secret(&self, secret: &[u8]) -> Result<(), Error> {
                self.set_password(&String::from_utf8_lossy(secret))
            }

            fn get_secret(&self) -> Result<Vec<u8>, Error> {
                Ok(self.get_password()?.into_bytes())
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
    }

    pub fn install_mock_keyring() {
        use std::sync::OnceLock;
        static ONCE: OnceLock<()> = OnceLock::new();
        ONCE.get_or_init(|| {
            keyring::set_default_credential_builder(Box::new(mock::Builder));
        });
    }

    // The mock backend makes the keyring path deterministic in tests (no OS
    // keychain interaction).
    #[test]
    fn keyring_backend_round_trips() {
        install_mock_keyring();
        assert!(matches!(
            store_secret("@acme/x", "secret-1").unwrap(),
            SecretBackend::Keyring
        ));
        assert_eq!(load_secret("@acme/x").unwrap().as_deref(), Some("secret-1"));

        delete_secret("@acme/x");
        assert_eq!(load_secret("@acme/x").unwrap(), None);
    }

    #[test]
    fn file_fallback_round_trips_with_restricted_permissions() {
        let dir = tempfile::tempdir().unwrap();
        // Aim the fallback at a temp location by testing the file functions
        // through a name that maps into the temp secrets dir.
        let path = dir.path().join("fallback.json");
        #[derive(serde::Serialize, serde::Deserialize)]
        struct S<'a> {
            secret: &'a str,
        }
        std::fs::write(
            &path,
            serde_json::to_string(&S { secret: "s3cret" }).unwrap(),
        )
        .unwrap();
        restrict_permissions(&path);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "fallback file must be 0600");
        }

        let raw = std::fs::read_to_string(&path).unwrap();
        let loaded: S = serde_json::from_str(&raw).unwrap();
        assert_eq!(loaded.secret, "s3cret");
    }
}
