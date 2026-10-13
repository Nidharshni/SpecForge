//! Which codes an extension may report. Extensions state their codes as
//! text (a rule descriptor's `code`, a pass diagnostic's `code`), so the host
//! checks them where they enter ([`check_extension_code`]); a misuse is
//! W150, and the rule or finding is kept as the extension gave it.

use crate::{Level, lookup, retired};

/// Whether extension `extension` (its declared name) may report `code` at
/// `level` (`Error`, `Warning` or `Info`). The catalog decides:
///
/// - a catalogued code belongs to its owner, who reports it at its
///   catalogued level (at any level for an `A###` code, whose pass sets
///   it); core or another extension reporting it is
///   [`CodeMisuse::NotItsCode`], and its owner reporting it at another
///   level is [`CodeMisuse::WrongLevel`];
/// - a retired code is nobody's ([`CodeMisuse::Retired`]);
/// - a first-party extension (`@specforge/...`) reports only catalogued
///   codes ([`CodeMisuse::Uncatalogued`]);
/// - any other extension reports `E900`-`E998`, `W900`-`W998` and
///   `I900`-`I998` ([`CodeMisuse::OutsideThirdPartyRange`]), at the level
///   its prefix states ([`CodeMisuse::PrefixContradictsLevel`]).
pub fn check_extension_code(extension: &str, code: &str, level: Level) -> Result<(), CodeMisuse> {
    if let Some(entry) = lookup(code) {
        if entry.owner != extension {
            return Err(CodeMisuse::NotItsCode {
                owner: entry.owner,
                title: entry.title,
            });
        }
        return match entry.level {
            Level::SetByPass => Ok(()),
            catalogued if catalogued == level => Ok(()),
            catalogued => Err(CodeMisuse::WrongLevel { catalogued }),
        };
    }
    if let Some(replaced_by) = retired(code) {
        return Err(CodeMisuse::Retired { replaced_by });
    }
    if extension.starts_with(FIRST_PARTY) {
        return Err(CodeMisuse::Uncatalogued);
    }
    match third_party_prefix(code) {
        None => Err(CodeMisuse::OutsideThirdPartyRange),
        Some(prefix) if prefix_level(prefix) == level => Ok(()),
        Some(_) => Err(CodeMisuse::PrefixContradictsLevel),
    }
}

/// The scope of the extensions the project ships and documents.
const FIRST_PARTY: &str = "@specforge/";

/// The prefix of a third-party code (`E900`-`E998`, `W900`-`W998`,
/// `I900`-`I998`), when `code` is one.
fn third_party_prefix(code: &str) -> Option<u8> {
    let [prefix @ (b'E' | b'W' | b'I'), digits @ ..] = code.as_bytes() else {
        return None;
    };
    let in_range = digits.len() == 3
        && digits.iter().all(u8::is_ascii_digit)
        && digits >= b"900".as_slice()
        && digits <= b"998".as_slice();
    in_range.then_some(*prefix)
}

fn prefix_level(prefix: u8) -> Level {
    match prefix {
        b'E' => Level::Error,
        b'W' => Level::Warning,
        _ => Level::Info,
    }
}

/// Why an extension may not report a code. `Display` is the reason W150
/// gives, a noun phrase: `a code core owns ("Parse error"); an extension's
/// own codes are ...`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeMisuse {
    /// The catalog gives the code to another owner (`"core"` or an
    /// extension).
    NotItsCode {
        owner: &'static str,
        title: &'static str,
    },
    /// The extension's own code, reported at another level than the
    /// catalog's.
    WrongLevel { catalogued: Level },
    /// A retired code (never reused for another meaning).
    Retired { replaced_by: Option<&'static str> },
    /// A first-party extension's code the catalog does not list.
    Uncatalogued,
    /// A third-party code outside `E900`-`E998` / `W900`-`W998` /
    /// `I900`-`I998`.
    OutsideThirdPartyRange,
    /// A third-party code whose `E`/`W`/`I` prefix contradicts the level
    /// reported.
    PrefixContradictsLevel,
}

/// The third-party ranges, as W150 states them.
const OWN_RANGE: &str =
    "an extension's own codes are E900\u{2013}E998, W900\u{2013}W998 and I900\u{2013}I998";

impl std::fmt::Display for CodeMisuse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodeMisuse::NotItsCode { owner, title } => {
                write!(f, "a code {owner} owns (\"{title}\"); {OWN_RANGE}")
            }
            CodeMisuse::WrongLevel { catalogued } => write!(
                f,
                "its own code, reported at another level than the catalog's ({})",
                catalogued.describe()
            ),
            CodeMisuse::Retired { replaced_by: None } => {
                write!(f, "a retired code, which is never reused")
            }
            CodeMisuse::Retired {
                replaced_by: Some(new),
            } => write!(f, "a retired code (replaced by {new})"),
            CodeMisuse::Uncatalogued => write!(
                f,
                "a code the catalog does not list; a first-party extension's codes are catalogued"
            ),
            CodeMisuse::OutsideThirdPartyRange => {
                write!(f, "a code outside its range; {OWN_RANGE}")
            }
            CodeMisuse::PrefixContradictsLevel => write!(
                f,
                "a code whose prefix contradicts the level reported; E states an error, W a warning and I an info"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    #[specforge_test(
        invariant = "diagnostic_code_uniqueness",
        verify = "an extension reports only its own catalogued codes, or third-party codes whose prefix states their level"
    )]
    fn a_core_code_is_not_an_extensions() {
        let misuse = check_extension_code("@acme/squat", "E001", Level::Info).unwrap_err();
        assert_eq!(
            misuse,
            CodeMisuse::NotItsCode {
                owner: "core",
                title: "Parse error"
            }
        );
        let text = misuse.to_string();
        assert!(text.contains("core owns"), "{text}");
        assert!(text.contains("Parse error"), "{text}");
        assert!(text.contains("E900"), "{text}");
        // A first-party extension has no claim on core's codes either, at
        // any level.
        assert!(matches!(
            check_extension_code("@specforge/product", "E001", Level::Error),
            Err(CodeMisuse::NotItsCode { owner: "core", .. })
        ));
        // Another extension's catalogued code is not this one's.
        let other = check_extension_code("@acme/squat", "W041", Level::Warning).unwrap_err();
        assert!(
            other.to_string().contains("@specforge/product owns"),
            "{other}"
        );
    }

    #[specforge_test(
        invariant = "diagnostic_code_uniqueness",
        verify = "an extension reports only its own catalogued codes, or third-party codes whose prefix states their level"
    )]
    fn an_extensions_own_code_at_its_level_is_accepted() {
        assert_eq!(
            check_extension_code("@specforge/product", "W041", Level::Warning),
            Ok(())
        );
        assert_eq!(
            check_extension_code("@specforge/product", "I068", Level::Info),
            Ok(())
        );
        for (code, level) in [
            ("E900", Level::Error),
            ("W950", Level::Warning),
            ("I998", Level::Info),
        ] {
            assert_eq!(
                check_extension_code("@acme/x", code, level),
                Ok(()),
                "{code}"
            );
        }
    }

    #[specforge_test(
        invariant = "diagnostic_code_uniqueness",
        verify = "an extension reports only its own catalogued codes, or third-party codes whose prefix states their level"
    )]
    fn an_extensions_own_code_at_another_level_is_refused() {
        let misuse = check_extension_code("@specforge/product", "W041", Level::Error).unwrap_err();
        assert_eq!(
            misuse,
            CodeMisuse::WrongLevel {
                catalogued: Level::Warning
            }
        );
        assert!(misuse.to_string().contains("warning"), "{misuse}");
    }

    #[specforge_test(
        invariant = "diagnostic_code_uniqueness",
        verify = "an extension reports only its own catalogued codes, or third-party codes whose prefix states their level"
    )]
    fn a_graded_code_accepts_any_level() {
        // The testing extension's A-codes are graded by its pass.
        let entry = lookup("A001").expect("A001 is catalogued");
        assert_eq!(entry.level, Level::SetByPass);
        for level in [Level::Error, Level::Warning, Level::Info] {
            assert_eq!(
                check_extension_code(entry.owner, "A001", level),
                Ok(()),
                "{level:?}"
            );
        }
    }

    #[specforge_test(
        invariant = "diagnostic_code_uniqueness",
        verify = "an extension reports only its own catalogued codes, or third-party codes whose prefix states their level"
    )]
    fn a_retired_code_is_refused() {
        assert_eq!(
            check_extension_code("@acme/x", "E047", Level::Error),
            Err(CodeMisuse::Retired {
                replaced_by: Some("W139")
            })
        );
        assert_eq!(
            check_extension_code("@specforge/product", "W024", Level::Warning),
            Err(CodeMisuse::Retired { replaced_by: None })
        );
    }

    #[specforge_test(
        invariant = "diagnostic_code_uniqueness",
        verify = "an extension reports only its own catalogued codes, or third-party codes whose prefix states their level"
    )]
    fn a_first_party_uncatalogued_code_is_refused() {
        assert_eq!(
            check_extension_code("@specforge/rust", "W500", Level::Warning),
            Err(CodeMisuse::Uncatalogued)
        );
        // Not even the third-party range: a first-party code is catalogued.
        assert_eq!(
            check_extension_code("@specforge/rust", "W950", Level::Warning),
            Err(CodeMisuse::Uncatalogued)
        );
    }

    #[specforge_test(
        invariant = "diagnostic_code_uniqueness",
        verify = "an extension reports only its own catalogued codes, or third-party codes whose prefix states their level"
    )]
    fn a_third_party_code_outside_its_range_is_refused() {
        for code in ["W500", "E899", "E999", "W9500", "X950", "w950", "A950", ""] {
            assert_eq!(
                check_extension_code("@acme/x", code, Level::Warning),
                Err(CodeMisuse::OutsideThirdPartyRange),
                "{code:?}"
            );
        }
        // I999 is core's, not the third-party range's.
        assert!(matches!(
            check_extension_code("@acme/x", "I999", Level::Info),
            Err(CodeMisuse::NotItsCode { owner: "core", .. })
        ));
    }

    #[specforge_test(
        invariant = "diagnostic_code_uniqueness",
        verify = "an extension reports only its own catalogued codes, or third-party codes whose prefix states their level"
    )]
    fn a_third_party_prefix_states_its_level() {
        assert_eq!(
            check_extension_code("@acme/x", "W950", Level::Error),
            Err(CodeMisuse::PrefixContradictsLevel)
        );
        assert_eq!(
            check_extension_code("@acme/x", "E950", Level::Info),
            Err(CodeMisuse::PrefixContradictsLevel)
        );
        assert_eq!(
            check_extension_code("@acme/x", "I950", Level::Warning),
            Err(CodeMisuse::PrefixContradictsLevel)
        );
    }
}
