//! The user's known-keys store, shared by `specforge add` and `specforge update` (spec #21; ADR 0059, 0213).
//!
//! The store is read ([`keys`], R-TRUST-006 when it can't be) and written ([`record`], W156 when a pin
//! replaces another); the decision over what it says is `specforge_ops::registry::TrustPolicy::decide`
//! (R-TRUST-001, -003, -005), which both registry adapters ask. The pin a decision calls for is saved by
//! [`record`] only once the install that fetched the package is written.
//!
//! Nothing here writes to the terminal or reads from it: what a user should hear is a diagnostic
//! ([`unsigned_accepted`] for W155, [`record`]'s result for W156).

use crate::trust::{KnownKeys, load_known_keys_at, save_known_keys_at};
use std::path::Path;

use specforge_common::{Diagnostic, codes};

/// The store at `path`, read (R-TRUST-006 when it exists and can't be). Every registry package needs it,
/// signed or not: its pin holds an unsigned package too (ADR 0221).
pub fn keys(path: &Path) -> Result<KnownKeys, Diagnostic> {
    load_known_keys_at(path).map_err(|why| unusable_store(path, &why))
}

/// W155: `name` `version` is not signed, and was accepted because unsigned packages were allowed.
pub fn unsigned_accepted(name: &str, version: &str) -> Diagnostic {
    Diagnostic::new(
        codes::W155,
        format!("{name} {version} is not signed, and was accepted because unsigned packages were allowed"),
    )
    .with_suggestion(
        "ask the publisher to sign it (specforge publish signs every package)".to_string(),
    )
}

/// One pin to save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewPin<'a> {
    pub package: &'a str,
    pub key_id: &'a str,
    /// The key it replaces after consent (reported as W156); `None` on first use.
    pub replaces: Option<&'a str>,
}

/// Save `pins` in the store at `known_keys`, all at once (the file is replaced whole, atomically).
/// R-TRUST-006 when the store can't be read or saved, and then nothing is saved. Returns a W156 for
/// each pin that replaces another, in order. No pins: nothing is read or written.
pub fn record(pins: &[NewPin<'_>], known_keys: &Path) -> Result<Vec<Diagnostic>, Diagnostic> {
    if pins.is_empty() {
        return Ok(Vec::new());
    }
    let mut known =
        load_known_keys_at(known_keys).map_err(|why| unusable_store(known_keys, &why))?;
    let mut reported = Vec::new();
    for pin in pins {
        known.pin(pin.package, pin.key_id);
        if let Some(previous) = pin.replaces {
            reported.push(
                Diagnostic::new(
                    codes::W156,
                    format!(
                        "the publisher key of {} changed from {previous} to {}, and the new key is now pinned",
                        pin.package, pin.key_id
                    ),
                )
                .with_suggestion(
                    "confirm the change with the publisher if you did not expect it".to_string(),
                ),
            );
        }
    }
    save(&known, known_keys)?;
    Ok(reported)
}

fn unusable_store(path: &Path, why: &str) -> Diagnostic {
    Diagnostic::new(
        codes::R_TRUST_006,
        format!(
            "the known-keys store {} can't be read: {why}",
            path.display()
        ),
    )
    .with_suggestion(
        "fix its JSON, or move it aside (every package is then trusted on first use again)"
            .to_string(),
    )
}

fn save(known: &KnownKeys, path: &Path) -> Result<(), Diagnostic> {
    save_known_keys_at(path, known).map_err(|message| {
        Diagnostic::new(codes::R_TRUST_006, message)
            .with_suggestion("check permissions on the file".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SigningKey;

    #[test]
    fn an_unsigned_package_accepted_is_w155() {
        let w155 = unsigned_accepted("@acme/tool", "1.0.0");
        assert_eq!(w155.code, "W155");
        assert!(w155.message.contains("@acme/tool 1.0.0"));
    }

    #[test]
    fn record_pins_a_first_use_silently_and_reports_a_repin_as_w156() {
        let (key_a, key_b) = (SigningKey::generate(), SigningKey::generate());
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");

        let reported = record(
            &[NewPin {
                package: "@acme/tool",
                key_id: &key_a.key_id(),
                replaces: None,
            }],
            &store,
        )
        .unwrap();
        assert!(reported.is_empty(), "a first-use pin is not reported");
        assert_eq!(
            keys(&store).unwrap().pin_for("@acme/tool"),
            Some(key_a.key_id().as_str())
        );

        // One W156 naming both keys.
        let reported = record(
            &[NewPin {
                package: "@acme/tool",
                key_id: &key_b.key_id(),
                replaces: Some(&key_a.key_id()),
            }],
            &store,
        )
        .unwrap();
        assert_eq!(reported.len(), 1);
        assert_eq!(reported[0].code, "W156");
        assert!(reported[0].message.contains(&key_a.key_id()));
        assert!(reported[0].message.contains(&key_b.key_id()));
        assert_eq!(
            keys(&store).unwrap().pin_for("@acme/tool"),
            Some(key_b.key_id().as_str())
        );
    }

    #[test]
    fn a_store_that_cannot_be_read_refuses_and_is_left_as_it_was() {
        let key = SigningKey::generate();
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");
        let broken = format!(r#"{{"denied_keys":["{}"],}}"#, key.key_id());
        std::fs::write(&store, &broken).unwrap();

        let err = keys(&store).unwrap_err();
        assert_eq!(err.code, "R-TRUST-006");
        assert!(err.message.contains("can't be read"), "{}", err.message);
        assert!(
            err.message.contains(&store.display().to_string()),
            "{}",
            err.message
        );

        let pin = NewPin {
            package: "@acme/tool",
            key_id: &key.key_id(),
            replaces: None,
        };
        let err = record(&[pin], &store).unwrap_err();
        assert_eq!(err.code, "R-TRUST-006");
        assert_eq!(std::fs::read_to_string(&store).unwrap(), broken);
    }

    #[test]
    fn record_saves_every_pin_at_once_or_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");
        let pins = [
            NewPin {
                package: "@acme/a",
                key_id: "k1",
                replaces: None,
            },
            NewPin {
                package: "@acme/b",
                key_id: "k2",
                replaces: Some("k0"),
            },
        ];

        let reported = record(&pins, &store).unwrap();
        assert_eq!(reported.len(), 1, "only the re-pin is reported");
        let known = load_known_keys_at(&store).unwrap();
        assert_eq!(known.pin_for("@acme/a"), Some("k1"));
        assert_eq!(known.pin_for("@acme/b"), Some("k2"));

        // A store whose directory can't exist: nothing is saved.
        let file = dir.path().join("a-file");
        std::fs::write(&file, "x").unwrap();
        let unsavable = file.join("known-keys.json");
        let err = record(&pins, &unsavable).unwrap_err();
        assert_eq!(err.code, "R-TRUST-006");
        assert!(!unsavable.exists());
    }

    #[test]
    fn no_pins_read_and_write_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");
        std::fs::write(&store, "not json").unwrap();

        assert!(record(&[], &store).unwrap().is_empty());
        assert_eq!(std::fs::read_to_string(&store).unwrap(), "not json");
        assert!(record(&[], Path::new("unused.json")).unwrap().is_empty());
    }
}
