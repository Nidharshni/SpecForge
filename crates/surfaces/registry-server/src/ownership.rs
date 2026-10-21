//! Who may write into a scope (publish, yank): the token's filter, then the scope's owner (ADR 0185).

use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use specforge_package::PackageName;

use crate::db::TokenRecord;

/// Why a token may not write into a package's scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteRefused {
    /// The token's filter (`token create --scope`) does not cover the package: 403 FORBIDDEN.
    Outside { filter: String, package: String },
    /// Another owner holds the scope: 403 SCOPE_OWNED.
    Owned { scope: String, owner: String },
    /// The database failed: 500 DB_ERROR.
    Storage(String),
}

impl WriteRefused {
    /// The message the registry answers with, one spelling for publish and yank.
    pub fn message(&self) -> String {
        match self {
            Self::Outside { filter, package } => {
                format!("token's scope '{filter}' does not cover '{package}'")
            }
            Self::Owned { scope, owner } => format!(
                "scope '{scope}' is owned by publisher '{owner}' — only the owning publisher may publish into it"
            ),
            Self::Storage(why) => why.clone(),
        }
    }
}

/// Whether the scope was already the writer's, or nobody's yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    Owned,
    Unclaimed,
}

/// Registry-assigned publisher identity: deterministic per issuing token.
pub(crate) fn account_id_for(token_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token_hash.as_bytes());
    format!("acct_{}", &hex::encode(hasher.finalize())[..8])
}

/// The token's own filter covers `package`: no database read.
pub(crate) fn within_filter(
    writer: &TokenRecord,
    package: &PackageName,
) -> Result<(), WriteRefused> {
    if crate::auth::token_has_scope(writer, package) {
        Ok(())
    } else {
        Err(WriteRefused::Outside {
            filter: writer.scope.clone().unwrap_or_default(),
            package: package.to_string(),
        })
    }
}

/// May `writer` write into the scope of `package`? The filter first, then the scope's owner, read
/// through `conn` (a transaction derefs to one, so a write asks inside itself).
pub(crate) fn may_write(
    conn: &Connection,
    writer: &TokenRecord,
    package: &PackageName,
) -> Result<Claim, WriteRefused> {
    within_filter(writer, package)?;
    let Some(scope) = package.scope() else {
        return Ok(Claim::Unclaimed);
    };
    match owner(conn, scope)
        .map_err(|e| WriteRefused::Storage(format!("scope query failed: {e}")))?
    {
        None => Ok(Claim::Unclaimed),
        Some((hash, _)) if hash == writer.token_hash => Ok(Claim::Owned),
        Some((_, account)) => Err(WriteRefused::Owned {
            scope: scope.to_string(),
            owner: account,
        }),
    }
}

/// Claim an unclaimed scope for `writer`; true when this call claimed it.
pub(crate) fn claim(
    conn: &Connection,
    scope: &str,
    writer: &TokenRecord,
) -> rusqlite::Result<bool> {
    Ok(conn.execute(
        "INSERT OR IGNORE INTO scopes (scope, owner_token_hash, account_id) VALUES (?1, ?2, ?3)",
        params![scope, writer.token_hash, account_id_for(&writer.token_hash)],
    )? > 0)
}

/// The owner (token hash) and account id of `scope`, if claimed.
pub(crate) fn owner(conn: &Connection, scope: &str) -> rusqlite::Result<Option<(String, String)>> {
    match conn.query_row(
        "SELECT owner_token_hash, account_id FROM scopes WHERE scope = ?1",
        params![scope],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ) {
        Ok(found) => Ok(Some(found)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests;
