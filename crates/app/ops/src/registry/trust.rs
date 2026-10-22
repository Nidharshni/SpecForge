//! The trust decision (ADR 0213): how a registry package's publisher is accepted, decided once from what an
//! adapter learned, so the configured registry and the in-memory one cannot disagree.
//!
//! The adapter verifies the publisher signature (R-TRUST-002) and reads the user's known-keys store
//! (R-TRUST-006): crypto and a file, which ops never touches (ADR 0010). What it learned is plain data,
//! [`Signer`] and [`UserKeys`]; [`TrustPolicy::decide`] holds the rules over them.

use super::{Publisher, TrustPolicy};
use crate::asking::{ExpectedFrom, Question};
use crate::{OpError, OpErrorKind};
use specforge_common::codes;
use specforge_package::PackageName;

/// Who signed a package, as its adapter verified it over the digests the registry describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signer<'a> {
    /// No publisher signature.
    Unsigned,
    /// Signed by this key id; the signature verified (R-TRUST-002 is the adapter's, before this).
    Key(&'a str),
}

/// What the user's known-keys store says about one package name and one key. The adapter reads the
/// store (R-TRUST-006 when it can't); ops never does (ADR 0010).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UserKeys<'a> {
    /// The user's pin for the name.
    pub pin: Option<&'a str>,
    /// The key is on `trusted_keys`.
    pub trusted: bool,
    /// The key is on `denied_keys`.
    pub denied: bool,
}

impl TrustPolicy<'_> {
    /// The publisher of `name` signed by `signer`, decided once (ADR 0059, 0213); writes nothing.
    ///
    /// In order: the expected key, for every package, is the user's pin, else the key the lock records,
    /// else none (first use). An unsigned package is R-TRUST-007 when a key is expected, whatever
    /// `allow_unsigned`, `trusted_keys` or the consent (ADR 0221), else R-TRUST-001 unless
    /// `allow_unsigned`. A denied key is R-TRUST-005 whatever the consent. A key other than the expected
    /// one is accepted when it is on `trusted_keys`, else put to `asking`, and refused as R-TRUST-003 when
    /// nobody consents. So [`Publisher::Unsigned`] is answered only for a name with no expected key.
    ///
    /// Call it once the reply names `name@version` and after [`TrustPolicy::admits`] (R014 precedes any
    /// question, ADR 0201); pass what the user's store says about the name, whatever the signer. It asks at most once and pins nothing: [`Publisher::pin`] names what `Registry::pin`
    /// saves when the plan commits.
    pub fn decide(
        &self,
        name: &PackageName,
        signer: Signer<'_>,
        keys: UserKeys<'_>,
    ) -> Result<Publisher, OpError> {
        let expected = match (keys.pin, self.locked_key()) {
            (Some(pinned), _) => Some((pinned, ExpectedFrom::UserPin)),
            (None, Some(locked)) => Some((locked, ExpectedFrom::Lock)),
            (None, None) => None,
        };
        let key_id = match (signer, expected) {
            (Signer::Unsigned, Some((expected, from))) => {
                return Err(unsigned_for_an_expected_key(name, expected, from));
            }
            (Signer::Unsigned, None) if self.allow_unsigned => return Ok(Publisher::Unsigned),
            (Signer::Unsigned, None) => {
                return Err(refused(
                    codes::R_TRUST_001,
                    format!("package '{name}' is not signed"),
                    "if you trust its source, install it with `specforge add --allow-unsigned` (or \
                     `specforge update --allow-unsigned`) in a terminal"
                        .to_string(),
                ));
            }
            (Signer::Key(key_id), _) => key_id,
        };
        if keys.denied {
            return Err(refused(
                codes::R_TRUST_005,
                format!(
                    "publisher key '{key_id}' for '{name}' is denied in your known-keys config"
                ),
                "remove the key from denied_keys only if you trust it again".to_string(),
            ));
        }
        let signed = |first_use| Publisher::Signed {
            key_id: key_id.to_string(),
            first_use,
        };
        let Some((expected, from)) = expected else {
            return Ok(signed(true));
        };
        if expected == key_id {
            return Ok(signed(from == ExpectedFrom::Lock));
        }
        let consents = keys.trusted
            || self.asking.answers(&Question::KeyChange {
                package: name.as_str(),
                expected,
                offered: key_id,
                from,
            });
        if !consents {
            return Err(key_change_refused(
                name,
                expected,
                key_id,
                from,
                self.locked_key(),
            ));
        }
        Ok(Publisher::Repinned {
            key_id: key_id.to_string(),
            previous: expected.to_string(),
        })
    }
}

/// R-TRUST-007: `name` arrives unsigned, but `expected` is the key it is held to.
fn unsigned_for_an_expected_key(name: &PackageName, expected: &str, from: ExpectedFrom) -> OpError {
    let message = match from {
        ExpectedFrom::UserPin => {
            format!(
                "package '{name}' is not signed, but its publisher key is pinned to '{expected}'"
            )
        }
        ExpectedFrom::Lock => format!(
            "package '{name}' is not signed, but specforge.lock records its publisher key '{expected}'"
        ),
    };
    refused(
        codes::R_TRUST_007,
        message,
        format!(
            "a package its publisher signs does not arrive unsigned: check the registry that serves \
             '{name}'; if its publisher stopped signing it, remove the extension (`specforge remove \
             {name}`) and its pin in ~/.specforge/known-keys.json, then add it again"
        ),
    )
}

/// R-TRUST-003: `offered` is not the key expected for `name`.
fn key_change_refused(
    name: &PackageName,
    expected: &str,
    offered: &str,
    from: ExpectedFrom,
    locked: Option<&str>,
) -> OpError {
    let message = match from {
        ExpectedFrom::UserPin => {
            let also = if locked == Some(offered) {
                " (the key specforge.lock records)"
            } else {
                ""
            };
            format!(
                "key change rejected for '{name}': pinned '{expected}' but package is signed '{offered}'{also}"
            )
        }
        ExpectedFrom::Lock => format!(
            "key change rejected for '{name}': specforge.lock records '{expected}' but package is signed '{offered}'"
        ),
    };
    refused(
        codes::R_TRUST_003,
        message,
        format!(
            "if you trust the new key, accept it with `specforge add` or `specforge update` in a \
             terminal (`specforge add --yes` and `specforge update --yes` accept it without asking); \
             or add '{offered}' to trusted_keys"
        ),
    )
}

/// Every trust refusal is `permission_denied`.
fn refused(code: specforge_common::Code, message: String, suggestion: String) -> OpError {
    OpError::coded(OpErrorKind::PermissionDenied, code, message).with_suggestion(suggestion)
}

#[cfg(test)]
mod tests;
