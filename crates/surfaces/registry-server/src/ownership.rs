//! Who may write into a scope (publish, yank): the token's filter, then the scope's owner (ADR 0185).

use rusqlite::{Connection, params};
use specforge_package::PackageName;

use crate::db::TokenRecord;

/// Why a token may not write into a package's scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteRefused {
    /// The token's filter (`token create --scope`) does not cover the package: 403 FORBIDDEN.
    Outside { filter: String, package: String },
    /// Another account owns the scope: 403 SCOPE_OWNED.
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
                "scope '{scope}' is owned by account '{owner}' — only tokens issued to it may publish or yank there"
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

/// An account's name as an operator gives it: 1 to 39 of `a-z`, `0-9` and `-`, not starting with `-`.
pub fn account_name(text: &str) -> Result<&str, String> {
    let valid = (1..=39).contains(&text.len())
        && !text.starts_with('-')
        && text
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'));
    if valid {
        Ok(text)
    } else {
        Err(format!(
            "'{text}' is not an account name: 1 to 39 of a-z, 0-9 and '-', not starting with '-'"
        ))
    }
}

/// The token's own filter covers `package`: no database read.
fn within_filter(writer: &TokenRecord, package: &PackageName) -> Result<(), WriteRefused> {
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
        Some(account) if account == writer.account => Ok(Claim::Owned),
        Some(account) => Err(WriteRefused::Owned {
            scope: scope.to_string(),
            owner: account,
        }),
    }
}

/// Claim an unclaimed scope for the writer's account; true when this call claimed it.
pub(crate) fn claim(
    conn: &Connection,
    scope: &str,
    writer: &TokenRecord,
) -> rusqlite::Result<bool> {
    Ok(conn.execute(
        "INSERT OR IGNORE INTO scopes (scope, account) VALUES (?1, ?2)",
        params![scope, writer.account],
    )? > 0)
}

/// The account that owns `scope`, if claimed.
pub(crate) fn owner(conn: &Connection, scope: &str) -> rusqlite::Result<Option<String>> {
    match conn.query_row(
        "SELECT account FROM scopes WHERE scope = ?1",
        params![scope],
        |row| row.get(0),
    ) {
        Ok(found) => Ok(Some(found)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests;
