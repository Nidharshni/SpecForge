//! `identifier`: one word from an open set (ADR 0147).

use std::collections::{BTreeSet, HashMap};

use specforge_graph::Written;
use specforge_graph::field_form::{FieldForm, FieldForms, ValueShape};
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

fn forms() -> FieldForms {
    let mut declared = HashMap::new();
    for (name, ty) in [
        ("id_", FieldType::Identifier),
        ("ref_", FieldType::Reference),
        ("refs", FieldType::ReferenceList),
        ("text", FieldType::String),
        ("texts", FieldType::StringList),
        ("num", FieldType::Integer),
    ] {
        declared.insert(
            ("k".to_string(), name.to_string()),
            FieldForm::new(ty, vec![]),
        );
    }
    declared.insert(
        ("m".to_string(), "text".to_string()),
        FieldForm::new(FieldType::String, vec![]),
    );
    FieldForms::new(declared, BTreeSet::from(["m".to_string()]), BTreeSet::new())
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a quoted word on a word field and a bare word on a text field are W159"
)]
fn a_field_type_writes_its_values_in_one_form() {
    let forms = forms();
    let rewrite = |field, written, text| forms.rewrite("k", field, written, text);

    for field in ["id_", "ref_", "refs"] {
        assert_eq!(
            rewrite(field, Written::Quoted, "a"),
            Some(Written::Bare),
            "{field}"
        );
        assert_eq!(rewrite(field, Written::Bare, "a"), None, "{field}");
    }
    for field in ["text", "texts"] {
        assert_eq!(
            rewrite(field, Written::Bare, "a"),
            Some(Written::Quoted),
            "{field}"
        );
        assert_eq!(rewrite(field, Written::Quoted, "a"), None, "{field}");
    }
    assert_eq!(rewrite("num", Written::Quoted, "7"), None);
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a bare word on a kind that takes members is not W159"
)]
fn rewrite_leaves_what_it_cannot_write() {
    let forms = forms();
    // A path, or true/false, cannot be a bare word.
    assert_eq!(forms.rewrite("k", "ref_", Written::Quoted, "api/lsp"), None);
    assert_eq!(forms.rewrite("k", "id_", Written::Quoted, "true"), None);
    // A scheme ref id is a reference list item's bare form (ADR 0153); a scalar reference takes a word only,
    // and an identifier too, as a scheme ref id does not parse bare outside a list.
    assert_eq!(
        forms.rewrite("k", "refs", Written::Quoted, "gh.issue:1"),
        Some(Written::Bare)
    );
    assert_eq!(
        forms.rewrite("k", "ref_", Written::Quoted, "gh.issue:1"),
        None
    );
    assert_eq!(
        forms.rewrite("k", "id_", Written::Quoted, "gh.issue:1"),
        None
    );
    // A scheme ref or a literal is neither form.
    assert_eq!(
        forms.rewrite("k", "texts", Written::SchemeRef, "gh.issue:1"),
        None
    );
    assert_eq!(
        forms.rewrite("k", "text", Written::Literal, "2026-01-01"),
        None
    );
    // A bare word on a members kind may be a member's type.
    assert_eq!(forms.rewrite("m", "text", Written::Bare, "string"), None);
    // An undeclared field or kind has no form.
    assert_eq!(forms.rewrite("k", "nope", Written::Bare, "a"), None);
    assert_eq!(forms.rewrite("zzz", "text", Written::Bare, "a"), None);
}
