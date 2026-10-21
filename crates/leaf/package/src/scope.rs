//! A package scope: what a scoped package name starts with and what a registry entry's
//! `scope_filter`, a registry token's filter and `scope assign` name (ADR 0200).
//!
//! Read where the text enters, by the rule a package name's scope is read by, and compared only as
//! itself. If [`crate::name::is_part`] changes, the JSON schema's `scope_filter` pattern changes
//! with it.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::name::{MAX_NAME_LEN, PackageName, is_part};

/// A package scope: `@` and one name part (`@acme`).
///
/// Serialized as its text; deserializing text that is not a scope fails (`specforge.json`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Scope(pub(crate) String);

/// Why text is not a scope; `write` is the scope it most likely meant, when one is evident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeError {
    pub text: String,
    pub write: Option<Scope>,
}

/// `@scope/x` must stay a name: two bytes for `/x`.
fn is_scope(text: &str) -> bool {
    text.len() <= MAX_NAME_LEN - 2 && text.strip_prefix('@').is_some_and(is_part)
}

impl Scope {
    /// Read `text` (no trimming). Refused, never repaired: `sdk`, `@sdk/`, `@SDK` and `""` are not
    /// scopes.
    pub fn parse(text: &str) -> Result<Self, ScopeError> {
        if is_scope(text) {
            return Ok(Scope(text.to_string()));
        }
        let first = text
            .trim_start_matches('@')
            .split('/')
            .next()
            .unwrap_or_default();
        let guess = format!("@{}", first.to_ascii_lowercase());
        Err(ScopeError {
            text: text.to_string(),
            write: (is_scope(&guess) && guess != text).then_some(Scope(guess)),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `name` is in this scope: `@acme` covers `@acme/tool`, never `@acmex/tool` or a local
    /// name.
    pub fn covers(&self, name: &PackageName) -> bool {
        name.scope().as_ref() == Some(self)
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.0)
    }
}

impl fmt::Display for ScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = &self.text;
        match &self.write {
            Some(write) => write!(f, "'{text}' is not a scope: write '{write}'"),
            None => write!(
                f,
                "'{text}' is not a scope: a scope is @ and one name part (lowercase letters, digits, '.', '_' and '-', starting with a letter or digit), such as @acme"
            ),
        }
    }
}

impl std::error::Error for ScopeError {}

impl FromStr for Scope {
    type Err = ScopeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl TryFrom<String> for Scope {
    type Error = ScopeError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

impl From<Scope> for String {
    fn from(scope: Scope) -> Self {
        scope.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_is_at_and_one_name_part() {
        for ok in ["@acme", "@a", "@my-org.x_1"] {
            assert_eq!(Scope::parse(ok).unwrap().as_str(), ok);
        }
        let long = format!("@{}", "a".repeat(212));
        for bad in [
            "",
            "@",
            "acme",
            "@acme/",
            "@Acme",
            "@acme/tool",
            "@-x",
            "@@acme",
            &long,
        ] {
            assert!(Scope::parse(bad).is_err(), "{bad:?}");
        }
        assert!(Scope::parse(&long[..long.len() - 1]).is_ok());
    }

    #[test]
    fn a_refused_scope_names_the_scope_to_write() {
        for (text, write) in [
            ("sdk", "@sdk"),
            ("@sdk/", "@sdk"),
            ("@SDK", "@sdk"),
            ("@org/soft", "@org"),
        ] {
            assert_eq!(
                Scope::parse(text).unwrap_err().to_string(),
                format!("'{text}' is not a scope: write '{write}'")
            );
        }
        for text in ["", "@", "@-x", "@_x"] {
            let why = Scope::parse(text).unwrap_err();
            assert_eq!(why.write, None, "{text:?}");
            assert!(
                why.to_string()
                    .starts_with(&format!("'{text}' is not a scope: a scope is @")),
                "{why}"
            );
        }
    }

    #[test]
    fn a_scope_covers_the_names_under_it_only() {
        let web = Scope::parse("@web").unwrap();
        for (name, covered) in [
            ("@web/ui", true),
            ("@webui/x", false),
            ("@web.ui/x", false),
            ("web", false),
        ] {
            assert_eq!(
                web.covers(&PackageName::parse(name).unwrap()),
                covered,
                "{name}"
            );
        }
        assert_eq!(PackageName::parse("@web/ui").unwrap().scope(), Some(web));
        assert_eq!(
            serde_json::from_str::<Scope>("\"@acme\"").unwrap().as_str(),
            "@acme"
        );
        let why = serde_json::from_str::<Scope>("\"sdk\"")
            .unwrap_err()
            .to_string();
        assert!(why.contains("'sdk' is not a scope: write '@sdk'"), "{why}");
    }
}
