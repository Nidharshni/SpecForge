//! A value is written in its field's form (ADR 0147), by the declared types of `FieldForms`.

use super::*;
use specforge_graph::FieldForm;
use specforge_protocol_types::FieldType;
use specforge_test_macros::test as specforge_test;
use std::collections::{BTreeSet, HashMap};

fn forms() -> FieldForms {
    let mut declared = HashMap::new();
    for (kind, field, ty) in [
        ("behavior", "category", FieldType::Identifier),
        ("behavior", "contract", FieldType::String),
        ("behavior", "types", FieldType::ReferenceList),
        ("behavior", "tags", FieldType::StringList),
        ("type", "description", FieldType::String),
    ] {
        declared.insert(
            (kind.to_string(), field.to_string()),
            FieldForm::new(ty, vec![]),
        );
    }
    FieldForms::new(
        declared,
        BTreeSet::from(["type".to_string()]),
        BTreeSet::new(),
    )
}

fn fmt_forms(source: &str) -> String {
    format_source(source, &FormatConfig::default(), &forms()).formatted
}

#[specforge_test(
    behavior = "apply_format_rules",
    verify = "a value is written in its field's form, read from the project's declarations"
)]
fn a_value_is_written_in_its_fields_form() {
    assert_eq!(
        fmt_forms("behavior b \"B\" {\n  category \"cart\"\n  contract plain\n}\n"),
        "behavior b \"B\" {\n  category cart\n  contract \"plain\"\n}\n"
    );
    assert_eq!(
        fmt_forms("behavior b \"B\" {\n  types [\"a\", b]\n  tags [x, \"y\"]\n}\n"),
        "behavior b \"B\" {\n  types [a, b]\n  tags  [\"x\", \"y\"]\n}\n"
    );
}

#[specforge_test(
    behavior = "apply_format_rules",
    verify = "a value is written in its field's form, read from the project's declarations"
)]
fn what_cannot_be_written_in_the_form_is_left() {
    let text = "behavior b \"B\" {\n  category \"api/lsp\"\n  contract \"\"\"\n    x\n  \"\"\"\n}\n\n\
                type T \"T\" {\n  description string\n}\n";
    assert_eq!(fmt_forms(text), text);
    let nested = "behavior b \"B\" {\n  inner {\n    category \"x\"\n  }\n}\n";
    assert_eq!(fmt_forms(nested), nested);
    let truth = "behavior b \"B\" {\n  category \"true\"\n}\n";
    assert_eq!(fmt_forms(truth), truth);
}

#[specforge_test(
    behavior = "apply_format_rules",
    verify = "a value is written in its field's form, read from the project's declarations"
)]
fn a_quoted_scheme_ref_in_a_reference_list_loses_its_quotes() {
    assert_eq!(
        fmt_forms("behavior b \"B\" {\n  types [\"gh.issue:1\", b]\n}\n"),
        "behavior b \"B\" {\n  types [gh.issue:1, b]\n}\n"
    );
}

#[specforge_test(
    behavior = "apply_format_rules",
    verify = "without the project's declarations no value is rewritten"
)]
fn with_no_forms_nothing_is_rewritten() {
    let text = "behavior b \"B\" {\n  category \"cart\"\n  contract plain\n}\n";
    assert_eq!(fmt(text), text);
}

#[specforge_test(
    behavior = "apply_format_rules",
    verify = "a value is written in its field's form, read from the project's declarations"
)]
fn the_rewritten_text_is_a_fixed_point() {
    let once = fmt_forms("behavior b \"B\" {\n  category \"cart\"\n  types [\"a\"]\n}\n");
    assert_eq!(fmt_forms(&once), once);
}
