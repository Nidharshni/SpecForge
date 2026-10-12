use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Poison-recovering lock (C8-08): a panic in one query must not lock
    /// every future request out of the database. rusqlite keeps the file
    /// consistent through the journal, so recovering the guard is safe.
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Debug, Clone)]
pub struct PackageVersion {
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub description: String,
    pub keywords: String,
    pub publisher: String,
    pub published_at: String,
    /// Wire signature object (JSON with sig/keyId/pubkey/signedAt); empty when unsigned.
    pub signature: String,
    /// Short publisher key id; empty when unsigned.
    pub key_id: String,
    /// Exact manifest JSON uploaded with the package; served so clients can
    /// verify manifest_sha256 offline.
    pub manifest: String,
}

#[derive(Debug, Clone)]
pub struct TokenRecord {
    pub token_hash: String,
    pub scope: Option<String>,
    pub label: String,
    pub created_at: String,
    /// RFC3339 expiry instant; `None` = never expires (--no-expiry escape).
    pub expires_at: Option<String>,
    /// Admin tokens may create/list/revoke other tokens via the admin API.
    pub admin: bool,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("failed to open database: {}", e))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )
        .map_err(|e| format!("failed to set pragmas: {e}"))?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = self.conn();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS packages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                description TEXT NOT NULL DEFAULT '',
                keywords TEXT NOT NULL DEFAULT '',
                publisher TEXT NOT NULL DEFAULT '',
                published_at TEXT NOT NULL DEFAULT (datetime('now')),
                yanked INTEGER NOT NULL DEFAULT 0,
                UNIQUE(name, version)
            );

            CREATE INDEX IF NOT EXISTS idx_packages_name ON packages(name);
            CREATE INDEX IF NOT EXISTS idx_packages_keywords ON packages(keywords);

            CREATE TABLE IF NOT EXISTS tokens (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                token_hash TEXT NOT NULL UNIQUE,
                scope TEXT,
                label TEXT NOT NULL DEFAULT 'default',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                revoked INTEGER NOT NULL DEFAULT 0,
                expires_at TEXT,
                admin INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_tokens_hash ON tokens(token_hash);

            CREATE TABLE IF NOT EXISTS scopes (
                scope TEXT PRIMARY KEY,
                owner_token_hash TEXT NOT NULL,
                account_id TEXT NOT NULL,
                claimed_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .map_err(|e| format!("migration failed: {}", e))?;

        // Signature columns arrived after the initial schema: add them to
        // databases created before signed publishing existed.
        for (column, ddl) in [
            (
                "signature",
                "ALTER TABLE packages ADD COLUMN signature TEXT NOT NULL DEFAULT ''",
            ),
            (
                "key_id",
                "ALTER TABLE packages ADD COLUMN key_id TEXT NOT NULL DEFAULT ''",
            ),
            (
                "manifest",
                "ALTER TABLE packages ADD COLUMN manifest TEXT NOT NULL DEFAULT ''",
            ),
        ] {
            let present: bool = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM pragma_table_info('packages') WHERE name = '{column}'"
                    ),
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map(|n| n > 0)
                .unwrap_or(false);
            if !present {
                conn.execute_batch(ddl)
                    .map_err(|e| format!("migration failed ({column}): {}", e))?;
            }
        }
        Ok(())
    }

    pub fn insert_package(&self, pkg: &PackageVersion) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO packages (name, version, sha256, size_bytes, description, keywords, publisher, published_at, signature, key_id, manifest)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                pkg.name,
                pkg.version,
                pkg.sha256,
                pkg.size_bytes,
                pkg.description,
                pkg.keywords,
                pkg.publisher,
                pkg.published_at,
                pkg.signature,
                pkg.key_id,
                pkg.manifest,
            ],
        ).map_err(|e| {
            if e.to_string().contains("UNIQUE constraint") {
                format!("version {} already exists for {}", pkg.version, pkg.name)
            } else {
                format!("failed to insert package: {}", e)
            }
        })?;
        Ok(())
    }

    pub fn get_package_version(&self, name: &str, version: &str) -> Option<PackageVersion> {
        let conn = self.conn();
        conn.query_row(
            "SELECT name, version, sha256, size_bytes, description, keywords, publisher, published_at, signature, key_id, manifest
             FROM packages WHERE name = ?1 AND version = ?2 AND yanked = 0",
            params![name, version],
            |row| {
                Ok(PackageVersion {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    sha256: row.get(2)?,
                    size_bytes: row.get(3)?,
                    description: row.get(4)?,
                    keywords: row.get(5)?,
                    publisher: row.get(6)?,
                    published_at: row.get(7)?,
                    signature: row.get(8)?,
                    key_id: row.get(9)?,
                    manifest: row.get(10)?,
                })
            },
        )
        .ok()
    }

    pub fn get_package_versions(&self, name: &str) -> Result<Vec<String>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT version FROM packages WHERE name = ?1 AND yanked = 0 ORDER BY published_at",
            )
            .map_err(|e| format!("versions query failed: {e}"))?;
        let rows = stmt
            .query_map(params![name], |row| row.get(0))
            .map_err(|e| format!("versions query failed: {e}"))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Each package one of whose versions matches `query`, at the version `specforge add <name>`
    /// installs (`latest`), sorted by name; with `contributes`, only those whose listed version's
    /// stored declaration declares that category (an unreadable manifest is not kept).
    pub fn search(
        &self,
        query: &str,
        limit: u32,
        contributes: Option<specforge_protocol_types::DeclaredCategory>,
    ) -> Result<Vec<PackageVersion>, String> {
        use specforge_package::{Version, VersionRequirement};
        use std::collections::BTreeMap;

        let conn = self.conn();
        let pattern = format!("%{query}%");
        // Every non-yanked version of each package one of whose versions matches the term.
        let mut stmt = conn
            .prepare(
                "SELECT name, version, sha256, size_bytes, description, keywords, publisher, published_at, manifest
                 FROM packages
                 WHERE yanked = 0 AND name IN (
                   SELECT name FROM packages
                   WHERE yanked = 0 AND (name LIKE ?1 OR description LIKE ?1 OR keywords LIKE ?1))",
            )
            .map_err(|e| format!("search query failed: {e}"))?;
        let rows: Vec<PackageVersion> = stmt
            .query_map(params![pattern], |row| {
                Ok(PackageVersion {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    sha256: row.get(2)?,
                    size_bytes: row.get(3)?,
                    description: row.get(4)?,
                    keywords: row.get(5)?,
                    publisher: row.get(6)?,
                    published_at: row.get(7)?,
                    signature: String::new(),
                    key_id: String::new(),
                    manifest: row.get(8)?,
                })
            })
            .map_err(|e| format!("search query failed: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        // Each package at what `latest` picks, by SemVer (SQL MAX(version) is lexicographic and
        // would rank 9.0.0 above 10.0.0); the map yields the names in order.
        let mut by_name: BTreeMap<String, Vec<PackageVersion>> = BTreeMap::new();
        for row in rows {
            by_name.entry(row.name.clone()).or_default().push(row);
        }
        let mut out: Vec<PackageVersion> = by_name
            .into_values()
            .filter_map(|versions| {
                VersionRequirement::Latest.pick_from(versions, |p| Version::parse(&p.version).ok())
            })
            .collect();
        if let Some(category) = contributes {
            out.retain(|p| {
                serde_json::from_str::<specforge_protocol_types::ExtensionDeclaration>(&p.manifest)
                    .is_ok_and(|d| d.declares(category))
            });
        }
        out.truncate(limit as usize);
        Ok(out)
    }

    /// Claim a namespace scope on first publish. Returns `true` when this
    /// call claimed it, `false` when it was already owned (by anyone).
    pub fn claim_scope(
        &self,
        scope: &str,
        owner_token_hash: &str,
        account_id: &str,
    ) -> Result<bool, String> {
        let conn = self.conn();
        let rows = conn
            .execute(
                "INSERT OR IGNORE INTO scopes (scope, owner_token_hash, account_id) VALUES (?1, ?2, ?3)",
                params![scope, owner_token_hash, account_id],
            )
            .map_err(|e| format!("failed to claim scope: {}", e))?;
        Ok(rows > 0)
    }

    /// The owner (token hash) and registry-assigned account id of a claimed
    /// scope, if it has been claimed.
    pub fn get_scope_owner(&self, scope: &str) -> Option<(String, String)> {
        let conn = self.conn();
        conn.query_row(
            "SELECT owner_token_hash, account_id FROM scopes WHERE scope = ?1",
            params![scope],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok()
    }

    /// Compensating delete for a publish whose blob commit failed after
    /// the row landed. Returns true when a row was removed.
    pub fn delete_package(&self, name: &str, version: &str) -> bool {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM packages WHERE name = ?1 AND version = ?2",
            params![name, version],
        )
        .map(|n| n > 0)
        .unwrap_or(false)
    }

    pub fn yank_version(&self, name: &str, version: &str) -> bool {
        let conn = self.conn();
        let rows = conn
            .execute(
                "UPDATE packages SET yanked = 1 WHERE name = ?1 AND version = ?2",
                params![name, version],
            )
            .unwrap_or(0);
        rows > 0
    }

    // --- Token management ---

    #[allow(clippy::too_many_arguments)]
    pub fn insert_token(
        &self,
        token_hash: &str,
        scope: Option<&str>,
        label: &str,
        expires_at: Option<&str>,
        admin: bool,
    ) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO tokens (token_hash, scope, label, expires_at, admin) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![token_hash, scope, label, expires_at, admin as i64],
        )
        .map_err(|e| format!("failed to insert token: {}", e))?;
        Ok(())
    }

    pub fn validate_token(&self, token_hash: &str) -> Option<TokenRecord> {
        let conn = self.conn();
        conn.query_row(
            "SELECT token_hash, scope, label, created_at, expires_at, admin FROM tokens WHERE token_hash = ?1 AND revoked = 0",
            params![token_hash],
            |row| {
                Ok(TokenRecord {
                    token_hash: row.get(0)?,
                    scope: row.get(1)?,
                    label: row.get(2)?,
                    created_at: row.get(3)?,
                    expires_at: row.get(4)?,
                    admin: row.get::<_, i64>(5)? != 0,
                })
            },
        )
        .ok()
    }

    pub fn list_tokens(&self) -> Result<Vec<TokenRecord>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT token_hash, scope, label, created_at, expires_at, admin FROM tokens WHERE revoked = 0 ORDER BY created_at")
            .map_err(|e| format!("token listing failed: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TokenRecord {
                    token_hash: row.get(0)?,
                    scope: row.get(1)?,
                    label: row.get(2)?,
                    created_at: row.get(3)?,
                    expires_at: row.get(4)?,
                    admin: row.get::<_, i64>(5)? != 0,
                })
            })
            .map_err(|e| format!("token listing failed: {e}"))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub fn revoke_token_by_prefix(&self, prefix: &str) -> bool {
        let conn = self.conn();
        let pattern = format!("{}%", prefix);
        let rows = conn
            .execute(
                "UPDATE tokens SET revoked = 1 WHERE token_hash LIKE ?1 AND revoked = 0",
                params![pattern],
            )
            .unwrap_or(0);
        rows > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pv(name: &str, version: &str) -> PackageVersion {
        PackageVersion {
            name: name.to_string(),
            version: version.to_string(),
            sha256: "hash".to_string(),
            size_bytes: 1,
            description: format!("{} package", name),
            keywords: String::new(),
            publisher: "tester".to_string(),
            published_at: "2026-01-01".to_string(),
            signature: String::new(),
            key_id: String::new(),
            manifest: String::new(),
        }
    }

    // C8-08: committed data survives an unclean shutdown — the leaked
    // connection never closes cleanly, so the WAL must be recovered on
    // reopen.
    #[test]
    fn reopens_after_unclean_shutdown_recovers_wal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.db");
        {
            let db = Database::open(&path).unwrap();
            db.insert_package(&pv("pkg", "1.0.0")).unwrap();
            std::mem::forget(db);
        }
        let db = Database::open(&path).unwrap();
        let versions = db.get_package_versions("pkg").unwrap();
        assert_eq!(versions, vec!["1.0.0".to_string()]);
    }

    #[test]
    fn search_prefers_semver_latest_over_lexicographic() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        db.insert_package(&pv("pkg", "9.0.0")).unwrap();
        db.insert_package(&pv("pkg", "10.0.0")).unwrap();
        let hits = db.search("pkg", 10, None).unwrap();
        assert_eq!(hits.len(), 1, "one row per package name");
        assert_eq!(hits[0].version, "10.0.0", "10.0.0 must outrank 9.0.0");
    }

    /// The version a search of `published` (all `pkg`) lists.
    fn listed(published: &[&str]) -> String {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        for version in published {
            db.insert_package(&pv("pkg", version)).unwrap();
        }
        let hits = db.search("pkg", 10, None).unwrap();
        assert_eq!(hits.len(), 1, "one row per package name: {published:?}");
        hits[0].version.clone()
    }

    #[specforge_test_macros::test(
        behavior = "search_registry",
        verify = "a registry lists each package at the version add installs from it"
    )]
    fn search_lists_the_version_add_installs() {
        assert_eq!(listed(&["9.0.0", "10.0.0"]), "10.0.0");
        // The highest release, not a pre-release of a higher version.
        assert_eq!(listed(&["2.0.0-beta.1", "1.9.0"]), "1.9.0");
        assert_eq!(listed(&["2.0.0-beta.1", "1.9.0", "2.0.0"]), "2.0.0");
        // A pre-release only when there is no release; identifiers compare as SemVer, not text.
        assert_eq!(listed(&["2.0.0-beta.9", "2.0.0-beta.10"]), "2.0.0-beta.10");
    }
}
