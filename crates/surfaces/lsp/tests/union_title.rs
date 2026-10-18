//! A titled union reads as a form: its title is a string, its variants are values (ADR 0148).

use specforge_test_macros::test as spec;

use crate::semantic_tokens::{kinds, token, tokens_of};

#[spec(
    behavior = "provide_semantic_tokens",
    verify = "a titled union is a form and its variants are values"
)]
fn a_titled_union_is_a_form_and_its_variants_are_values() {
    let tokens = tokens_of(
        concat!(
            "type status \"Status\" = active | inactive\n",
            "\n",
            "type next \"Next\" {\n",
            "}\n",
        ),
        kinds(&[("type", None)]),
    );
    assert_eq!(token(&tokens, "\"Status\"").token_type, "string");
    // A variant is a value of no field: it takes no token of its own.
    assert!(
        tokens
            .iter()
            .all(|t| t.text != "active" && t.text != "inactive")
    );
    let next = token(&tokens, "next");
    assert_ne!(next.modifiers & specforge_lsp::MOD_DECLARATION, 0);
}
