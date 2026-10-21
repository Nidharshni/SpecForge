use crate::ownership::{self, Account, Claim, WriteRefused};
use rusqlite::{Connection, params};
use specforge_package::PackageName;
use specforge_registry_wire::PackageSignature;
use std::path::Path;

/// The schema this registry reads and writes (`PRAGMA user_version`). A data directory written before the
/// signature was typed (ADR 0061), or before a token was issued to an account (ADR 0185), is refused.
const SCHEMA: i64 = 2;
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

/// The length of a token id: the first hex digits of its hash, as the listing shows.
pub const TOKEN_ID_LEN: usize = 8;

/// Why a token was not revoked ([`Database::revoke_token`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevokeRefused {
    /// The segment is not a token id (8 lowercase hex digits).
    NotAnId,
    /// Two live tokens share the id; neither is revoked.
    Ambiguous,
    /// The database failed.
    Storage(String),
}

/// Why a publish was not recorded ([`Database::publish`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishRefused {
    /// The token may not write into the scope (its filter, or another owner).
    Write(WriteRefused),
    /// The version is already stored (the message says so).
    DuplicateVersion(String),
    /// The database failed.
    Storage(String),
}

impl From<WriteRefused> for PublishRefused {
    fn from(refused: WriteRefused) -> Self {
        match refused {
            WriteRefused::Storage(why) => Self::Storage(why),
            other => Self::Write(other),
        }
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
    /// The publisher signature, stored as its JSON; `None` only for what `testing::LocalRegistry::store` stores
    /// unsigned. Its key id is `signature.key_id()`.
    pub signature: Option<PackageSignature>,
    /// Exact manifest JSON uploaded with the package; served so clients can
    /// verify manifest_sha256 offline.
    pub manifest: String,
}

#[derive(Debug, Clone)]
pub struct TokenRecord {
    pub token_hash: String,
    /// The account the token was issued to; it, not the token, owns the scopes the token claims.
    pub account: Account,
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

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|e| format!("failed to read the schema version: {e}"))?;
        let has_packages: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'packages'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .map_err(|e| format!("failed to read the schema: {e}"))?;
        if has_packages && version != SCHEMA {
            return Err(format!(
                "the registry data in {} was written by an earlier specforge-registry (schema {version}); this one reads schema {SCHEMA}: serve from a new --data-dir",
                path.display()
            ));
        }

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
                signature TEXT,
                manifest TEXT NOT NULL DEFAULT '',
                UNIQUE(name, version)
            );

            CREATE INDEX IF NOT EXISTS idx_packages_name ON packages(name);
            CREATE INDEX IF NOT EXISTS idx_packages_keywords ON packages(keywords);

            CREATE TABLE IF NOT EXISTS accounts (
                name TEXT PRIMARY KEY,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS tokens (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                token_hash TEXT NOT NULL UNIQUE,
                account TEXT NOT NULL REFERENCES accounts(name),
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
                account TEXT NOT NULL REFERENCES accounts(name),
                claimed_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .map_err(|e| format!("migration failed: {}", e))?;

        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA}"))
            .map_err(|e| format!("migration failed (schema version): {e}"))?;
        Ok(())
    }

    pub fn insert_package(&self, pkg: &PackageVersion) -> Result<(), String> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO packages (name, version, sha256, size_bytes, description, keywords, publisher, published_at, signature, manifest)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                pkg.name,
                pkg.version,
                pkg.sha256,
                pkg.size_bytes,
                pkg.description,
                pkg.keywords,
                pkg.publisher,
                pkg.published_at,
                pkg.signature.as_ref().map(PackageSignature::to_json),
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
            "SELECT name, version, sha256, size_bytes, description, keywords, publisher, published_at, signature, manifest
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
                    signature: row
                        .get::<_, Option<String>>(8)?
                        .map(|text| {
                            PackageSignature::parse(&text).map_err(|why| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    8,
                                    rusqlite::types::Type::Text,
                                    Box::new(why),
                                )
                            })
                        })
                        .transpose()?,
                    manifest: row.get(9)?,
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
        // An escaped prefilter (`%`, `_` and `\` are text); `specforge_package::search::matches`
        // decides, as the in-memory registries do.
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{escaped}%");
        // Every non-yanked version of each package one of whose versions matches the term.
        let mut stmt = conn
            .prepare(
                "SELECT name, version, sha256, size_bytes, description, keywords, publisher, published_at, manifest
                 FROM packages
                 WHERE yanked = 0 AND name IN (
                   SELECT name FROM packages
                   WHERE yanked = 0 AND (name LIKE ?1 ESCAPE '\\' OR description LIKE ?1 ESCAPE '\\' OR keywords LIKE ?1 ESCAPE '\\'))",
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
                    signature: None,
                    manifest: row.get(8)?,
                })
            })
            .map_err(|e| format!("search query failed: {e}"))?
            .filter_map(|r| r.ok())
            .collect();
        // The one rule decides which of a package's versions' terms match: the prefilter spans
        // the joined keyword column, so `a,b` passes it and fails here.
        let matching: std::collections::BTreeSet<String> = rows
            .iter()
            .filter(|p| {
                let keywords: Vec<&str> = p.keywords.split(',').filter(|k| !k.is_empty()).collect();
                specforge_package::search::matches(query, &p.name, &p.description, &keywords)
            })
            .map(|p| p.name.clone())
            .collect();
        let rows: Vec<PackageVersion> = rows
            .into_iter()
            .filter(|p| matching.contains(&p.name))
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

    /// May `writer` write into the scope of `package`? The read-only pre-check of [`Self::publish`].
    pub fn may_write(
        &self,
        writer: &TokenRecord,
        package: &PackageName,
    ) -> Result<(), WriteRefused> {
        ownership::may_write(&self.conn(), writer, package).map(drop)
    }

    /// Record `pkg` as `writer` publishes it into the scope of `package`, in one transaction: ask
    /// [`ownership::may_write`], claim the scope for the writer when nobody owns it, insert the
    /// version (`UNIQUE(name, version)` refuses a duplicate). A refusal leaves the scope as it was.
    /// `Ok(true)` when this publish claimed the scope.
    pub fn publish(
        &self,
        pkg: &PackageVersion,
        package: &PackageName,
        writer: &TokenRecord,
    ) -> Result<bool, PublishRefused> {
        let mut conn = self.conn();
        let storage =
            |e: rusqlite::Error| PublishRefused::Storage(format!("failed to publish: {e}"));
        let tx = conn.transaction().map_err(storage)?;
        let mut claimed = false;
        if ownership::may_write(&tx, writer, package)? == Claim::Unclaimed
            && let Some(scope) = package.scope()
        {
            claimed = ownership::claim(&tx, scope.as_str(), writer).map_err(storage)?;
            if !claimed {
                // Claimed between the read and the insert: ask again.
                ownership::may_write(&tx, writer, package)?;
            }
        }
        let inserted = tx.execute(
            "INSERT INTO packages (name, version, sha256, size_bytes, description, keywords, publisher, published_at, signature, manifest)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                pkg.name,
                pkg.version,
                pkg.sha256,
                pkg.size_bytes,
                pkg.description,
                pkg.keywords,
                pkg.publisher,
                pkg.published_at,
                pkg.signature.as_ref().map(PackageSignature::to_json),
                pkg.manifest,
            ],
        );
        if let Err(e) = inserted {
            return Err(if e.to_string().contains("UNIQUE constraint") {
                PublishRefused::DuplicateVersion(format!(
                    "version {} already exists for {}",
                    pkg.version, pkg.name
                ))
            } else {
                storage(e)
            });
        }
        tx.commit().map_err(storage)?;
        Ok(claimed)
    }

    /// Hand `scope` to `account`; the previous owner (an operator's act, never a request's).
    pub fn assign_scope(
        &self,
        scope: &str,
        account: &Account,
    ) -> Result<Option<String>, ownership::AssignRefused> {
        ownership::assign(&self.conn(), scope, account)
    }

    /// The account that owns `scope`, if it has been claimed.
    pub fn scope_owner(&self, scope: &str) -> Option<String> {
        ownership::owner(&self.conn(), scope).ok().flatten()
    }

    /// Withdraw a version whose blob could not be committed, and the scope claim the same publish
    /// made (`claimed`) unless a package of the scope has been stored since: the compensation of
    /// [`Self::publish`]. True when a row was removed.
    pub fn withdraw(&self, name: &str, version: &str, scope: &str, claimed: bool) -> bool {
        let conn = self.conn();
        let removed = conn
            .execute(
                "DELETE FROM packages WHERE name = ?1 AND version = ?2",
                params![name, version],
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        if claimed {
            let _ = conn.execute(
                "DELETE FROM scopes WHERE scope = ?1 AND NOT EXISTS (
                   SELECT 1 FROM packages WHERE substr(name, 1, length(?1) + 1) = ?1 || '/')",
                params![scope],
            );
        }
        removed
    }

    /// Withdraw a version, held to the publish rule: `writer` must be allowed into the package's scope,
    /// asked in the same transaction as the `UPDATE`. `Ok(false)` when no such version exists. Never
    /// claims a scope.
    pub fn yank(
        &self,
        package: &PackageName,
        version: &str,
        writer: &TokenRecord,
    ) -> Result<bool, WriteRefused> {
        let mut conn = self.conn();
        let storage = |e: rusqlite::Error| WriteRefused::Storage(format!("failed to yank: {e}"));
        let tx = conn.transaction().map_err(storage)?;
        ownership::may_write(&tx, writer, package)?;
        let rows = tx
            .execute(
                "UPDATE packages SET yanked = 1 WHERE name = ?1 AND version = ?2",
                params![package.as_str(), version],
            )
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(rows > 0)
    }

    // --- Token management ---

    /// Store a token issued to `account`, creating the account with its first token.
    pub fn insert_token(
        &self,
        token_hash: &str,
        account: &Account,
        scope: Option<&str>,
        label: &str,
        expires_at: Option<&str>,
        admin: bool,
    ) -> Result<(), String> {
        let mut conn = self.conn();
        let storage = |e: rusqlite::Error| format!("failed to insert token: {e}");
        let tx = conn.transaction().map_err(storage)?;
        tx.execute(
            "INSERT OR IGNORE INTO accounts (name) VALUES (?1)",
            params![account],
        )
        .map_err(storage)?;
        tx.execute(
            "INSERT INTO tokens (token_hash, account, scope, label, expires_at, admin) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![token_hash, account, scope, label, expires_at, admin as i64],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)
    }

    /// Whether any token was ever issued to `account`.
    pub fn has_account(&self, account: &Account) -> bool {
        self.conn()
            .query_row(
                "SELECT 1 FROM accounts WHERE name = ?1",
                params![account],
                |_| Ok(()),
            )
            .is_ok()
    }

    pub fn validate_token(&self, token_hash: &str) -> Option<TokenRecord> {
        let conn = self.conn();
        conn.query_row(
            "SELECT token_hash, account, scope, label, created_at, expires_at, admin FROM tokens WHERE token_hash = ?1 AND revoked = 0",
            params![token_hash],
            |row| {
                Ok(TokenRecord {
                    token_hash: row.get(0)?,
                    account: row.get(1)?,
                    scope: row.get(2)?,
                    label: row.get(3)?,
                    created_at: row.get(4)?,
                    expires_at: row.get(5)?,
                    admin: row.get::<_, i64>(6)? != 0,
                })
            },
        )
        .ok()
    }

    pub fn list_tokens(&self) -> Result<Vec<TokenRecord>, String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT token_hash, account, scope, label, created_at, expires_at, admin FROM tokens WHERE revoked = 0 ORDER BY created_at")
            .map_err(|e| format!("token listing failed: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TokenRecord {
                    token_hash: row.get(0)?,
                    account: row.get(1)?,
                    scope: row.get(2)?,
                    label: row.get(3)?,
                    created_at: row.get(4)?,
                    expires_at: row.get(5)?,
                    admin: row.get::<_, i64>(6)? != 0,
                })
            })
            .map_err(|e| format!("token listing failed: {e}"))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Revoke the token whose id (the first 8 hex digits of its hash, as the listing shows) is
    /// `id`: `Ok(true)` when exactly one live token had it, `Ok(false)` when none did. An id is
    /// never a pattern.
    pub fn revoke_token(&self, id: &str) -> Result<bool, RevokeRefused> {
        if id.len() != TOKEN_ID_LEN || !id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(RevokeRefused::NotAnId);
        }
        let conn = self.conn();
        let storage =
            |e: rusqlite::Error| RevokeRefused::Storage(format!("token revocation failed: {e}"));
        let mut stmt = conn
            .prepare(
                "SELECT token_hash FROM tokens WHERE substr(token_hash, 1, 8) = ?1 AND revoked = 0",
            )
            .map_err(storage)?;
        let hashes: Vec<String> = stmt
            .query_map(params![id], |row| row.get(0))
            .map_err(storage)?
            .filter_map(|r| r.ok())
            .collect();
        match hashes.as_slice() {
            [] => Ok(false),
            [hash] => {
                conn.execute(
                    "UPDATE tokens SET revoked = 1 WHERE token_hash = ?1",
                    params![hash],
                )
                .map_err(storage)?;
                Ok(true)
            }
            _ => Err(RevokeRefused::Ambiguous),
        }
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
            signature: None,
            manifest: String::new(),
        }
    }

    fn name(text: &str) -> PackageName {
        PackageName::parse(text).unwrap()
    }

    /// A token of `account`, which the database must know (the scope table references it).
    fn writer(db: &Database, account: &str) -> TokenRecord {
        let hash = format!("hash-{account}");
        let _ = db.insert_token(
            &hash,
            &Account::parse(account).unwrap(),
            None,
            "t",
            None,
            false,
        );
        db.validate_token(&hash).unwrap()
    }

    #[test]
    fn an_older_data_directory_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE packages (id INTEGER PRIMARY KEY, name TEXT)")
                .unwrap();
        }
        let error = Database::open(&path)
            .err()
            .expect("an older file is refused");
        assert!(error.contains("--data-dir"), "{error}");

        // An empty file opens and carries the schema version.
        let fresh = dir.path().join("fresh.db");
        drop(Database::open(&fresh).unwrap());
        let version: i64 = Connection::open(&fresh)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA);
        Database::open(&fresh).expect("a current file reopens");
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

    #[specforge_test_macros::test(
        behavior = "publish_to_registry",
        verify = "a publish the registry refuses claims no scope"
    )]
    fn a_refused_publish_leaves_the_scope_unclaimed() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        // The lost race: the version is stored between the handler's check and the transaction.
        db.insert_package(&pv("@fresh/x", "1.0.0")).unwrap();
        let refused = db.publish(
            &pv("@fresh/x", "1.0.0"),
            &name("@fresh/x"),
            &writer(&db, "tok-a"),
        );
        assert!(
            matches!(refused, Err(PublishRefused::DuplicateVersion(_))),
            "{refused:?}"
        );
        assert_eq!(db.scope_owner("@fresh"), None);

        // A publish that stores claims; another token's is refused and the owner stays.
        assert_eq!(
            db.publish(
                &pv("@fresh/y", "1.0.0"),
                &name("@fresh/y"),
                &writer(&db, "tok-a")
            ),
            Ok(true)
        );
        assert_eq!(
            db.publish(
                &pv("@fresh/z", "1.0.0"),
                &name("@fresh/z"),
                &writer(&db, "tok-a")
            ),
            Ok(false)
        );
        assert_eq!(
            db.publish(
                &pv("@fresh/w", "1.0.0"),
                &name("@fresh/w"),
                &writer(&db, "tok-b")
            ),
            Err(PublishRefused::Write(WriteRefused::Owned {
                scope: "@fresh".to_string(),
                owner: "tok-a".to_string()
            }))
        );
        assert_eq!(db.scope_owner("@fresh"), Some("tok-a".to_string()));
    }

    #[test]
    fn withdrawing_a_claiming_publish_releases_the_scope() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        assert_eq!(
            db.publish(
                &pv("@fresh/y", "1.0.0"),
                &name("@fresh/y"),
                &writer(&db, "tok-a")
            ),
            Ok(true)
        );
        assert!(db.withdraw("@fresh/y", "1.0.0", "@fresh", true));
        assert_eq!(db.scope_owner("@fresh"), None);

        // A scope that holds another package keeps its claim.
        assert_eq!(
            db.publish(
                &pv("@fresh/y", "1.0.0"),
                &name("@fresh/y"),
                &writer(&db, "tok-a")
            ),
            Ok(true)
        );
        assert_eq!(
            db.publish(
                &pv("@fresh/z", "1.0.0"),
                &name("@fresh/z"),
                &writer(&db, "tok-a")
            ),
            Ok(false)
        );
        db.withdraw("@fresh/y", "1.0.0", "@fresh", true);
        assert!(db.scope_owner("@fresh").is_some());
    }
}
