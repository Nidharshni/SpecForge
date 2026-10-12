//! Trust flow shared by `specforge add` and `specforge update` (spec #21, T2).
//!
//! Sequence per package, after sha256 integrity:
//! 1. verify the publisher signature offline (`verify_package_signature`)
//! 2. refuse unsigned packages unless `--allow-unsigned` (accepting one is W155)
//! 3. refuse keys on the deny list (config-level revocation)
//! 4. TOFU: pin on first install; on later installs require a match —
//!    a mismatch is a key change, accepted when the key is on `trusted_keys`
//!    or when the caller's `decide` says so (accepting one is W156)
//! 5. operator allowlist (`trusted_keys`) accepts a key without a prior pin
//!    and re-pins it
//!
//! The flow writes nothing to the terminal and reads nothing from it: what it
//! accepted is data ([`Accepted`]), what a user should hear is a diagnostic
//! ([`Trusted::diagnostics`]), and the question of a key change is the
//! caller's `decide`.

use crate::{KnownKeys, TrustCheck, verify_package_signature};
use specforge_registry_wire::PackageMetadata;
use std::path::Path;

use specforge_common::{Diagnostic, codes};

/// A signed package whose key is not the one pinned for its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChange {
    pub package: String,
    pub pinned: String,
    pub offered: String,
}

/// What the trust check accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accepted {
    /// No signature; `allow_unsigned`. Reported as W155.
    Unsigned,
    /// Signed by `key_id`; `pinned_now` when no pin existed and this check made it (trust on first use).
    Signed { key_id: String, pinned_now: bool },
    /// Signed by `key_id`, re-pinned from `previous` with consent (`trusted_keys`, or `decide`). Reported
    /// as W156.
    Repinned { key_id: String, previous: String },
}

/// The checked package's trust, and what it reports (W155, W156).
#[derive(Debug)]
pub struct Trusted {
    pub accepted: Accepted,
    pub diagnostics: Vec<Diagnostic>,
}

/// Verify `metadata`'s publisher signature over `wasm`, then apply the pin store at `known_keys` (trust on
/// first use). A key change is accepted when the key is on `trusted_keys`, else when `decide` says so;
/// refused with R-TRUST-003 otherwise. Writes nothing but the pin store.
///
/// `known_keys` is the file the user's pins live in (the user's `known-keys.json`): the caller says
/// whose, so a test never reads or writes the real one.
pub fn check_and_pin(
    name: &str,
    metadata: &PackageMetadata,
    wasm_bytes: &[u8],
    allow_unsigned: bool,
    decide: &dyn Fn(&KeyChange) -> bool,
    known_keys: &Path,
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
            let mut known = load_known_keys_at(known_keys);

            // Config-level revocation wins over everything.
            if known.is_denied(&key_id) {
                return Err(Diagnostic::new(
                    codes::R_TRUST_005,
                    format!(
                        "publisher key '{}' for '{}' is denied in your known-keys config",
                        key_id, name
                    ),
                )
                .with_suggestion(
                    "remove the key from denied_keys only if you trust it again".to_string(),
                ));
            }

            // Own the pin so the mutable re-pin below doesn't conflict with
            // the borrow the match arms would otherwise hold.
            let existing_pin = known.pin_for(name).map(str::to_string);
            match existing_pin {
                None => {
                    known.pin(name, &key_id);
                    save(&known, known_keys)?;
                    Ok(Trusted {
                        accepted: Accepted::Signed {
                            key_id,
                            pinned_now: true,
                        },
                        diagnostics: Vec::new(),
                    })
                }
                Some(pinned) if pinned == key_id => Ok(Trusted {
                    accepted: Accepted::Signed {
                        key_id,
                        pinned_now: false,
                    },
                    diagnostics: Vec::new(),
                }),
                // Key change: the pin and the new signature disagree.
                Some(pinned) => {
                    let change = KeyChange {
                        package: name.to_string(),
                        pinned: pinned.clone(),
                        offered: key_id.clone(),
                    };
                    if !known.is_trusted(&key_id) && !decide(&change) {
                        return Err(Diagnostic::new(
                            codes::R_TRUST_003,
                            format!(
                                "key change rejected for '{}': pinned '{}' but package is signed '{}'",
                                name, pinned, key_id
                            ),
                        )
                        .with_suggestion(format!(
                            "if you trust the new key, re-run with --yes (or add '{}' to trusted_keys)",
                            key_id
                        )));
                    }
                    known.pin(name, &key_id);
                    save(&known, known_keys)?;
                    let diagnostic = Diagnostic::new(
                        codes::W156,
                        format!(
                            "the publisher key of {name} changed from {pinned} to {key_id}, and the new key is now pinned"
                        ),
                    )
                    .with_suggestion(
                        "confirm the change with the publisher if you did not expect it"
                            .to_string(),
                    );
                    Ok(Trusted {
                        accepted: Accepted::Repinned {
                            key_id,
                            previous: pinned,
                        },
                        diagnostics: vec![diagnostic],
                    })
                }
            }
        }
    }
}

fn save(known: &KnownKeys, path: &Path) -> Result<(), Diagnostic> {
    save_known_keys_at(path, known).map_err(|message| {
        Diagnostic::new(codes::R_TRUST_006, message)
            .with_suggestion("check permissions on the file".to_string())
    })
}

// Re-exported so callers can hit the same store paths in tests.
pub use super::trust::{load_known_keys_at, save_known_keys_at};

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

    #[test]
    fn unsigned_package_is_refused_without_flag() {
        let err = check_and_pin(
            "@acme/tool",
            &unsigned_response(),
            WASM,
            false,
            &always,
            no_store(),
        )
        .unwrap_err();
        assert_eq!(err.code, "R-TRUST-001");
        assert!(
            err.suggestion
                .unwrap_or_default()
                .contains("--allow-unsigned")
        );
    }

    #[test]
    fn unsigned_package_passes_with_flag_and_no_pin_and_is_w155() {
        let trusted = check_and_pin(
            "@acme/tool",
            &unsigned_response(),
            WASM,
            true,
            &never,
            no_store(),
        )
        .unwrap();
        assert_eq!(trusted.accepted, Accepted::Unsigned);
        assert_eq!(trusted.diagnostics.len(), 1);
        assert_eq!(trusted.diagnostics[0].code, "W155");
        assert!(trusted.diagnostics[0].message.contains("@acme/tool 1.0.0"));
    }

    #[test]
    fn first_verified_install_pins_the_key() {
        let key = SigningKey::generate();
        let response = signed_response(&key, MANIFEST, WASM);
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");

        let trusted = check_and_pin("@acme/tool", &response, WASM, false, &never, &store).unwrap();
        assert_eq!(
            trusted.accepted,
            Accepted::Signed {
                key_id: key.key_id(),
                pinned_now: true
            }
        );
        assert!(trusted.diagnostics.is_empty());

        let known = load_known_keys_at(&store);
        assert_eq!(known.pin_for("@acme/tool"), Some(key.key_id().as_str()));
    }

    #[test]
    fn matching_pin_accepts_without_asking() {
        let key = SigningKey::generate();
        let response = signed_response(&key, MANIFEST, WASM);
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");

        check_and_pin("@acme/tool", &response, WASM, false, &never, &store).unwrap();
        // Second install of the same package/key: accepted, pin unchanged.
        let trusted = check_and_pin("@acme/tool", &response, WASM, false, &never, &store).unwrap();
        assert_eq!(
            trusted.accepted,
            Accepted::Signed {
                key_id: key.key_id(),
                pinned_now: false
            }
        );
    }

    #[test]
    fn a_key_change_is_decided_by_decide_and_reported_as_w156() {
        let key_a = SigningKey::generate();
        let key_b = SigningKey::generate();
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("known-keys.json");

        let first = signed_response(&key_a, MANIFEST, WASM);
        check_and_pin("@acme/tool", &first, WASM, false, &never, &store).unwrap();

        // Different key signs the same package: refused when nobody consents.
        let second = signed_response(&key_b, MANIFEST, WASM);
        let err = check_and_pin("@acme/tool", &second, WASM, false, &never, &store).unwrap_err();
        assert_eq!(err.code, "R-TRUST-003");

        // The asker sees both keys; yes accepts and re-pins.
        let asked = |change: &KeyChange| {
            assert_eq!(change.package, "@acme/tool");
            assert_eq!(change.pinned, key_a.key_id());
            assert_eq!(change.offered, key_b.key_id());
            true
        };
        let trusted = check_and_pin("@acme/tool", &second, WASM, false, &asked, &store).unwrap();
        assert_eq!(
            trusted.accepted,
            Accepted::Repinned {
                key_id: key_b.key_id(),
                previous: key_a.key_id()
            }
        );
        assert_eq!(trusted.diagnostics.len(), 1);
        assert_eq!(trusted.diagnostics[0].code, "W156");
        assert!(trusted.diagnostics[0].message.contains(&key_a.key_id()));
        assert!(trusted.diagnostics[0].message.contains(&key_b.key_id()));
        let known = load_known_keys_at(&store);
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
        let trusted = check_and_pin("@acme/tool", &response, WASM, false, &never, &store).unwrap();
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
        let err = check_and_pin("@acme/tool", &response, WASM, false, &always, &store).unwrap_err();
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
        let err =
            check_and_pin("@acme/tool", &response, tampered, true, &always, &store).unwrap_err();
        assert_eq!(err.code, "R-TRUST-002");
    }
}
