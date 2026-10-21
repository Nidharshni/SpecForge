//! Whether an ID written as a token is a legal entity ID: the grammar's identifier and the registry's
//! entity-ID rule ([`specforge_registry::refused_id`], ADR 0189). `rename` and the typed `create` ask it.

use crate::{OpError, OpErrorKind};
use specforge_parser::lex::is_identifier;
use specforge_registry::{IdRefusal, KindRegistry, refused_id};

/// The new ID is not a legal entity ID.
pub const INVALID_ID: &str = "invalid_name";

/// Whether `id`, written as a token, is a legal entity ID: the grammar's identifier ([`is_identifier`]) and
/// nothing [`refused_id`] refuses over `kinds` (E014 length, E013 reserved word).
pub fn validate_id(id: &str, kinds: &KindRegistry) -> Result<(), OpError> {
    let refusal = refused_id(id, kinds);
    if !is_identifier(id) || refusal == Some(IdRefusal::Length) {
        return Err(OpError::new(
            OpErrorKind::InvalidInput,
            INVALID_ID,
            format!(
                "invalid entity ID '{id}': it must be 2-60 characters, letters, digits and \
                 underscores, starting with a letter or underscore"
            ),
        ));
    }
    if refusal == Some(IdRefusal::Reserved) {
        return Err(OpError::new(
            OpErrorKind::InvalidInput,
            INVALID_ID,
            format!(
                "invalid entity ID '{id}': it is a reserved word (E013), a structural keyword or the keyword \
                 of a declared kind"
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entity_id_follows_the_grammar_and_e014() {
        let none = KindRegistry::new();
        for ok in ["ab", "_x", "auth_token", "A1", &"a".repeat(60)] {
            assert!(validate_id(ok, &none).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "a",
            "1abc",
            "has-dash",
            "has space",
            "émile",
            "defaults",
            "spec",
            &"a".repeat(61),
        ] {
            assert_eq!(
                validate_id(bad, &none).unwrap_err().code,
                INVALID_ID,
                "{bad}"
            );
        }
    }
}
