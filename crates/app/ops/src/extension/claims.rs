//! The one rule for the specifiers of one change that name one extension (ADR 0098): the same way
//! again is a duplicate, added once by the first; another way (other bytes, another version,
//! another source) is `extension_conflict`, naming both. `add` and `init` claim every specifier
//! here before they ask their lock plan for it.

use super::{Install, Source, Specified};
use crate::{OpError, OpErrorKind};
use specforge_package::{PackageName, Version};

/// What makes two specifiers add one extension the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Key {
    Builtin,
    Registry(String),
    Bytes(String),
}

/// An extension a specifier of the change names, and how it adds it.
struct Claim {
    name: String,
    key: Key,
    /// How the refusal of a conflict says it: `builtin`, `<version> from <source>`.
    shown: String,
    specifier: String,
}

/// What a claim found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Claimed {
    /// The first specifier to name the extension: ask the plan for it.
    First,
    /// An earlier specifier (`first`) names `name` the same way: ask nothing.
    Duplicate { name: String, first: String },
}

/// What the specifiers of one change claimed so far.
#[derive(Default)]
pub(crate) struct Claims {
    claims: Vec<Claim>,
    /// Every specifier claimed so far, by source, with the extension it named.
    seen: Vec<(Source, String)>,
}

impl Claims {
    /// `Duplicate` when an earlier specifier has `wanted`'s source: it resolves the same way, so
    /// nothing is read or asked again.
    pub(crate) fn repeated(&self, wanted: &Specified) -> Option<Claimed> {
        let (_, name) = self
            .seen
            .iter()
            .find(|(source, _)| *source == wanted.source)?;
        self.duplicate_of(name)
    }

    /// A builtin, which every spelling adds the same way.
    pub(crate) fn builtin(&mut self, wanted: &Specified, name: &str) -> Result<Claimed, OpError> {
        self.claim(wanted, name, Key::Builtin, "builtin".to_string())
    }

    /// A local file, read: keyed by its SHA-256, shown `<version> from <source>`.
    pub(crate) fn local(
        &mut self,
        wanted: &Specified,
        install: &Install<'_>,
    ) -> Result<Claimed, OpError> {
        let shown = format!("{} from {}", install.version(), install.source());
        let key = Key::Bytes(install.sha256().to_string());
        self.claim(wanted, install.name().as_str(), key, shown)
    }

    /// A registry package at the version it resolved to: shown `<version> from registry`.
    pub(crate) fn registry(
        &mut self,
        wanted: &Specified,
        name: &PackageName,
        version: &Version,
    ) -> Result<Claimed, OpError> {
        let key = Key::Registry(version.to_string());
        self.claim(
            wanted,
            name.as_str(),
            key,
            format!("{version} from registry"),
        )
    }

    fn duplicate_of(&self, name: &str) -> Option<Claimed> {
        let first = self.claims.iter().find(|claim| claim.name == name)?;
        Some(Claimed::Duplicate {
            name: name.to_string(),
            first: first.specifier.clone(),
        })
    }

    /// `First` (and recorded), `Duplicate` when an earlier specifier claimed `name` as `key`, or
    /// the conflict when it claimed it differently.
    fn claim(
        &mut self,
        wanted: &Specified,
        name: &str,
        key: Key,
        shown: String,
    ) -> Result<Claimed, OpError> {
        let claimed = match self.claims.iter().find(|claim| claim.name == name) {
            Some(first) if first.key == key => Claimed::Duplicate {
                name: name.to_string(),
                first: first.specifier.clone(),
            },
            Some(first) => {
                return Err(OpError::new(
                    OpErrorKind::Conflict,
                    "extension_conflict",
                    format!(
                        "'{}' and '{}' both add '{name}', as {} and as {shown}",
                        first.specifier, wanted.specifier, first.shown
                    ),
                )
                .with_suggestion(format!("add '{name}' once")));
            }
            None => {
                self.claims.push(Claim {
                    name: name.to_string(),
                    key,
                    shown,
                    specifier: wanted.specifier.clone(),
                });
                Claimed::First
            }
        };
        self.seen.push((wanted.source.clone(), name.to_string()));
        Ok(claimed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn specified(specifier: &str) -> Specified {
        Specified {
            specifier: specifier.to_string(),
            source: Source::Local(PathBuf::from(specifier)),
        }
    }

    fn package(name: &str) -> PackageName {
        PackageName::parse(name).unwrap()
    }

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn a_second_claim_the_same_way_is_a_duplicate_of_the_first() {
        let mut claims = Claims::default();
        let (name, one) = (package("@x/b"), version("1.0.0"));

        let first = claims.registry(&specified("@x/b"), &name, &one).unwrap();
        let again = claims
            .registry(&specified("@x/b@1.0.0"), &name, &one)
            .unwrap();

        assert_eq!(first, Claimed::First);
        assert_eq!(
            again,
            Claimed::Duplicate {
                name: "@x/b".to_string(),
                first: "@x/b".to_string()
            }
        );
    }

    #[test]
    fn a_claim_another_way_is_a_conflict_naming_both() {
        let mut claims = Claims::default();
        let name = package("@x/b");
        claims
            .registry(&specified("@x/b@1.0.0"), &name, &version("1.0.0"))
            .unwrap();

        let error = claims
            .registry(&specified("@x/b@2.0.0"), &name, &version("2.0.0"))
            .unwrap_err();

        assert_eq!(error.code, "extension_conflict");
        assert_eq!(error.kind, OpErrorKind::Conflict);
        assert_eq!(
            error.message,
            "'@x/b@1.0.0' and '@x/b@2.0.0' both add '@x/b', as 1.0.0 from registry and as 2.0.0 from registry"
        );
        assert_eq!(error.suggestion.as_deref(), Some("add '@x/b' once"));
    }

    #[test]
    fn a_builtin_is_claimed_the_same_way_by_every_spelling() {
        let mut claims = Claims::default();
        let spelling = |text: &str| Specified {
            specifier: text.to_string(),
            source: Source::Builtin("@specforge/product"),
        };

        let first = claims.builtin(&spelling("@specforge/product"), "@specforge/product");
        let again = claims.builtin(&spelling("@specforge/product@latest"), "@specforge/product");

        assert_eq!(first.unwrap(), Claimed::First);
        assert!(matches!(again.unwrap(), Claimed::Duplicate { .. }));
    }

    #[test]
    fn a_repeated_source_is_a_duplicate_before_it_is_resolved() {
        let mut claims = Claims::default();
        let (name, one) = (package("@x/b"), version("1.0.0"));
        let wanted = specified("a.wasm");
        assert_eq!(claims.repeated(&wanted), None);
        claims.registry(&wanted, &name, &one).unwrap();

        let repeated = claims.repeated(&wanted);

        assert_eq!(
            repeated,
            Some(Claimed::Duplicate {
                name: "@x/b".to_string(),
                first: "a.wasm".to_string()
            })
        );
        // A source first seen as a duplicate is repeated too.
        let same_name = specified("other.wasm");
        claims.registry(&same_name, &name, &one).unwrap();
        assert!(claims.repeated(&same_name).is_some());
    }
}
