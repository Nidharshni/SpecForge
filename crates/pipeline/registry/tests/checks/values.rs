//! E061: a field value that cannot be the type its extension declared, read from the value as the
//! graph build stored it, with the span it is written at. Every test starts from `.spec` text
//! (ADR 0115).

use specforge_common::Sym;
use specforge_registry::entity::EntityRecord;
use specforge_test_macros::test as spec;

use crate::support::written::{checked, tickets};
use crate::support::{build, check, coded_in, span};

/// The E061 messages and suggestions of the checks over `ticket t1 "T"` written with `body`.
fn e061(body: &str) -> Vec<(String, Option<String>)> {
    let text = format!("ticket t1 \"T\" {{\n{body}}}\n");
    let diags = checked(&build([tickets()]), &[("main.spec", &text)]);
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
    let found = e061("  priority urgent\n  urgent maybe\n  points \"many\"\n");
    let messages: Vec<&str> = found.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(
        messages,
        [
            "field 'priority' of ticket 't1' is declared enum (low, medium, high), but was given urgent",
            "field 'urgent' of ticket 't1' is declared bool, but was given maybe",
            "field 'points' of ticket 't1' is declared integer, but was given \"many\"",
        ]
    );
    // A bool's remedy is the two literals.
    assert_eq!(found[1].1.as_deref(), Some("use true or false"));
}

#[test]
fn values_of_the_declared_types_are_no_diagnostic() {
    let found = e061(
        "  priority \"medium\"\n  priority high\n  urgent true\n  points 8\n  labels [one]\n  \
         summary \"free text\"\n  blocker t2\n  details {\n    k \"x\"\n  }\n  \
         summary expr { a < 1 }\n  undeclared {\n    a \"x\"\n  }\n",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[spec(
    behavior = "check_field_value_types",
    verify = "an enum value suggests the closest declared value"
)]
fn an_enum_value_suggests_the_closest_declared_value() {
    let found = e061("  priority hgh\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].1.as_deref(), Some("did you mean 'high'?"));
}

#[spec(
    behavior = "check_field_value_types",
    verify = "a list on a field declared as a single value is an error"
)]
fn a_list_on_a_single_value_field_is_an_error() {
    let found = e061("  summary [a]\n  blocker [t2]\n  priority [low]\n");
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
    let found = e061(
        "  points {\n    a \"x\"\n  }\n  points expr { a < 1 }\n  points string | int\n  \
         points 2026-10-07\n  points [1, x]\n",
    );
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
    let text = "widget w1 \"W\" {\n  points \"many\"\n}\n";
    let diags = checked(&build([tickets()]), &[("main.spec", text)]);
    assert!(coded_in(&diags, "E061").is_empty(), "{diags:?}");
}

/// The error is at the value's own span, else at the entity.
#[test]
fn e061_points_at_the_value_else_the_entity() {
    // Written: the parser gives every value it reads a span.
    let text = "ticket t1 \"T\" {\n  points \"many\"\n}\n";
    let diags = checked(&build([tickets()]), &[("main.spec", text)]);
    let e061 = coded_in(&diags, "E061");
    assert_eq!(e061.len(), 1, "{diags:?}");
    let s = e061[0].span.as_ref().unwrap();
    assert_eq!((s.start_line, s.end_line), (2, 2), "{s:?}");
    assert!(s.start_col < s.end_col, "{s:?}");

    // A record shape no parse makes: a field without a value span falls back to the entity. The
    // parser synthesizes fields with none (`FieldMap::push`), but never one a ticket can mistype.
    let mut record = EntityRecord::new("ticket", "t1", span("main.spec")).with_value(
        "points",
        specforge_graph::ValueShape::String,
        "many",
    );
    record.fields[0].value_span = None;
    let diags = check(&build([tickets()]), &[record]);
    let e061 = coded_in(&diags, "E061");
    assert_eq!(e061.len(), 1, "{diags:?}");
    let s = e061[0].span.as_ref().unwrap();
    assert_eq!(
        (s.file, s.start_line, s.start_col),
        (Sym::new("main.spec"), 1, 0)
    );
}

/// Each row is one value written on one field: the field, the text, what the message says was
/// given, and the remedy E061 suggests.
#[spec(
    behavior = "check_field_value_types",
    verify = "a value of a shape its declared type never takes is an error"
)]
fn a_value_of_a_shape_its_type_never_takes_is_an_error() {
    let block = "{\n    a \"x\"\n  }";
    let refused: [(&str, &str, &str, Option<&str>); 6] = [
        ("summary", block, "a block", Some("write the value as text")),
        (
            "blocker",
            block,
            "a block",
            Some("name one entity by its id"),
        ),
        (
            "blocker",
            "expr { a < 1 }",
            "an expression",
            Some("name one entity by its id"),
        ),
        ("labels", block, "a block", Some("write a list: [a, b]")),
        (
            "details",
            "\"FieldMap\"",
            "\"FieldMap\"",
            Some("write a block: { name value }"),
        ),
        ("points", "expr { a < 1 }", "an expression", None),
    ];
    for (field, written, given, suggestion) in refused {
        let found = e061(&format!("  {field} {written}\n"));
        assert_eq!(found.len(), 1, "{field} {written}: {found:?}");
        let (message, remedy) = &found[0];
        assert!(
            message.starts_with(&format!("field '{field}' of ticket 't1' is declared "))
                && message.ends_with(&format!("but was given {given}")),
            "{message}"
        );
        assert_eq!(remedy.as_deref(), suggestion, "{field} {written}");
    }

    let admitted =
        e061("  summary expr { a < 1 }\n  summary a | b\n  details {\n    k \"x\"\n  }\n");
    assert!(admitted.is_empty(), "{admitted:?}");
}
