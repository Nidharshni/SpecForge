//! User-level trust store for publisher keys (spec #21 / ticket T2).
//!
//! Backs the TOFU key-pinning model decided on wayfinder map #8:
//! - `pins`: package name → key id, recorded on first verified install;
//!   later installs must match (key-change flow re-pins interactively).
//! - `trusted_keys`: operator allowlist — these keys are acceptable without
//!   a prior pin (air-gapped / pre-seeded setups).
//! - `denied_keys`: config-level revocation — refuses a key even if pinned
//!   or trusted (no server infrastructure in v1).
//!
//! Stored at `~/.specforge/known-keys.json`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// User-level publisher key state.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnownKeys {
    /// TOFU pins: package name → key id recorded at first verified install.
    #[serde(default)]
    pub pins: BTreeMap<String, String>,
    /// Operator allowlist: keys acceptable without a prior pin.
    #[serde(default)]
    pub trusted_keys: Vec<String>,
    /// Config-level revocation: refuse these keys outright.
    #[serde(default)]
    pub denied_keys: Vec<String>,
}

impl KnownKeys {
    /// The pin recorded for a package, if any.
    pub fn pin_for(&self, name: &str) -> Option<&str> {
        self.pins.get(name).map(String::as_str)
    }

    /// Record/overwrite the pin for a package.
    pub fn pin(&mut self, name: &str, key_id: &str) {
        self.pins.insert(name.to_string(), key_id.to_string());
    }

    /// Whether the key is explicitly denied (config-level revocation).
    pub fn is_denied(&self, key_id: &str) -> bool {
        self.denied_keys.iter().any(|k| k == key_id)
    }

    /// Whether the key is on the operator allowlist.
    pub fn is_trusted(&self, key_id: &str) -> bool {
        self.trusted_keys.iter().any(|k| k == key_id)
    }
}

/// Default on-disk location of the known-keys store.
pub fn known_keys_path() -> PathBuf {
    crate::credentials::user_dir().join("known-keys.json")
}

/// The trust store at `path`. A missing file is an empty store. A file that can't be read, or isn't
/// the store's JSON, is an error saying why: a store read as empty would trust any key and drop
/// `denied_keys`.
pub fn load_known_keys_at(path: &std::path::Path) -> Result<KnownKeys, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(KnownKeys::default()),
        Err(e) => Err(e.to_string()),
    }
}

/// Save the store to `path` whole: written beside it, owner-only (0600 on Unix), then renamed over
/// it, so a failed save leaves the previous file as it was.
pub fn save_known_keys_at(path: &std::path::Path, keys: &KnownKeys) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }
    let json = serde_json::to_string_pretty(keys)
        .map_err(|e| format!("failed to serialize known keys: {}", e))?;
    let mut temp_name = path.file_name().unwrap_or_default().to_os_string();
    temp_name.push(".tmp");
    let temp = path.with_file_name(temp_name);
    let saved = std::fs::write(&temp, json).and_then(|()| {
        restrict_permissions(&temp);
        std::fs::rename(&temp, path)
    });
    saved.map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("failed to write {}: {}", path.display(), e)
    })
}

#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_loads_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = load_known_keys_at(&dir.path().join("known-keys.json")).unwrap();
        assert_eq!(store, KnownKeys::default());
        assert!(store.pin_for("@acme/x").is_none());
    }

    #[test]
    fn pins_round_trip_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known-keys.json");

        let mut store = KnownKeys::default();
        store.pin("@acme/x", "key-1111");
        store.trusted_keys.push("key-2222".to_string());
        store.denied_keys.push("key-3333".to_string());
        save_known_keys_at(&path, &store).unwrap();

        let loaded = load_known_keys_at(&path).unwrap();
        assert_eq!(loaded.pin_for("@acme/x"), Some("key-1111"));
        assert!(loaded.is_trusted("key-2222"));
        assert!(loaded.is_denied("key-3333"));
        assert!(!loaded.is_denied("key-1111"));
    }

    #[test]
    fn a_malformed_store_is_an_error_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known-keys.json");
        std::fs::write(&path, r#"{"pins":{},}"#).unwrap();

        let why = load_known_keys_at(&path).unwrap_err();

        assert!(why.contains("trailing comma"), "{why}");
        // A path that is a directory can't be read either.
        assert!(load_known_keys_at(dir.path()).is_err());
    }

    #[test]
    fn a_save_replaces_the_file_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known-keys.json");
        let mut store = KnownKeys::default();
        store.pin("@acme/x", "key-1111");
        save_known_keys_at(&path, &store).unwrap();
        store.pin("@acme/x", "key-2222");
        save_known_keys_at(&path, &store).unwrap();

        assert_eq!(
            load_known_keys_at(&path).unwrap().pin_for("@acme/x"),
            Some("key-2222")
        );
        let files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(files, ["known-keys.json"], "no temp file is left beside it");
    }

    #[test]
    fn pin_overwrite_changes_the_recorded_key() {
        let mut store = KnownKeys::default();
        store.pin("@acme/x", "key-old");
        store.pin("@acme/x", "key-new");
        assert_eq!(store.pin_for("@acme/x"), Some("key-new"));
    }

    #[test]
    fn unpinned_name_has_no_pin() {
        let store = KnownKeys::default();
        assert_eq!(store.pin_for("other"), None);
    }
}
