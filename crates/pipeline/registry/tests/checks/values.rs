//! E061: a field value that cannot be the type its extension declared,
//! read from the record's value shape and span, never a graph node.

use specforge_common::{SourceSpan, Sym};
use specforge_extension_sdk::prelude::*;
use specforge_graph::ValueShape;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::entity::EntityRecord;
use specforge_test_macros::test as spec;

use crate::support::{build, check, coded_in, declare, span};

/// `ticket`: an enum, a bool, an integer, a string list, a string and a
/// reference to another `ticket`.
fn tickets() -> ExtensionDeclaration {
    declare("@test/tickets", |c| {
        c.kind("Ticket", |k| {
            k.keyword("ticket");
            k.field("priority", |f| {
                f.vocabulary(&Vocabulary::new(&["low", "medium", "high"]));
            });
            k.field("urgent", |f| {
                f.field_type(FieldType::Bool);
            });
            k.field("points", |f| {
                f.field_type(FieldType::Integer);
            });
            k.field("labels", |f| {
                f.field_type(FieldType::StringList);
            });
            k.field("summary", |f| {
                f.field_type(FieldType::String);
            });
            k.field("blocker", |f| {
                f.field_type(FieldType::Reference).target_kind("ticket");
            });
            k.field("details", |f| {
                f.field_type(FieldType::Block);
            });
            k.field("watchers", |f| {
                f.field_type(FieldType::ReferenceList).target_kind("ticket");
            });
        });
    })
}

/// The E061 messages of the checks over one `ticket` with `fields`.
fn e061(fields: &[(&str, ValueShape, &str)]) -> Vec<(String, Option<String>)> {
    let mut record = EntityRecord::new("ticket", "t1", span("main.spec"));
    for (key, shape, text) in fields {
        record = record.with_value(key, *shape, text);
    }
    let diags = check(&build([tickets()]), &[record]);
    coded_in(&diags, "E061")
        .into_iter()
        .map(|d| (d.message.clone(), d.suggestion.clone()))
        .collect()
}

#[spec(
    behavior = "check_field_value_types",
    verify = "a value that is not the declared integer, bool or enum type is an error"
)]
fn values_that_are_not_the_declared_integer_bool_or_enum_are_errors() {
    let found = e061(&[
        ("priority", ValueShape::Identifier, "urgent"),
        ("urgent", ValueShape::Identifier, "maybe"),
        ("points", ValueShape::String, "many"),
        ("priority", ValueShape::Integer, "3"),
    ]);
    let messages: Vec<&str> = found.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(
        messages,
        [
            "field 'priority' of ticket 't1' is declared enum (low, medium, high), but was given urgent",
            "field 'urgent' of ticket 't1' is declared bool, but was given maybe",
            "field 'points' of ticket 't1' is declared integer, but was given \"many\"",
            "field 'priority' of ticket 't1' is declared enum (low, medium, high), but was given 3",
        ]
    );
    // A bool's remedy is the two literals; an enum given a non-word names
    // its values.
    assert_eq!(found[1].1.as_deref(), Some("use true or false"));
    assert_eq!(found[3].1.as_deref(), Some("use one of: low, medium, high"));
}

#[test]
fn values_of_the_declared_types_are_no_diagnostic() {
    let found = e061(&[
        ("priority", ValueShape::String, "medium"),
        ("priority", ValueShape::Identifier, "high"),
        ("urgent", ValueShape::Boolean, "true"),
        ("points", ValueShape::Integer, "8"),
        ("labels", ValueShape::Strings, "one"),
        ("summary", ValueShape::String, "free text"),
        ("blocker", ValueShape::Identifier, "t2"),
        ("details", ValueShape::Block, "k"),
        ("summary", ValueShape::Expression, "a < 1"),
        ("undeclared", ValueShape::Block, "a"),
    ]);
    assert!(found.is_empty(), "{found:?}");
}

#[spec(
    behavior = "check_field_value_types",
    verify = "an enum value suggests the closest declared value"
)]
fn an_enum_value_suggests_the_closest_declared_value() {
    let found = e061(&[("priority", ValueShape::Identifier, "hgh")]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].1.as_deref(), Some("did you mean 'high'?"));
}

#[spec(
    behavior = "check_field_value_types",
    verify = "a list on a field declared as a single value is an error"
)]
fn a_list_on_a_single_value_field_is_an_error() {
    let found = e061(&[
        ("summary", ValueShape::Strings, "a"),
        ("blocker", ValueShape::References, "t2"),
        ("priority", ValueShape::Strings, "low"),
    ]);
    assert_eq!(found.len(), 3, "{found:?}");
    assert_eq!(
        found[0],
        (
            "field 'summary' of ticket 't1' is declared string, but was given a list".to_string(),
            Some("give a single value, not a list".to_string())
        )
    );
    // An enum given a list names its values first.
    assert_eq!(found[2].1.as_deref(), Some("use one of: low, medium, high"));
}

#[test]
fn the_message_describes_the_value_by_its_shape() {
    let found = e061(&[
        ("points", ValueShape::Block, "a, b"),
        ("points", ValueShape::Expression, "a < 1"),
        ("points", ValueShape::TypeUnion, "string | int"),
        ("points", ValueShape::Date, "2026-10-07"),
        ("points", ValueShape::Mixed, "1, x"),
    ]);
    let given: Vec<&str> = found
        .iter()
        .map(|(m, _)| m.rsplit("was given ").next().unwrap())
        .collect();
    assert_eq!(
        given,
        [
            "a block",
            "an expression",
            "string | int",
            "2026-10-07",
            "a list"
        ]
    );
}

#[test]
fn an_unregistered_kind_or_field_is_left_to_e024_and_w020() {
    let record = EntityRecord::new("widget", "w1", span("main.spec")).with_value(
        "points",
        ValueShape::String,
        "many",
    );
    let diags = check(&build([tickets()]), &[record]);
    assert!(coded_in(&diags, "E061").is_empty(), "{diags:?}");
}

/// The error is at the value's own span, else at the entity.
#[test]
fn e061_points_at_the_value_else_the_entity() {
    let mut record = EntityRecord::new("ticket", "t1", span("main.spec"))
        .with_value("points", ValueShape::String, "many")
        .with_value("urgent", ValueShape::String, "maybe");
    record.fields[0].value_span = Some(SourceSpan {
        file: Sym::new("main.spec"),
        start_line: 3,
        start_col: 10,
        end_line: 3,
        end_col: 16,
    });
    let diags = check(&build([tickets()]), &[record]);
    let e061 = coded_in(&diags, "E061");
    assert_eq!(e061.len(), 2, "{diags:?}");
    let at = |d: &specforge_common::Diagnostic| {
        let s = d.span.as_ref().unwrap();
        (s.start_line, s.start_col)
    };
    assert_eq!(at(e061[0]), (3, 10));
    assert_eq!(at(e061[1]), (1, 0));
}

/// Each row is one value on one field: the field, its shape and text, what
/// the message says was given, and the remedy E061 suggests.
#[spec(
    behavior = "check_field_value_types",
    verify = "a value of a shape its declared type never takes is an error"
)]
fn a_value_of_a_shape_its_type_never_takes_is_an_error() {
    let refused: [(&str, ValueShape, &str, &str, Option<&str>); 6] = [
        (
            "summary",
            ValueShape::Block,
            "a",
            "a block",
            Some("write the value as text"),
        ),
        (
            "blocker",
            ValueShape::Block,
            "a",
            "a block",
            Some("name one entity by its id"),
        ),
        (
            "blocker",
            ValueShape::Expression,
            "a < 1",
            "an expression",
            Some("name one entity by its id"),
        ),
        (
            "labels",
            ValueShape::Block,
            "a",
            "a block",
            Some("write a list: [a, b]"),
        ),
        (
            "details",
            ValueShape::String,
            "FieldMap",
            "\"FieldMap\"",
            Some("write a block: { name value }"),
        ),
        (
            "points",
            ValueShape::Expression,
            "a < 1",
            "an expression",
            None,
        ),
    ];
    for (field, shape, text, given, suggestion) in refused {
        let found = e061(&[(field, shape, text)]);
        assert_eq!(found.len(), 1, "{field} {shape:?}: {found:?}");
        let (message, remedy) = &found[0];
        assert!(
            message.starts_with(&format!("field '{field}' of ticket 't1' is declared "))
                && message.ends_with(&format!("but was given {given}")),
            "{message}"
        );
        assert_eq!(remedy.as_deref(), suggestion, "{field} {shape:?}");
    }

    let admitted = e061(&[
        ("summary", ValueShape::Expression, "a < 1"),
        ("summary", ValueShape::TypeUnion, "a | b"),
        ("labels", ValueShape::Variants, "a, b"),
        ("details", ValueShape::Block, "k"),
    ]);
    assert!(admitted.is_empty(), "{admitted:?}");
}
