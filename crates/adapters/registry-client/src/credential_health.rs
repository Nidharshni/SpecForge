//! Registry credential health, which `specforge doctor` reports: the
//! user's, not the project's.

use crate::credentials::{CredentialEntry, CredentialStore};
use serde::Serialize;
use specforge_common::codes;
use std::path::Path;

/// How serious a credential line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CredentialLevel {
    Ok,
    Warning,
    Error,
}

/// One line about one registry's stored credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CredentialLine {
    pub level: CredentialLevel,
    pub text: String,
}

/// The health of the user's registry credentials: expired tokens and
/// unreadable keyring entries break add and publish, so they are worth
/// surfacing before they bite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialHealth {
    /// One line per stored credential worth mentioning, by registry root.
    pub lines: Vec<CredentialLine>,
    /// How many credentials are unusable (doctor then exits 1).
    pub failures: usize,
    /// Whether a publish signing key exists.
    pub signing_key_present: bool,
}

/// The credential health of the credential store and the signing key in the user's directory `dir`,
/// as of `now`.
pub fn credential_health(dir: &Path, now: chrono::DateTime<chrono::Utc>) -> CredentialHealth {
    let mut lines = Vec::new();
    let mut failures = 0usize;
    // An unreadable store is one unusable credential: every reader refuses it (R012).
    let store = CredentialStore::read(dir).unwrap_or_else(|diag| {
        failures += 1;
        lines.push(CredentialLine {
            level: CredentialLevel::Error,
            text: format!("{} — {}", diag.message, diag.suggestion.unwrap_or_default()),
        });
        CredentialStore::empty(dir)
    });
    let mut roots: Vec<_> = store.registries.keys().cloned().collect();
    roots.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    for root in roots {
        match store.credential(&root) {
            // A reference resolves in the shell that runs a command, which need not be
            // doctor's: a variable not set here, or a file not readable here, is a warning.
            Err(diag) if diag.is(codes::R010) || diag.is(codes::R011) => {
                lines.push(CredentialLine {
                    level: CredentialLevel::Warning,
                    text: format!("registry '{root}': {}", diag.message),
                });
            }
            Err(diag) => {
                failures += 1;
                lines.push(CredentialLine {
                    level: CredentialLevel::Error,
                    text: format!("{} — {}", diag.message, diag.suggestion.unwrap_or_default()),
                });
            }
            Ok(Some(_)) => {
                // Healthy; note an approaching expiry so re-login is not a surprise.
                let Some(CredentialEntry::Token {
                    expires_at: Some(expires_at),
                    ..
                }) = store.registries.get(&root)
                else {
                    continue;
                };
                let line = match assess_expiry(Some(expires_at), now) {
                    Some(TokenExpiry::ExpiringSoon(days)) => CredentialLine {
                        level: CredentialLevel::Warning,
                        text: format!(
                            "registry '{root}': token expires in {days} day(s) — re-login soon"
                        ),
                    },
                    // An expired token already failed `credential`.
                    Some(TokenExpiry::Valid) | Some(TokenExpiry::Expired) => CredentialLine {
                        level: CredentialLevel::Ok,
                        text: format!("registry '{root}': token ok"),
                    },
                    None => CredentialLine {
                        level: CredentialLevel::Ok,
                        text: format!("registry '{root}': token ok (expires {expires_at})"),
                    },
                };
                lines.push(line);
            }
            Ok(None) => {}
        }
    }
    CredentialHealth {
        lines,
        failures,
        signing_key_present: dir.join("signing-key.json").exists(),
    }
}

/// Expiry assessment for a stored token timestamp (RFC 3339).
#[derive(Debug, PartialEq)]
enum TokenExpiry {
    Valid,
    ExpiringSoon(i64),
    Expired,
}

/// Within 7 days of expiry (or past it) a token is worth flagging.
const EXPIRY_WARNING_DAYS: i64 = 7;

fn assess_expiry(
    expires_at: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<TokenExpiry> {
    let deadline = chrono::DateTime::parse_from_rfc3339(expires_at?)
        .ok()?
        .with_timezone(&chrono::Utc);
    let days = (deadline - now).num_days();
    if days < 0 {
        Some(TokenExpiry::Expired)
    } else if days <= EXPIRY_WARNING_DAYS {
        Some(TokenExpiry::ExpiringSoon(days))
    } else {
        Some(TokenExpiry::Valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(days_from_now: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() + chrono::Duration::days(days_from_now)
    }

    #[test]
    fn expiry_buckets() {
        let now = chrono::Utc::now();
        let stamp = |days| at(days).to_rfc3339();
        assert_eq!(
            assess_expiry(Some(&stamp(-2)), now),
            Some(TokenExpiry::Expired)
        );
        // num_days truncates elapsed sub-second precision, so allow ±1 day.
        assert!(matches!(
            assess_expiry(Some(&stamp(3)), now),
            Some(TokenExpiry::ExpiringSoon(2)) | Some(TokenExpiry::ExpiringSoon(3))
        ));
        assert_eq!(
            assess_expiry(Some(&stamp(30)), now),
            Some(TokenExpiry::Valid)
        );
        assert_eq!(assess_expiry(None, now), None);
        assert_eq!(assess_expiry(Some("not-a-date"), now), None);
    }

    #[test]
    fn an_unset_token_variable_is_a_warning_in_doctor() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("credentials.json"),
            r#"{"registries":{"https://ci.invalid":{"token_env":"P16_DOCTOR_UNSET_VARIABLE"}}}"#,
        )
        .unwrap();

        let health = credential_health(dir.path(), chrono::Utc::now());

        assert_eq!(health.failures, 0, "{health:?}");
        assert_eq!(health.lines.len(), 1);
        assert_eq!(health.lines[0].level, CredentialLevel::Warning);
        assert!(
            health.lines[0].text.contains("P16_DOCTOR_UNSET_VARIABLE"),
            "{health:?}"
        );
    }

    #[test]
    fn credential_health_reads_the_store_it_is_given() {
        let dir = tempfile::TempDir::new().unwrap();
        let token =
            |days: i64| serde_json::json!({"token": "t", "expires_at": at(days).to_rfc3339()});
        std::fs::write(
            dir.path().join("credentials.json"),
            serde_json::json!({"registries": {
                "https://soon.invalid": token(3), "https://gone.invalid": token(-2),
                "https://fine.invalid": token(30),
            }})
            .to_string(),
        )
        .unwrap();

        let health = credential_health(dir.path(), chrono::Utc::now());

        assert_eq!(health.failures, 1, "{health:?}");
        assert!(!health.signing_key_present);
        let levels: Vec<CredentialLevel> = health.lines.iter().map(|l| l.level).collect();
        // By root: fine, gone, soon.
        assert_eq!(
            levels,
            [
                CredentialLevel::Ok,
                CredentialLevel::Error,
                CredentialLevel::Warning
            ]
        );
        assert_eq!(
            health.lines[0].text,
            "registry 'https://fine.invalid': token ok"
        );
        assert!(health.lines[2].text.contains("re-login soon"), "{health:?}");

        let other = tempfile::TempDir::new().unwrap();
        std::fs::write(other.path().join("signing-key.json"), "{}").unwrap();
        let empty = credential_health(other.path(), chrono::Utc::now());
        assert!(empty.lines.is_empty() && empty.failures == 0);
        assert!(empty.signing_key_present);
    }

    #[specforge_test_macros::test(
        behavior = "run_doctor_check",
        verify = "an unreadable credential store is a credential failure in doctor"
    )]
    fn an_unreadable_store_is_a_credential_failure() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("credentials.json"),
            r#"{"registries": {"a": {"token": "t"},}}"#,
        )
        .unwrap();

        let health = credential_health(dir.path(), chrono::Utc::now());

        assert_eq!(health.failures, 1, "{health:?}");
        assert_eq!(health.lines.len(), 1, "{health:?}");
        assert_eq!(health.lines[0].level, CredentialLevel::Error);
        assert!(
            health.lines[0]
                .text
                .contains("invalid credentials file format"),
            "{health:?}"
        );
    }
}
