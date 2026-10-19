//! `specforge explain` and `specforge.explain`: what the diagnostic catalog says of one code, one
//! operation whose outcome is the document both surfaces print (`McpExplainResult`, ADR 0052).

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_diagnostics::{CodeEntry, lookup, retired};

use crate::{OpError, OpErrorKind, Refusal};

/// The refusal of a code the catalog neither has nor retired.
pub const UNKNOWN_CODE: &str = "unknown_code";

/// The forms a code takes: the suggestion of the refusal of an unknown one.
const CODE_FORMS: &str = "codes are E### (error), W### (warning), I### (info), A### (analyze finding), \
     R### and R-<AREA>-### (registry client); E900-E998, W900-W998 and I900-I998 are \
     third-party extensions' (see the extension's documentation)";

/// What the catalog says of a code: its entry, or that it was retired and what replaced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[serde(untagged)]
pub enum Explanation {
    /// A code the catalog has.
    Catalogued(Catalogued),
    /// A code the catalog retired.
    Retired(Retired),
}

/// A catalogued code: its entry, `retired: false`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct Catalogued {
    #[serde(flatten)]
    pub entry: Entry,
    pub retired: bool,
}

/// A retired code, upper-cased, `retired: true`, and the entry of the code that replaced it, if
/// one did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct Retired {
    pub code: String,
    pub retired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<Entry>,
}

/// One catalogue entry as both surfaces print it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct Entry {
    pub code: String,
    pub title: String,
    /// `core`, or the extension that owns the code.
    pub owner: String,
    /// The level's description (`error`, `warning`, `info`, `set by the analyze pass`).
    pub level: String,
    pub explanation: String,
    /// Its section of `docs/diagnostics.md`.
    pub docs: String,
}

impl Entry {
    fn of(entry: &CodeEntry) -> Self {
        Entry {
            code: entry.code.to_string(),
            title: entry.title.to_string(),
            owner: entry.owner.to_string(),
            level: entry.level.describe().to_string(),
            explanation: entry.explanation.to_string(),
            docs: entry.docs_href(),
        }
    }
}

/// The catalog's answer for `code`: any case, surrounding whitespace ignored. A code the catalog
/// neither has nor retired is `unknown_code` (invalid input), with the code forms as its
/// suggestion.
pub fn explain(code: &str) -> Result<Explanation, Refusal<ExplainField>> {
    let code = code.trim();
    if let Some(replacement) = retired(code) {
        return Ok(Explanation::Retired(Retired {
            code: code.to_uppercase(),
            retired: true,
            replaced_by: replacement.and_then(lookup).map(Entry::of),
        }));
    }
    match lookup(code) {
        Some(entry) => Ok(Explanation::Catalogued(Catalogued {
            entry: Entry::of(entry),
            retired: false,
        })),
        None => Err(OpError::new(
            OpErrorKind::InvalidInput,
            UNKNOWN_CODE,
            format!("unknown diagnostic code: {code}"),
        )
        .with_suggestion(CODE_FORMS)
        .about(ExplainField::Code)),
    }
}

/// The field of an explain request a refusal can be about (ADR 0072).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplainField {
    /// The diagnostic code asked about.
    Code,
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    #[specforge_test(
        behavior = "explain_diagnostic_code",
        verify = "specforge explain reads the code as specforge.explain does: any case, surrounding whitespace ignored"
    )]
    fn explain_answers_the_entry_any_case_and_trimmed() {
        let Explanation::Catalogued(found) = explain(" w018 ").unwrap() else {
            panic!("W018 is catalogued");
        };
        assert_eq!(found.entry.code, "W018");
        assert!(!found.retired);
        assert_eq!(
            found.entry.docs,
            "https://github.com/leaderiop/SpecForge/blob/main/docs/diagnostics.md#w018"
        );
    }

    #[test]
    fn a_retired_code_names_its_replacement_or_none() {
        let Explanation::Retired(renumbered) = explain("E047").unwrap() else {
            panic!("E047 is retired");
        };
        assert_eq!(renumbered.replaced_by.unwrap().code, "W139");

        let gone = serde_json::to_value(explain("W024").unwrap()).unwrap();
        assert_eq!(gone, serde_json::json!({"code": "W024", "retired": true}));
    }

    #[specforge_test(
        behavior = "explain_diagnostic_code",
        verify = "an uncatalogued code is refused with unknown_code on both surfaces"
    )]
    fn an_uncatalogued_code_is_unknown_code() {
        for code in ["Z123", "E901"] {
            let error = explain(code).unwrap_err();
            assert_eq!(error.error.kind, OpErrorKind::InvalidInput, "{code}");
            assert_eq!(error.error.code, UNKNOWN_CODE, "{code}");
            assert_eq!(
                error.error.message,
                format!("unknown diagnostic code: {code}"),
                "{code}"
            );
            assert!(
                error
                    .error
                    .suggestion
                    .as_deref()
                    .unwrap()
                    .starts_with("codes are E###"),
                "{code}"
            );
        }
    }

    #[specforge_test(
        invariant = "mcp_structured_error_responses",
        verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
    )]
    fn explain_states_which_field_a_refusal_is_about() {
        assert_eq!(
            explain("Z123").unwrap_err().subject,
            Some(ExplainField::Code)
        );
    }
}
