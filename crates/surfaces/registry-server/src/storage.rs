//! Blob storage for published packages.
//!
//! Atomicity contract (C8-06): `store_wasm_temp` writes to a unique temp
//! file and fsyncs; `commit_wasm` promotes it with an atomic rename after
//! the database row has been committed. A crash or rejected publish can
//! therefore only ever leave a temp file (garbage-collectable), never a
//! torn or orphaned final blob.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct LocalStorage {
    base_dir: PathBuf,
}

impl LocalStorage {
    /// Storage under `base_dir`, created when missing.
    pub fn new(base_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&base_dir)
            .map_err(|e| format!("failed to create storage directory: {e}"))?;
        Ok(Self { base_dir })
    }

    /// Storage-safe directory key for a package name. The name may contain
    /// `/` (scoped packages) and the platform path separator must never
    /// appear inside it; percent-encoding makes the mapping injective
    /// (`%` itself is encoded first so no round-trip ambiguity exists).
    fn package_dir_key(name: &str) -> String {
        let encoded = name.replace('%', "%25").replace('/', "%2F");
        // defensive: strip path separators that could survive on some hosts
        encoded.replace(['/', '\\'], "_")
    }

    fn package_dir(&self, name: &str) -> PathBuf {
        self.base_dir.join(Self::package_dir_key(name))
    }

    fn wasm_path(&self, name: &str, version: &str) -> PathBuf {
        self.package_dir(name).join(format!("{}.wasm", version))
    }

    /// Write the payload to a unique temp file (fsynced) without touching
    /// the final blob location. Returns the temp path for `commit_wasm`.
    pub fn store_wasm_temp(
        &self,
        name: &str,
        version: &str,
        data: &[u8],
    ) -> Result<PathBuf, String> {
        let dir = self.package_dir(name);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("failed to create package directory: {e}"))?;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let temp = dir.join(format!(
            ".tmp-{}-{}-{}.wasm",
            version.replace('/', "_"),
            std::process::id(),
            nanos
        ));

        use std::io::Write;
        let mut file =
            std::fs::File::create(&temp).map_err(|e| format!("failed to create temp blob: {e}"))?;
        file.write_all(data)
            .map_err(|e| format!("failed to write temp blob: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("failed to sync temp blob: {e}"))?;

        Ok(temp)
    }

    /// Atomically promote the temp blob to its final location. Same-
    /// filesystem rename: either the full blob appears or nothing does.
    pub fn commit_wasm(&self, name: &str, version: &str, temp: &PathBuf) -> Result<(), String> {
        let final_path = self.wasm_path(name, version);
        std::fs::rename(temp, &final_path).map_err(|e| format!("failed to commit blob: {e}"))
    }

    /// Best-effort temp cleanup for rejected publishes.
    pub fn discard_temp(temp: &PathBuf) {
        let _ = std::fs::remove_file(temp);
    }

    pub fn read_wasm(&self, name: &str, version: &str) -> Option<Vec<u8>> {
        std::fs::read(self.wasm_path(name, version)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_dir_keys_are_collision_free() {
        // the audit's C8-06 collision: '@a/pkg_1' vs '@a_b/c' flattened to
        // the same directory under the old '/'->'_' mapping. Percent-
        // encoding keeps them distinct.
        let a = LocalStorage::package_dir_key("@a/pkg_1");
        let b = LocalStorage::package_dir_key("@a_b/c");
        assert_ne!(a, b);
        assert_eq!(
            LocalStorage::package_dir_key("@org/software"),
            "@org%2Fsoftware"
        );
    }

    #[test]
    fn temp_commit_round_trip_is_atomic_in_intent() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().to_path_buf()).unwrap();

        let temp = storage
            .store_wasm_temp("@t/pkg", "1.0.0", b"payload-bytes")
            .unwrap();
        // final blob must not exist before commit
        assert!(storage.read_wasm("@t/pkg", "1.0.0").is_none());

        storage.commit_wasm("@t/pkg", "1.0.0", &temp).unwrap();
        assert!(storage.read_wasm("@t/pkg", "1.0.0").is_some());
        assert_eq!(
            storage.read_wasm("@t/pkg", "1.0.0").unwrap(),
            b"payload-bytes"
        );
        // temp consumed by the rename
        assert!(!temp.exists());
    }

    #[test]
    fn discarded_temp_leaves_no_blob() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().to_path_buf()).unwrap();
        let temp = storage
            .store_wasm_temp("@t/pkg", "1.0.0", b"rejected")
            .unwrap();
        LocalStorage::discard_temp(&temp);
        assert!(storage.read_wasm("@t/pkg", "1.0.0").is_none());
    }

    #[test]
    fn distinct_names_never_share_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().to_path_buf()).unwrap();
        let t1 = storage
            .store_wasm_temp("@a/pkg_1", "1.0.0", b"one")
            .unwrap();
        storage.commit_wasm("@a/pkg_1", "1.0.0", &t1).unwrap();
        let t2 = storage.store_wasm_temp("@a_b/c", "1.0.0", b"two").unwrap();
        storage.commit_wasm("@a_b/c", "1.0.0", &t2).unwrap();
        assert_eq!(
            storage.read_wasm("@a/pkg_1", "1.0.0").unwrap(),
            b"one",
            "@a/pkg_1 must not see @a_b/c's bytes"
        );
        assert_eq!(
            storage.read_wasm("@a_b/c", "1.0.0").unwrap(),
            b"two",
            "@a_b/c must not see @a/pkg_1's bytes"
        );
    }
}
