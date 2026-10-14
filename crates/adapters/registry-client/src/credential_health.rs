//! Registry credential health, which `specforge doctor` reports: the
//! user's, not the project's.

use crate::credentials::{CredentialEntry, credentials_path, read_credentials};
use crate::signing::signing_key_path;
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
    /// One line per stored credential worth mentioning, by registry alias.
    pub lines: Vec<CredentialLine>,
    /// How many credentials are unusable (doctor then exits 1).
    pub failures: usize,
    /// Whether a publish signing key exists.
    pub signing_key_present: bool,
}

/// The user's credential health: the credential store and signing key
/// under `~/.specforge`.
pub fn user_credential_health() -> CredentialHealth {
    credential_health(&credentials_path(), &signing_key_path(), chrono::Utc::now())
}

/// The credential health of the store at `credentials` and the signing
/// key at `signing_key`, as of `now`.
pub fn credential_health(
    credentials: &Path,
    signing_key: &Path,
    now: chrono::DateTime<chrono::Utc>,
) -> CredentialHealth {
    let mut lines = Vec::new();
    let mut failures = 0usize;
    let store = read_credentials(credentials).unwrap_or_default();
    let mut aliases: Vec<String> = store.registries.keys().cloned().collect();
    aliases.sort();
    for alias in aliases {
        match store.credential(&alias) {
            // A reference resolves in the shell that runs a command, which need not be
            // doctor's: a variable not set here, or a file not readable here, is a warning.
            Err(diag) if diag.is(codes::R010) || diag.is(codes::R011) => {
                lines.push(CredentialLine {
                    level: CredentialLevel::Warning,
                    text: format!("registry '{alias}': {}", diag.message),
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
                }) = store.registries.get(&alias)
                else {
                    continue;
                };
                let line = match assess_expiry(Some(expires_at), now) {
                    Some(TokenExpiry::ExpiringSoon(days)) => CredentialLine {
                        level: CredentialLevel::Warning,
                        text: format!(
                            "registry '{alias}': token expires in {days} day(s) — re-login soon"
                        ),
                    },
                    // An expired token already failed `credential`.
                    Some(TokenExpiry::Valid) | Some(TokenExpiry::Expired) => CredentialLine {
                        level: CredentialLevel::Ok,
                        text: format!("registry '{alias}': token ok"),
                    },
                    None => CredentialLine {
                        level: CredentialLevel::Ok,
                        text: format!("registry '{alias}': token ok (expires {expires_at})"),
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
        signing_key_present: signing_key.exists(),
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
        let store = dir.path().join("credentials.json");
        std::fs::write(
            &store,
            r#"{"registries":{"ci":{"token_env":"P16_DOCTOR_UNSET_VARIABLE"}}}"#,
        )
        .unwrap();

        let health = credential_health(&store, &dir.path().join("no-key"), chrono::Utc::now());

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
        let store = dir.path().join("credentials.json");
        let token =
            |days: i64| serde_json::json!({"token": "t", "expires_at": at(days).to_rfc3339()});
        std::fs::write(
            &store,
            serde_json::json!({"registries": {
                "soon": token(3), "gone": token(-2), "fine": token(30),
            }})
            .to_string(),
        )
        .unwrap();

        let health = credential_health(&store, &dir.path().join("no-key"), chrono::Utc::now());

        assert_eq!(health.failures, 1, "{health:?}");
        assert!(!health.signing_key_present);
        let levels: Vec<CredentialLevel> = health.lines.iter().map(|l| l.level).collect();
        // By alias: fine, gone, soon.
        assert_eq!(
            levels,
            [
                CredentialLevel::Ok,
                CredentialLevel::Error,
                CredentialLevel::Warning
            ]
        );
        assert_eq!(health.lines[0].text, "registry 'fine': token ok");
        assert!(health.lines[2].text.contains("re-login soon"), "{health:?}");

        let empty = credential_health(&dir.path().join("none.json"), &store, chrono::Utc::now());
        assert!(empty.lines.is_empty() && empty.failures == 0);
        assert!(empty.signing_key_present);
    }
}
