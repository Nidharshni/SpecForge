//! Field values against their declared types, through the graph build and
//! the environment's checks every surface (check, watch, LSP) runs.

use specforge_common::Diagnostic;
use specforge_graph::{Graph, build_graph_with_config};
use specforge_parser::FieldValue;
use specforge_project::Environment;
use specforge_protocol_types::{
    EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor, HandshakeResponse,
};
use specforge_test_macros::test as specforge_test;

/// An extension declaring a `ticket` with one field of each checked type.
fn ticket_manifest() -> ExtensionDeclaration {
    let field = |name: &str, field_type: &str, enum_values: &[&str]| FieldDescriptor {
        name: name.into(),
        field_type: field_type.into(),
        enum_values: enum_values.iter().map(|v| v.to_string()).collect(),
        ..FieldDescriptor::default()
    };
    ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: "@test/tickets".into(),
            version: "1.0.0".into(),
            ..HandshakeResponse::default()
        },
        entities: vec![EntityKindDescriptor {
            name: "Ticket".into(),
            keyword: Some("ticket".into()),
            fields: vec![
                field("priority", "enum", &["low", "medium", "high"]),
                field("urgent", "bool", &[]),
                field("points", "integer", &[]),
                field("labels", "string_list", &[]),
                field("summary", "string", &[]),
            ],
            ..EntityKindDescriptor::default()
        }],
        ..ExtensionDeclaration::default()
    }
}

/// Build and check `source` against the ticket extension's registries.
fn build_and_check(source: &str) -> (Graph, Vec<Diagnostic>) {
    let env = Environment::with_registries(
        specforge_registry::build_registries(vec![ticket_manifest()]),
        None,
    );
    let pop_diags: Vec<_> = env.registries.diagnostics().collect();
    assert!(pop_diags.is_empty(), "{pop_diags:?}");
    let parsed = specforge_parser::parse(source, "main.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let (graph, mut diags) = build_graph_with_config(&[parsed], &env.graph_config());
    let entities = env.entity_snapshot(&graph);
    diags.extend(env.run_checks(&graph, &entities));
    (graph, diags)
}

fn e061(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == "E061").collect()
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a value that is not the declared integer, bool or enum type is an error"
)]
fn values_that_are_not_the_declared_bool_or_enum_are_errors() {
    let (_graph, diags) = build_and_check(
        r#"ticket t1 "T" {
  priority urgent
  urgent maybe
  points 3
}
"#,
    );

    let errors = e061(&diags);
    let messages: Vec<&str> = errors.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        vec![
            "field 'priority' of ticket 't1' is declared enum (low, medium, high), but was given urgent",
            "field 'urgent' of ticket 't1' is declared bool, but was given maybe",
        ]
    );
    for d in &errors {
        assert_eq!(d.severity, specforge_common::Severity::Error);
    }
    // Each points at its value.
    let spans: Vec<(usize, usize)> = errors
        .iter()
        .map(|d| {
            let span = d.span.as_ref().unwrap();
            (span.start_line, span.start_col)
        })
        .collect();
    assert_eq!(spans, vec![(2, 12), (3, 10)]);
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "an enum value suggests the closest declared value"
)]
fn an_enum_value_suggests_the_closest_declared_value() {
    let (_graph, diags) = build_and_check(
        r#"ticket t1 "T" {
  priority hgh
}
"#,
    );

    let errors = e061(&diags);
    assert_eq!(errors.len(), 1, "{diags:?}");
    assert_eq!(
        errors[0].suggestion.as_deref(),
        Some("did you mean 'high'?")
    );
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "Check Field Value Types: declared field types hold — registries_populated_fired, single_values_listed, mismatches_diagnosed, undeclared_untouched"
)]
fn check_field_value_types_contract() {
    let (graph, diags) = build_and_check(
        r#"ticket t1 "T" {
  priority "medium"
  urgent "true"
  points "8"
  labels "one"
  summary 42
  note "free text"
}
"#,
    );
    // requires registries_populated: the field types come from the manifest.
    // ensures single_values_listed and coercion: the stored values carry
    // the declared types, with no diagnostic.
    assert!(e061(&diags).is_empty(), "{diags:?}");
    let node = graph.node("t1").unwrap();
    assert!(matches!(node.fields.get("labels"), Some(FieldValue::StringList(l)) if l == &["one"]));
    assert!(matches!(
        node.fields.get("urgent"),
        Some(FieldValue::Boolean(true))
    ));
    assert!(matches!(
        node.fields.get("points"),
        Some(FieldValue::Integer(8))
    ));
    assert!(matches!(node.fields.get("summary"), Some(FieldValue::String(s)) if s == "42"));
    assert!(matches!(node.fields.get("priority"), Some(FieldValue::String(s)) if s == "medium"));
    // ensures undeclared_untouched: an unknown field keeps its value and is
    // W020's, not E061's.
    assert!(matches!(node.fields.get("note"), Some(FieldValue::String(s)) if s == "free text"));
    assert!(
        diags
            .iter()
            .any(|d| d.code == "W020" && d.message.contains("'note'"))
    );

    // ensures mismatches_diagnosed.
    let (_graph, diags) = build_and_check(
        r#"ticket t2 "T" {
  points "many"
  summary ["a", "b"]
}
"#,
    );
    assert_eq!(e061(&diags).len(), 2, "{diags:?}");
}
