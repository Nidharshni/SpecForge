//! Trust flow shared by `specforge add` and `specforge update` (spec #21, T2; ADR 0059).
//!
//! [`check`], per package, after sha256 integrity:
//! 1. verify the publisher signature offline (`verify_package_signature`)
//! 2. refuse unsigned packages unless `--allow-unsigned` (accepting one is W155)
//! 3. refuse a known-keys store that exists and can't be read (R-TRUST-006)
//! 4. refuse keys on the deny list (config-level revocation)
//! 5. TOFU: the key expected for the name is the user's pin, else the key the project's lock
//!    records; with neither, the key is accepted on first use. A mismatch is a key change, accepted
//!    when the key is on `trusted_keys` or when the caller's `decide` says so
//!
//! `check` writes nothing. What it accepted is data ([`Accepted`]); the pin the decision calls for is
//! saved by [`record`], once the install that fetched the package is written (W156 is reported
//! then).
//!
//! The flow writes nothing to the terminal and reads nothing from it: what it accepted is data
//! ([`Accepted`]), what a user should hear is a diagnostic ([`Trusted::diagnostics`] for W155,
//! [`record`]'s result for W156), and the question of a key change is the caller's `decide`.

use crate::trust::{KnownKeys, load_known_keys_at, save_known_keys_at};
use crate::{TrustCheck, verify_package_signature};
use specforge_registry_wire::PackageMetadata;
use std::path::Path;

use specforge_common::{Diagnostic, codes};

/// Where the key a signed package must match comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinSource {
    /// The user's pin in their known-keys store.
    User,
    /// The key the project's `specforge.lock` records for the package (the user has no pin).
    Lock,
}

/// A signed package whose key is not the one expected for its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChange {
    pub package: String,
    pub pinned: String,
    pub offered: String,
    pub source: PinSource,
}

impl KeyChange {
    /// R-TRUST-003 for this change. `locked` is the key the project's lock records for the package.
    pub fn refused(&self, locked: Option<&str>) -> Diagnostic {
        let message = match self.source {
            PinSource::User => {
                let mut message = format!(
                    "key change rejected for '{}': pinned '{}' but package is signed '{}'",
                    self.package, self.pinned, self.offered
                );
                if locked == Some(self.offered.as_str()) {
                    message.push_str(" (the key specforge.lock records)");
                }
                message
            }
            PinSource::Lock => format!(
                "key change rejected for '{}': specforge.lock records '{}' but package is signed '{}'",
                self.package, self.pinned, self.offered
            ),
        };
        Diagnostic::new(codes::R_TRUST_003, message).with_suggestion(format!(
            "if you trust the new key, re-run with --yes (or add '{}' to trusted_keys)",
            self.offered
        ))
    }
}

/// What the trust check accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accepted {
    /// No signature; `allow_unsigned`. Reported as W155.
    Unsigned,
    /// Signed by `key_id`; `first_use` when the user has no pin for the name (the key is pinned when
    /// the install is written).
    Signed { key_id: String, first_use: bool },
    /// Signed by `key_id`, replacing the expected `previous` with consent (`trusted_keys`, or
    /// `decide`). Pinned, and reported as W156, when the install is written.
    Repinned { key_id: String, previous: String },
}

/// The checked package's trust, and what it reports (W155).
#[derive(Debug)]
pub struct Trusted {
    pub accepted: Accepted,
    pub diagnostics: Vec<Diagnostic>,
}

/// Verify `metadata`'s publisher signature over `wasm_bytes`, then decide the publisher against the
/// user's store at `known_keys` (pins, `trusted_keys`, `denied_keys`) and `locked`, the key the
/// project's lock records for the name. The expected key is the user's pin, else `locked`; with
/// neither, the key is accepted on first use. A key change is accepted when the key is on
/// `trusted_keys`, else when `decide` says so; refused with R-TRUST-003 otherwise. A denied key is
/// R-TRUST-005. A store that exists and can't be read is R-TRUST-006. **Writes nothing**; the pin
/// the decision calls for is [`record`]ed by the caller once the install is written.
///
/// `known_keys` is the file the user's pins live in (the user's `known-keys.json`): the caller says
/// whose, so a test never reads or writes the real one.
pub fn check(
    name: &str,
    metadata: &PackageMetadata,
    wasm_bytes: &[u8],
    allow_unsigned: bool,
    decide: &dyn Fn(&KeyChange) -> bool,
    known_keys: &Path,
    locked: Option<&str>,
) -> Result<Trusted, Diagnostic> {
    match verify_package_signature(metadata, wasm_bytes)? {
        TrustCheck::Unsigned => {
            if !allow_unsigned {
                return Err(Diagnostic::new(
                    codes::R_TRUST_001,
                    format!("package '{name}' is not signed"),
                )
                .with_suggestion("re-run with --allow-unsigned to accept the risk".to_string()));
            }
            Ok(Trusted {
                accepted: Accepted::Unsigned,
                diagnostics: vec![
                    Diagnostic::new(
                        codes::W155,
                        format!(
                            "{name} {} is not signed, and was accepted because --allow-unsigned was given",
                            metadata.version
                        ),
                    )
                    .with_suggestion(
                        "ask the publisher to sign it (specforge publish signs every package)"
                            .to_string(),
                    ),
                ],
            })
        }
        TrustCheck::Verified { key_id } => {
            let known =
                load_known_keys_at(known_keys).map_err(|why| unusable_store(known_keys, &why))?;

            // Config-level revocation wins over everything.
            if known.is_denied(&key_id) {
                return Err(denied(name, &key_id));
            }

            let expected = known
                .pin_for(name)
                .map(|key| (key.to_string(), PinSource::User))
                .or_else(|| locked.map(|key| (key.to_string(), PinSource::Lock)));
            let accepted = match expected {
                None => Accepted::Signed {
                    key_id,
                    first_use: true,
                },
                Some((pinned, source)) if pinned == key_id => Accepted::Signed {
                    key_id,
                    first_use: source == PinSource::Lock,
                },
                // Key change: the expected key and the new signature disagree.
                Some((pinned, source)) => {
                    let change = KeyChange {
                        package: name.to_string(),
                        pinned: pinned.clone(),
                        offered: key_id.clone(),
                        source,
                    };
                    if !known.is_trusted(&key_id) && !decide(&change) {
                        return Err(change.refused(locked));
                    }
                    Accepted::Repinned {
                        key_id,
                        previous: pinned,
                    }
                }
            };
            Ok(Trusted {
                accepted,
                diagnostics: Vec::new(),
            })
        }
    }
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

fn denied(name: &str, key_id: &str) -> Diagnostic {
    Diagnostic::new(
        codes::R_TRUST_005,
        format!("publisher key '{key_id}' for '{name}' is denied in your known-keys config"),
    )
    .with_suggestion("remove the key from denied_keys only if you trust it again".to_string())
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

    fn signed_response(key: &SigningKey, manifest_json: &str, wasm: &[u8]) -> PackageMetadata {
        use chrono::TimeZone;
        use specforge_registry_wire::{SignedContent, sha256_hex};
        let sig = key.sign_at(
            &SignedContent::of("@acme/tool", "1.0.0", wasm, manifest_json),
            chrono::Utc.with_ymd_and_hms(2026, 9, 24, 0, 0, 0).unwrap(),
        );
        PackageMetadata {
            name: "@acme/tool".to_string(),
            version: "1.0.0".to_string(),
            wasm_url: String::new(),
            sha256: sha256_hex(wasm),
            signature: Some(sig),
            manifest: manifest_json.to_string(),
            ..Default::default()
        }
    }

    fn unsigned_response() -> PackageMetadata {
        PackageMetadata {
            name: "@acme/tool".to_string(),
            version: "1.0.0".to_string(),
            wasm_url: String::new(),
            sha256: "x".to_string(),
            signature: None,
            manifest: String::new(),
            ..Default::default()
        }
    }

    const MANIFEST: &str = r#"{"name":"@acme/tool","version":"1.0.0"}"#;
    const WASM: &[u8] = b"\0asm-bytes";
    /// An unsigned package reads and writes no store; the path is never touched.
    fn no_store() -> &'static Path {
        Path::new("unused-known-keys.json")
    }

    fn never(_: &KeyChange) -> bool {
        false
    }

    fn always(_: &KeyChange) -> bool {
        true
    }

    /// `check` with no key locked for the name.
    fn check_unlocked(
        metadata: &PackageMetadata,
        wasm: &[u8],
        allow_unsigned: bool,
        decide: &dyn Fn(&KeyChange) -> bool,
        store: &Path,
    ) -> Result<Trusted, Diagnostic> {
        check(
            "@acme/tool",
            metadata,
            wasm,
            allow_unsigned,
            decide,
            store,
            None,
        )
    }

    #[test]
    fn unsigned_package_is_refused_without_flag() {
        let err =
            check_unlocked(&unsigned_response(), WASM, false, &always, no_store()).unwrap_err();
        assert_eq!(err.code, "R-TRUST-001");
        assert!(
            err.suggestion
                .unwrap_or_default()
                .contains("--allow-unsigned")
        );
    }

    #[test]
    fn unsigned_package_passes_with_flag_and_no_pin_and_is_w155() {
        let trusted = check_unlocked(&unsigned_response(), WASM, true, &never, no_store()).unwrap();
        assert_eq!(trusted.accepted, Accepted::Unsigned);
        assert_eq!(trusted.diagnostics.len(), 1);
        assert_eq!(trusted.diagnostics[0].code, "W155");
        assert!(trusted.diagnostics[0].message.contains("@acme/tool 1.0.0"));
    }

    #[test]
    fn a_first_use_is_accepted_and_record_pins_it() {
        let key = SigningKey::generate();
        let response = signed_response(&key, MANIFEST, WASM);
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");

        let trusted = check_unlocked(&response, WASM, false, &never, &store).unwrap();
        assert_eq!(
            trusted.accepted,
            Accepted::Signed {
                key_id: key.key_id(),
                first_use: true
            }
        );
        assert!(trusted.diagnostics.is_empty());
        assert!(!store.exists(), "the check writes nothing");

        let reported = record(
            &[NewPin {
                package: "@acme/tool",
                key_id: &key.key_id(),
                replaces: None,
            }],
            &store,
        )
        .unwrap();
        assert!(reported.is_empty(), "a first-use pin is not reported");
        let known = load_known_keys_at(&store).unwrap();
        assert_eq!(known.pin_for("@acme/tool"), Some(key.key_id().as_str()));
    }

    #[test]
    fn matching_pin_accepts_without_asking() {
        let key = SigningKey::generate();
        let response = signed_response(&key, MANIFEST, WASM);
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");
        let mut known = KnownKeys::default();
        known.pin("@acme/tool", &key.key_id());
        save_known_keys_at(&store, &known).unwrap();

        // An install of the pinned package/key: accepted, pin unchanged.
        let trusted = check_unlocked(&response, WASM, false, &never, &store).unwrap();
        assert_eq!(
            trusted.accepted,
            Accepted::Signed {
                key_id: key.key_id(),
                first_use: false
            }
        );
        assert_eq!(load_known_keys_at(&store).unwrap(), known);
    }

    #[test]
    fn a_key_change_is_decided_by_decide_and_record_reports_w156() {
        let key_a = SigningKey::generate();
        let key_b = SigningKey::generate();
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");
        let mut known = KnownKeys::default();
        known.pin("@acme/tool", &key_a.key_id());
        save_known_keys_at(&store, &known).unwrap();

        // Different key signs the same package: refused when nobody consents.
        let second = signed_response(&key_b, MANIFEST, WASM);
        let err = check_unlocked(&second, WASM, false, &never, &store).unwrap_err();
        assert_eq!(err.code, "R-TRUST-003");

        // The asker sees both keys; yes accepts. The store still holds the old pin.
        let asked = |change: &KeyChange| {
            assert_eq!(change.package, "@acme/tool");
            assert_eq!(change.pinned, key_a.key_id());
            assert_eq!(change.offered, key_b.key_id());
            assert_eq!(change.source, PinSource::User);
            true
        };
        let trusted = check_unlocked(&second, WASM, false, &asked, &store).unwrap();
        assert_eq!(
            trusted.accepted,
            Accepted::Repinned {
                key_id: key_b.key_id(),
                previous: key_a.key_id()
            }
        );
        assert!(trusted.diagnostics.is_empty());
        let known = load_known_keys_at(&store).unwrap();
        assert_eq!(known.pin_for("@acme/tool"), Some(key_a.key_id().as_str()));

        // The caller records it once the install is written: one W156 naming both keys.
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
        let known = load_known_keys_at(&store).unwrap();
        assert_eq!(known.pin_for("@acme/tool"), Some(key_b.key_id().as_str()));
    }

    #[test]
    fn trusted_key_is_accepted_despite_missing_pin() {
        let key = SigningKey::generate();
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");

        // Pre-seed the allowlist with this key.
        let mut known = KnownKeys::default();
        known.trusted_keys.push(key.key_id());
        save_known_keys_at(&store, &known).unwrap();

        let response = signed_response(&key, MANIFEST, WASM);
        let trusted = check_unlocked(&response, WASM, false, &never, &store).unwrap();
        assert!(matches!(trusted.accepted, Accepted::Signed { .. }));
    }

    #[test]
    fn denied_key_is_refused_even_if_pinned() {
        let key = SigningKey::generate();
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");

        let mut known = KnownKeys::default();
        known.pins.insert("@acme/tool".to_string(), key.key_id());
        known.denied_keys.push(key.key_id());
        save_known_keys_at(&store, &known).unwrap();

        let response = signed_response(&key, MANIFEST, WASM);
        let err = check_unlocked(&response, WASM, false, &always, &store).unwrap_err();
        assert_eq!(err.code, "R-TRUST-005");
    }

    #[test]
    fn tampered_wasm_is_refused_even_with_valid_pin() {
        let key = SigningKey::generate();
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");

        let response = signed_response(&key, MANIFEST, WASM);
        let tampered: &[u8] = b"\0asm-evil";

        // No --allow-unsigned escape for broken signatures.
        let err = check_unlocked(&response, tampered, true, &always, &store).unwrap_err();
        assert_eq!(err.code, "R-TRUST-002");
    }

    /// `check` with `locked` as the key the project's lock records for the name.
    fn check_locked(
        metadata: &PackageMetadata,
        decide: &dyn Fn(&KeyChange) -> bool,
        store: &Path,
        locked: &str,
    ) -> Result<Trusted, Diagnostic> {
        check(
            "@acme/tool",
            metadata,
            WASM,
            false,
            decide,
            store,
            Some(locked),
        )
    }

    #[test]
    fn with_no_pin_the_locked_key_is_expected() {
        let (locked, other) = (SigningKey::generate(), SigningKey::generate());
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");

        // The locked key is accepted; it is still pinned for the user when the install is written.
        let trusted = check_locked(
            &signed_response(&locked, MANIFEST, WASM),
            &never,
            &store,
            &locked.key_id(),
        )
        .unwrap();
        assert_eq!(
            trusted.accepted,
            Accepted::Signed {
                key_id: locked.key_id(),
                first_use: true
            }
        );

        // Another key is a key change, from the lock.
        let err = check_locked(
            &signed_response(&other, MANIFEST, WASM),
            &never,
            &store,
            &locked.key_id(),
        )
        .unwrap_err();
        assert_eq!(err.code, "R-TRUST-003");
        let seen = |change: &KeyChange| {
            assert_eq!(change.source, PinSource::Lock);
            assert_eq!(change.pinned, locked.key_id());
            true
        };
        let trusted = check_locked(
            &signed_response(&other, MANIFEST, WASM),
            &seen,
            &store,
            &locked.key_id(),
        )
        .unwrap();
        assert_eq!(
            trusted.accepted,
            Accepted::Repinned {
                key_id: other.key_id(),
                previous: locked.key_id()
            }
        );
    }

    #[test]
    fn a_users_pin_wins_over_the_locked_key() {
        let (pinned, locked) = (SigningKey::generate(), SigningKey::generate());
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");
        let mut known = KnownKeys::default();
        known.pin("@acme/tool", &pinned.key_id());
        save_known_keys_at(&store, &known).unwrap();

        // Signed by the user's pin: accepted as pinned, whatever the lock says.
        let trusted = check_locked(
            &signed_response(&pinned, MANIFEST, WASM),
            &never,
            &store,
            &locked.key_id(),
        )
        .unwrap();
        assert_eq!(
            trusted.accepted,
            Accepted::Signed {
                key_id: pinned.key_id(),
                first_use: false
            }
        );
        // Signed by the lock's key: a change from the user's pin.
        let err = check_locked(
            &signed_response(&locked, MANIFEST, WASM),
            &never,
            &store,
            &locked.key_id(),
        )
        .unwrap_err();
        assert_eq!(err.code, "R-TRUST-003");
        assert!(
            err.message.ends_with("(the key specforge.lock records)"),
            "{}",
            err.message
        );
    }

    #[test]
    fn a_change_from_the_locked_key_names_specforge_lock() {
        let (locked, other) = (SigningKey::generate(), SigningKey::generate());
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");

        let err = check_locked(
            &signed_response(&other, MANIFEST, WASM),
            &never,
            &store,
            &locked.key_id(),
        )
        .unwrap_err();

        assert_eq!(
            err.message,
            format!(
                "key change rejected for '@acme/tool': specforge.lock records '{}' but package is signed '{}'",
                locked.key_id(),
                other.key_id()
            )
        );
    }

    #[test]
    fn a_store_that_cannot_be_read_refuses_and_is_left_as_it_was() {
        let key = SigningKey::generate();
        let response = signed_response(&key, MANIFEST, WASM);
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");
        let broken = format!(r#"{{"denied_keys":["{}"],}}"#, key.key_id());
        std::fs::write(&store, &broken).unwrap();

        let err = check_unlocked(&response, WASM, false, &always, &store).unwrap_err();
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
