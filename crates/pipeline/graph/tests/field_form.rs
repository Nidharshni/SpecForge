//! `FieldForm`: a declared field type's one reading. What the field takes
//! (`admit`), how a written value is stored (`coerce`) and what every output
//! writes (`write`) agree with the schema of that JSON (`json_schema`).

use specforge_common::shape::{self, Definitions};
use specforge_common::{SourceSpan, Sym};
use specforge_graph::field_form::{self, FieldForm, FieldJson, ValueShape};
use specforge_parser::{FieldMap, FieldValue, SpannedRef, parse_expression};
use specforge_protocol_types::FieldType;
use specforge_test_macros::test as specforge_test;

fn span() -> SourceSpan {
    SourceSpan {
        file: Sym::new("t.spec"),
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 0,
    }
}

fn reference(id: &str) -> SpannedRef {
    SpannedRef {
        id: id.to_string(),
        span: span(),
    }
}

fn expression() -> FieldValue {
    FieldValue::Expression(vec![
        parse_expression("a < 1").unwrap(),
        parse_expression("b >= 2ms").unwrap(),
    ])
}

/// One value per `FieldValue` variant, plus the scalars a field commonly
/// gets written as.
fn samples() -> Vec<FieldValue> {
    let mut inner = FieldMap::new();
    inner.push(Sym::new("m"), expression());
    let mut block = FieldMap::new();
    block.push(Sym::new("k"), FieldValue::String("v".into()));
    block.push(Sym::new("n"), FieldValue::Block(inner));
    vec![
        FieldValue::String("low".into()),
        FieldValue::String("7".into()),
        FieldValue::String("true".into()),
        FieldValue::Identifier("high".into()),
        FieldValue::Identifier("true".into()),
        FieldValue::Date("2026-10-13".into()),
        FieldValue::Integer(3),
        FieldValue::Boolean(false),
        FieldValue::StringList(vec!["s".into(), "t".into()]),
        FieldValue::ReferenceList(vec![reference("x"), reference("y")]),
        FieldValue::MixedList(vec![FieldValue::Integer(1), FieldValue::String("a".into())]),
        FieldValue::MixedList(vec![FieldValue::Boolean(true), FieldValue::Integer(2)]),
        FieldValue::VariantList(vec!["a".into(), "b".into()]),
        FieldValue::TypeUnion(vec!["string".into(), "string[]".into()]),
        FieldValue::Block(block),
        expression(),
    ]
}

fn forms() -> Vec<FieldForm> {
    let mut forms: Vec<FieldForm> = FieldType::ALL
        .iter()
        .map(|t| FieldForm::new(*t, Vec::new()))
        .collect();
    forms.push(FieldForm::new(
        FieldType::Enum,
        vec!["low".to_string(), "high".to_string()],
    ));
    forms
}

/// The field text the checks read for a scalar value (E061 compares an
/// enum's text with its declared values); lists have none that matters.
fn text(value: &FieldValue) -> String {
    match value {
        FieldValue::String(s) | FieldValue::Identifier(s) | FieldValue::Date(s) => s.clone(),
        FieldValue::Integer(n) => n.to_string(),
        FieldValue::Boolean(b) => b.to_string(),
        _ => String::new(),
    }
}

/// The document of `form`'s schema: the schema at the root, `FieldJson`
/// defined once under `$defs`.
fn document(form: &FieldForm) -> serde_json::Value {
    let mut defs = Definitions::default();
    let schema = form.json_schema(&mut defs);
    defs.finish(schema)
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a value the check admits is written as the published schema admits it"
)]
fn a_value_the_check_admits_is_written_as_its_schema_admits() {
    let any_value = shape::build::<FieldJson>();
    let span = span();
    let mut admitted = 0;
    for form in forms() {
        let schema = document(&form);
        for value in samples() {
            let stored = form.coerce(&value, Some(&span)).unwrap_or(value.clone());
            let written: serde_json::Value = field_form::write(&stored).into();
            if form.admit(ValueShape::of(&stored), &text(&stored)).is_ok() {
                admitted += 1;
                assert_eq!(
                    shape::violations(&schema, &written),
                    Vec::<String>::new(),
                    "{:?} stored as {stored:?} on {form:?}",
                    value
                );
            }
            // Admitted or not, every value is written as a field value.
            assert_eq!(
                shape::violations(&any_value, &written),
                Vec::<String>::new(),
                "{value:?}"
            );
        }
    }
    assert!(
        admitted > 40,
        "{admitted} admitted pairs: the sample is thin"
    );
}

#[test]
fn coercion_is_idempotent() {
    let span = span();
    for form in forms() {
        for value in samples() {
            if let Some(once) = form.coerce(&value, Some(&span)) {
                assert!(
                    form.coerce(&once, Some(&span)).is_none(),
                    "{value:?} on {form:?}"
                );
            }
        }
    }
}

#[test]
fn the_written_form_never_holds_a_parse_tree() {
    assert_eq!(
        serde_json::Value::from(field_form::write(&expression())),
        serde_json::json!("a < 1\nb >= 2ms")
    );
    let mut block = FieldMap::new();
    block.push(Sym::new("m"), expression());
    assert_eq!(
        serde_json::Value::from(field_form::write(&FieldValue::Block(block))),
        serde_json::json!({"m": "a < 1\nb >= 2ms"})
    );
}

#[test]
fn a_scalar_or_scalar_list_on_a_list_field_is_stored_as_a_list() {
    let span = span();
    let list_form = FieldForm::new(FieldType::ReferenceList, Vec::new());
    // Compared by their debug form: a parsed value has no equality.
    let strings = |items: &[&str]| {
        format!(
            "{:?}",
            Some(FieldValue::StringList(
                items.iter().map(|s| s.to_string()).collect(),
            ))
        )
    };
    let list = |value: FieldValue| {
        format!(
            "{:?}",
            FieldForm::new(FieldType::ReferenceList, Vec::new()).coerce(&value, Some(&span))
        )
    };
    assert_eq!(list(FieldValue::Integer(5)), strings(&["5"]));
    assert_eq!(list(FieldValue::Boolean(true)), strings(&["true"]));
    assert_eq!(
        list(FieldValue::Date("2026-10-13".into())),
        strings(&["2026-10-13"])
    );
    assert_eq!(
        list(FieldValue::MixedList(vec![
            FieldValue::String("a".into()),
            FieldValue::Integer(7)
        ])),
        strings(&["a", "7"])
    );
    // A bare word keeps its span and links like `field [id]`.
    let coerced = list_form
        .coerce(&FieldValue::Identifier("inv1".into()), Some(&span))
        .unwrap();
    assert_eq!(ValueShape::of(&coerced), ValueShape::References);
}
