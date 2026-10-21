use sha2::{Digest, Sha256};
use specforge_package::Scope;

use crate::db::{Database, TokenRecord};
use crate::ownership::Account;

/// Create a token issued to `account`. `expires_in_days` = `Some(0)` expires immediately;
/// `None` never expires (the `--no-expiry` escape). Default policy is 90 days.
pub fn create_token(
    db: &Database,
    account: &Account,
    scope: Option<&Scope>,
    label: &str,
    expires_in_days: Option<u64>,
    admin: bool,
) -> String {
    let raw_token = generate_raw_token();
    let hash = hash_token(&raw_token);
    let expires_at = expires_in_days
        .map(|days| (chrono::Utc::now() + chrono::Duration::days(days as i64)).to_rfc3339());
    db.insert_token(&hash, account, scope, label, expires_at.as_deref(), admin)
        .expect("failed to store token");
    raw_token
}

/// Whether the record grants admin (token administration) rights.
pub fn is_admin(record: &TokenRecord) -> bool {
    record.admin
}

pub fn list_tokens(db: &Database) -> Result<Vec<TokenRecord>, String> {
    db.list_tokens()
}

/// Revoke the token whose id (8 hex digits of its hash) is `id`; never a pattern.
pub fn revoke_token(db: &Database, id: &str) -> Result<bool, crate::db::RevokeRefused> {
    db.revoke_token(id)
}

pub fn validate_bearer(db: &Database, auth_header: &str) -> Option<TokenRecord> {
    let token = auth_header.strip_prefix("Bearer ")?;
    let hash = hash_token(token);
    let record = db.validate_token(&hash)?;
    // Expired tokens are invalid: the caller sees a plain 401.
    if let Some(expires_at) = &record.expires_at {
        let deadline = chrono::DateTime::parse_from_rfc3339(expires_at).ok()?;
        if chrono::Utc::now() > deadline {
            return None;
        }
    }
    Some(record)
}

pub(crate) fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn generate_raw_token() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    format!("sfr_{}", hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acct(text: &str) -> Account {
        Account::parse(text).unwrap()
    }

    #[test]
    fn a_token_is_validated_with_its_account() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let raw = create_token(&db, &acct("acme"), None, "ci", Some(90), false);
        let record = validate_bearer(&db, &format!("Bearer {raw}")).expect("a live token");
        assert_eq!(record.account, acct("acme"));
        assert!(db.has_account(&acct("acme")) && !db.has_account(&acct("other")));
    }

    #[test]
    fn a_token_whose_stored_scope_is_no_scope_authenticates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let db = Database::open(&path).unwrap();
        let raw = create_token(&db, &acct("a"), None, "ci", Some(90), false);
        assert!(validate_bearer(&db, &format!("Bearer {raw}")).is_some());

        rusqlite::Connection::open(&path)
            .unwrap()
            .execute("UPDATE tokens SET scope = 'sdk'", [])
            .unwrap();

        assert!(validate_bearer(&db, &format!("Bearer {raw}")).is_none());
    }

    #[test]
    fn expired_token_fails_validation() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        // expires_in_days = Some(0) expires immediately
        let raw = create_token(&db, &acct("a"), None, "short-lived", Some(0), false);
        let header = format!("Bearer {}", raw);
        assert!(
            validate_bearer(&db, &header).is_none(),
            "expired token must not validate"
        );
    }

    #[test]
    fn unexpired_token_validates() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let raw = create_token(&db, &acct("a"), None, "normal", Some(90), false);
        let header = format!("Bearer {}", raw);
        assert!(validate_bearer(&db, &header).is_some());
    }

    #[test]
    fn admin_flag_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let raw = create_token(&db, &acct("a"), None, "root", Some(90), true);
        let record = validate_bearer(&db, &format!("Bearer {}", raw)).unwrap();
        assert!(is_admin(&record));
    }
}
