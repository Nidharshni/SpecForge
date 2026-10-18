//! `identifier`: one word from an open set (ADR 0147).

use specforge_graph::field_form::{FieldForm, ValueShape};
use specforge_protocol_types::FieldType;
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a value that is not one word on an identifier field is E061"
)]
fn an_identifier_field_takes_one_word() {
    let form = FieldForm::new(FieldType::Identifier, Vec::new());

    assert!(form.admit(ValueShape::Identifier, "cart").is_ok());
    assert!(form.admit(ValueShape::String, "cart").is_ok());
    assert!(form.admit(ValueShape::String, "_a1").is_ok());

    let remedy = "write one word: letters, digits and _, starting with a letter or _";
    for (shape, text) in [
        (ValueShape::String, "api/lsp"),
        (ValueShape::String, "two words"),
        (ValueShape::String, "1abc"),
        (ValueShape::String, ""),
        (ValueShape::Date, "2026-01-01"),
        (ValueShape::Integer, "7"),
    ] {
        let refusal = form.admit(shape, text).unwrap_err();
        assert_eq!(refusal.suggestion.as_deref(), Some(remedy), "{text}");
    }
    let list = form.admit(ValueShape::List, "").unwrap_err();
    assert_eq!(
        list.suggestion.as_deref(),
        Some("give a single value, not a list")
    );
}
