//! Who may write into a scope (publish, yank): the token's filter, then the scope's owner (ADR 0185).

use rusqlite::{Connection, params};
use specforge_package::{PackageName, Scope};

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

/// Why a scope was not assigned ([`assign`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignRefused {
    /// No token was ever issued to the account.
    UnknownAccount(String),
    /// The database failed.
    Storage(String),
}

impl std::fmt::Display for AssignRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownAccount(account) => write!(
                f,
                "no token was issued to account '{account}': create one with `token create --account`"
            ),
            Self::Storage(why) => f.write_str(why),
        }
    }
}

/// Whether the scope was already the writer's, or nobody's yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    Owned,
    Unclaimed,
}

/// An account's name, validated once: 1 to 39 of `a-z`, `0-9` and `-`, not starting with `-`. It is a
/// plain string on the wire and in the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account(String);

impl Account {
    pub fn parse(text: &str) -> Result<Self, String> {
        let valid = (1..=39).contains(&text.len())
            && !text.starts_with('-')
            && text
                .bytes()
                .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'));
        if valid {
            Ok(Self(text.to_string()))
        } else {
            Err(format!(
                "'{text}' is not an account name: 1 to 39 of a-z, 0-9 and '-', not starting with '-'"
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::str::FromStr for Account {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        Self::parse(text)
    }
}

impl std::fmt::Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad(&self.0)
    }
}

impl rusqlite::ToSql for Account {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

impl rusqlite::types::FromSql for Account {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let text = value.as_str()?;
        Self::parse(text).map_err(|why| rusqlite::types::FromSqlError::Other(why.into()))
    }
}

/// The token's own filter covers `package`: no database read.
fn within_filter(writer: &TokenRecord, package: &PackageName) -> Result<(), WriteRefused> {
    match &writer.scope {
        Some(filter) if !filter.covers(package) => Err(WriteRefused::Outside {
            filter: filter.to_string(),
            package: package.to_string(),
        }),
        _ => Ok(()),
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
    match owner(conn, scope.as_str())
        .map_err(|e| WriteRefused::Storage(format!("scope query failed: {e}")))?
    {
        None => Ok(Claim::Unclaimed),
        Some(account) if account == writer.account.as_str() => Ok(Claim::Owned),
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

/// Hand `scope` to `account`, claimed or not; the previous owner. The account must exist, so a typo
/// creates none.
pub(crate) fn assign(
    conn: &Connection,
    scope: &Scope,
    account: &Account,
) -> Result<Option<String>, AssignRefused> {
    let scope = scope.as_str();
    let storage = |e: rusqlite::Error| AssignRefused::Storage(format!("failed to assign: {e}"));
    match conn.query_row(
        "SELECT 1 FROM accounts WHERE name = ?1",
        params![account],
        |_| Ok(()),
    ) {
        Ok(()) => {}
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(AssignRefused::UnknownAccount(account.to_string()));
        }
        Err(e) => return Err(storage(e)),
    }
    let previous = owner(conn, scope).map_err(storage)?;
    conn.execute(
        "INSERT INTO scopes (scope, account) VALUES (?1, ?2)
         ON CONFLICT(scope) DO UPDATE SET account = excluded.account",
        params![scope, account],
    )
    .map_err(storage)?;
    Ok(previous)
}

#[cfg(test)]
mod tests;
