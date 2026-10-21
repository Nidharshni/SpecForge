//! A stored version: the row and the binary of one `name@version`, keyed by the parsed
//! [`PackageName`] and [`Version`] (ADR 0192). The handlers ask this module; nothing else maps a
//! name or a version to a file path, so a route segment that is no version never reaches the disk.
//!
//! Atomicity contract (C8-06): [`Stored::stage`] writes a unique fsynced temp file; [`Staged::commit`]
//! promotes it with an atomic rename after the database row has been committed. A crash or a rejected
//! publish can only leave a temp file (collectable), never a torn or orphaned final blob.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use specforge_package::{PackageName, Version};

use crate::db::{Database, PackageVersion, PublishRefused, TokenRecord};
use crate::ownership::WriteRefused;

/// The served binary does not hash to the digest its row records.
#[derive(Debug, PartialEq, Eq)]
pub struct DigestMismatch;

/// A publish that did not store the version.
#[derive(Debug)]
pub enum PublishFailed {
    /// The database refused the row; the staged blob was discarded.
    Refused(PublishRefused),
    /// The blob could not be committed; the row and the scope claim were withdrawn.
    Blob(String),
}

/// A blob written to a temp file, not yet visible as a version's binary.
#[must_use = "a staged blob is committed or discarded"]
#[derive(Debug)]
pub struct Staged {
    temp: PathBuf,
    target: PathBuf,
}

impl Staged {
    /// Best-effort removal, for a publish that was refused.
    pub fn discard(self) {
        let _ = std::fs::remove_file(self.temp);
    }

    /// Atomically promote the temp blob to its final location (same-filesystem rename).
    fn commit(self) -> Result<(), String> {
        std::fs::rename(&self.temp, &self.target).map_err(|e| format!("failed to commit blob: {e}"))
    }
}

/// The stored versions of a registry: its database and its blob directory.
pub struct Stored<'a> {
    pub(crate) db: &'a Database,
    pub(crate) dir: &'a Path,
}

impl Stored<'_> {
    /// Directory key for a package name. The name contains `/` when scoped; percent-encoding makes
    /// the mapping injective (`%` is encoded first).
    fn package_dir_key(name: &PackageName) -> String {
        name.as_str().replace('%', "%25").replace('/', "%2F")
    }

    fn package_dir(&self, name: &PackageName) -> PathBuf {
        self.dir.join(Self::package_dir_key(name))
    }

    fn blob_path(&self, name: &PackageName, version: &Version) -> PathBuf {
        self.package_dir(name).join(format!("{version}.wasm"))
    }

    /// The row of `name@version`; `None` when it was never published or is yanked.
    pub fn row(&self, name: &PackageName, version: &Version) -> Option<PackageVersion> {
        self.db
            .get_package_version(name.as_str(), &version.to_string())
    }

    /// The versions of `name` that can be fetched, in publish order.
    pub fn versions(&self, name: &PackageName) -> Result<Vec<String>, String> {
        self.db.get_package_versions(name.as_str())
    }

    /// The binary of `name@version`. The row comes first: no row (never published, yanked) means no
    /// binary is read. `Ok(None)` when there is no row or no blob; the digest the row records is
    /// what every served binary is held to.
    pub fn blob(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Result<Option<Vec<u8>>, DigestMismatch> {
        let Some(row) = self.row(name, version) else {
            return Ok(None);
        };
        let Ok(data) = std::fs::read(self.blob_path(name, version)) else {
            return Ok(None);
        };
        if hex::encode(Sha256::digest(&data)) == row.sha256 {
            Ok(Some(data))
        } else {
            Err(DigestMismatch)
        }
    }

    /// Write `data` to a unique fsynced temp file beside the final blob location.
    pub fn stage(
        &self,
        name: &PackageName,
        version: &Version,
        data: &[u8],
    ) -> Result<Staged, String> {
        let dir = self.package_dir(name);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("failed to create package directory: {e}"))?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let temp = dir.join(format!(".tmp-{}-{}.wasm", std::process::id(), nanos));

        use std::io::Write;
        let mut file =
            std::fs::File::create(&temp).map_err(|e| format!("failed to create temp blob: {e}"))?;
        file.write_all(data)
            .map_err(|e| format!("failed to write temp blob: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("failed to sync temp blob: {e}"))?;
        Ok(Staged {
            temp,
            target: self.blob_path(name, version),
        })
    }

    /// Record `row` as `writer` publishes it (claiming the scope when unowned), then make `staged`
    /// its binary. A refusal discards the blob; a blob that cannot be committed withdraws the row,
    /// so a row never outlives a missing blob.
    pub fn publish(
        &self,
        staged: Staged,
        row: &PackageVersion,
        package: &PackageName,
        writer: &TokenRecord,
    ) -> Result<(), PublishFailed> {
        let claimed = match self.db.publish(row, package, writer) {
            Ok(claimed) => claimed,
            Err(refused) => {
                staged.discard();
                return Err(PublishFailed::Refused(refused));
            }
        };
        staged.commit().map_err(|why| {
            let scope = package.scope().unwrap_or_default();
            self.db.withdraw(&row.name, &row.version, scope, claimed);
            PublishFailed::Blob(why)
        })
    }

    /// Take `name@version` down: its metadata and its binary are no longer served. `Ok(false)` when
    /// there is no such version; `writer` is held to the publish rule.
    pub fn yank(
        &self,
        name: &PackageName,
        version: &Version,
        writer: &TokenRecord,
    ) -> Result<bool, WriteRefused> {
        self.db.yank(name, &version.to_string(), writer)
    }

    /// Store `row` and `wasm` past the publish checks (test seeding).
    #[cfg(feature = "testing")]
    pub fn seed(&self, name: &PackageName, version: &Version, row: &PackageVersion, wasm: &[u8]) {
        let staged = self.stage(name, version, wasm).expect("a blob written");
        self.db.insert_package(row).expect("a row inserted");
        staged.commit().expect("a blob committed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> PackageName {
        PackageName::from_url_segment(text).unwrap()
    }

    #[test]
    fn package_dir_keys_are_collision_free() {
        // C8-06: '@a/pkg_1' vs '@a_b/c' once flattened to the same directory.
        let a = Stored::package_dir_key(&name("@a/pkg_1"));
        assert_ne!(a, Stored::package_dir_key(&name("@a_b/c")));
        assert_eq!(
            Stored::package_dir_key(&name("@org/software")),
            "@org%2Fsoftware"
        );
    }
}
