use sha2::{Digest, Sha256};
use specforge_package::PackageName;

use crate::db::{Database, TokenRecord};

/// Create a token. `expires_in_days` = `Some(0)` expires immediately;
/// `None` never expires (the `--no-expiry` escape). Default policy is 90 days.
pub fn create_token(
    db: &Database,
    scope: Option<&str>,
    label: &str,
    expires_in_days: Option<u64>,
    admin: bool,
) -> String {
    let raw_token = generate_raw_token();
    let hash = hash_token(&raw_token);
    let expires_at = expires_in_days
        .map(|days| (chrono::Utc::now() + chrono::Duration::days(days as i64)).to_rfc3339());
    db.insert_token(&hash, scope, label, expires_at.as_deref(), admin)
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

/// Whether the token's scope covers `name`: no scope covers every package;
/// otherwise the scope itself or a name under it (`@org` covers `@org/tool`).
pub fn token_has_scope(record: &TokenRecord, name: &PackageName) -> bool {
    covers(record, name.as_str())
}

fn covers(record: &TokenRecord, package_name: &str) -> bool {
    match &record.scope {
        None => true,
        Some(scope) => {
            package_name == scope
                || package_name
                    .strip_prefix(scope)
                    .is_some_and(|rest| rest.starts_with('/'))
        }
    }
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

    fn token(scope: Option<&str>) -> TokenRecord {
        TokenRecord {
            token_hash: "hash".to_string(),
            scope: scope.map(str::to_string),
            label: "test".to_string(),
            created_at: "2026-01-01".to_string(),
            expires_at: None,
            admin: false,
        }
    }

    #[test]
    fn expired_token_fails_validation() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        // expires_in_days = Some(0) expires immediately
        let raw = create_token(&db, None, "short-lived", Some(0), false);
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
        let raw = create_token(&db, None, "normal", Some(90), false);
        let header = format!("Bearer {}", raw);
        assert!(validate_bearer(&db, &header).is_some());
    }

    #[test]
    fn admin_flag_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let raw = create_token(&db, None, "root", Some(90), true);
        let record = validate_bearer(&db, &format!("Bearer {}", raw)).unwrap();
        assert!(is_admin(&record));
    }

    #[test]
    fn token_without_scope_grants_all() {
        assert!(covers(&token(None), "web"));
        assert!(covers(&token(None), "webui"));
    }

    #[test]
    fn exact_scope_match_allowed() {
        assert!(covers(&token(Some("web")), "web"));
    }

    #[test]
    fn scope_prefix_collision_denied() {
        assert!(!covers(&token(Some("web")), "webui"));
    }

    #[test]
    fn package_under_scope_allowed() {
        assert!(covers(&token(Some("web")), "web/sub"));
        assert!(covers(&token(Some("web")), "web/sub/deep"));
    }

    #[test]
    fn package_outside_scope_denied() {
        assert!(!covers(&token(Some("web")), "other"));
        assert!(!covers(&token(Some("web")), "websub"));
    }

    #[test]
    fn npm_style_scope_matches_only_own_packages() {
        assert!(covers(&token(Some("@web")), "@web/ui"));
        assert!(!covers(&token(Some("@web")), "@webui/x"));
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    fn name(text: &str) -> PackageName {
        PackageName::parse(text).unwrap()
    }

    fn token(scope: Option<&str>) -> TokenRecord {
        TokenRecord {
            token_hash: "hash".to_string(),
            scope: scope.map(str::to_string),
            label: "test".to_string(),
            created_at: "2026-01-01".to_string(),
            expires_at: None,
            admin: false,
        }
    }

    #[test]
    fn scope_prefix_does_not_leak_into_longer_names() {
        // C8-10/C14-06: a token scoped to '@org/soft' must NOT authorize
        // '@org/software' — the boundary is the '/', not the prefix.
        let t = token(Some("@org/soft"));
        assert!(token_has_scope(&t, &name("@org/soft")));
        assert!(!token_has_scope(&t, &name("@org/software")));
    }

    #[test]
    fn namespace_scope_covers_exactly_its_namespace() {
        let t = token(Some("@org"));
        assert!(token_has_scope(&t, &name("@org/software")));
        assert!(covers(&t, "@org"));
        assert!(!token_has_scope(&t, &name("@other/software")));
    }

    #[test]
    fn unscoped_token_has_full_access() {
        assert!(token_has_scope(&token(None), &name("@anything/here")));
    }
}
