//! The schema operation (`specforge schema`, `specforge.schema`): the
//! document it answers and the JSON Schema it publishes. Moved from the
//! emitter's serve-schema tests when the CLI began printing the
//! operation's outcome (ADR 0027's round, ADR 0015 D8).

use serde_json::{Value, json};
use specforge_emitter::GraphProtocolSchema;
use specforge_ops::export::Format;
use specforge_ops::schema::{SchemaRequest, json_schema, schema};
use specforge_protocol_types::{
    EdgeTypeDescriptor, EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor,
    HandshakeResponse, ValidationRuleDescriptor,
};
use specforge_registry::build_registries;
use specforge_test_macros::test as specforge_test;

use crate::view_support::Project;

/// A project whose one extension, `@t/soft` 1.0.0, declares `behavior`
/// (testable, a `contract` and a `features` reference), `feature` and
/// `event`; `implements` runs behavior → feature, `emits` feature →
/// event; one validation rule targets behavior.
fn project() -> Project {
    let field = |name: &str, field_type: &str| FieldDescriptor {
        name: name.into(),
        field_type: field_type.into(),
        ..FieldDescriptor::default()
    };
    let edge = |label: &str, source: &str, target: &str| EdgeTypeDescriptor {
        label: label.into(),
        source_kind: Some(source.into()),
        target_kind: Some(target.into()),
        ..EdgeTypeDescriptor::default()
    };
    let declaration = ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: "@t/soft".into(),
            version: "1.0.0".into(),
            ..HandshakeResponse::default()
        },
        entities: vec![
            EntityKindDescriptor {
                name: "behavior".into(),
                testable: true,
                supports_verify: true,
                fields: vec![
                    field("contract", "string"),
                    FieldDescriptor {
                        edge: Some("implements".into()),
                        target_kind: Some("feature".into()),
                        ..field("features", "reference_list")
                    },
                ],
                ..EntityKindDescriptor::default()
            },
            EntityKindDescriptor {
                name: "feature".into(),
                ..EntityKindDescriptor::default()
            },
            EntityKindDescriptor {
                name: "event".into(),
                ..EntityKindDescriptor::default()
            },
        ],
        edges: vec![
            edge("implements", "behavior", "feature"),
            edge("emits", "feature", "event"),
        ],
        validation_rules: vec![ValidationRuleDescriptor {
            code: "W901".into(),
            message_template: "{id} has no contract".into(),
            check: "field_required".into(),
            target_kind: Some("behavior".into()),
            field: Some("contract".into()),
            ..ValidationRuleDescriptor::default()
        }],
        ..ExtensionDeclaration::default()
    };
    Project::new("", build_registries(vec![declaration]))
}

/// The labels of a document's edge types.
fn labels(document: &Value) -> Vec<&str> {
    let mut labels: Vec<&str> = document["edge_types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|edge| edge["label"].as_str().unwrap())
        .collect();
    labels.sort_unstable();
    labels
}

#[specforge_test(
    behavior = "serve_schema_resource",
    verify = "specforge schema outputs full schema as JSON"
)]
fn the_outcome_serializes_the_whole_schema() {
    let project = project();
    let outcome = schema(&project.view(), &SchemaRequest::default()).unwrap();
    let text = serde_json::to_string_pretty(&outcome).unwrap();
    let parsed: Value = serde_json::from_str(&text).unwrap();

    assert_eq!(
        parsed["extensions"],
        json!([{ "name": "@t/soft", "version": "1.0.0" }])
    );
    let mut kinds: Vec<&str> = parsed["entity_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["name"].as_str().unwrap())
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, ["behavior", "event", "feature"]);
    let behavior = parsed["entity_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["name"] == "behavior")
        .unwrap();
    assert_eq!(behavior["source_extension"], "@t/soft");
    assert_eq!(behavior["testable"], true);
    assert!(
        behavior["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["name"] == "contract" && f["field_type"] == "string"),
        "{behavior}"
    );
    assert_eq!(labels(&parsed), ["emits", "implements"]);
    assert!(parsed.get("validation_rules").is_none(), "not asked for");

    // Nothing is lost: it reads back as the schema, and is the schema's own
    // serialization, keys in order.
    let back: GraphProtocolSchema = serde_json::from_str(&text).unwrap();
    assert_eq!(back, outcome.schema);
    assert_eq!(text, serde_json::to_string_pretty(&outcome.schema).unwrap());
}

#[specforge_test(
    behavior = "serve_schema_resource",
    verify = "--kind filter restricts to single entity kind"
)]
fn a_kind_keeps_its_entry_and_the_edges_that_touch_it() {
    let project = project();
    for (kind, edges) in [
        ("behavior", vec!["implements"]),
        ("feature", vec!["emits", "implements"]),
        ("event", vec!["emits"]),
    ] {
        let request = SchemaRequest {
            kind: Some(kind),
            ..SchemaRequest::default()
        };
        let document = serde_json::to_value(schema(&project.view(), &request).unwrap()).unwrap();
        let entries = document["entity_kinds"].as_array().unwrap();
        assert_eq!(entries.len(), 1, "{kind}: {document}");
        assert_eq!(entries[0]["name"], kind);
        assert_eq!(labels(&document), edges, "{kind}");
    }
}

#[specforge_test(behavior = "serve_schema_resource", verify = "missing kind error")]
fn an_unknown_kind_is_refused_naming_the_closest() {
    let project = project();
    let request = SchemaRequest {
        kind: Some("behaviour"),
        ..SchemaRequest::default()
    };
    let error = schema(&project.view(), &request).unwrap_err();
    assert_eq!(error.error.code, "unknown_kind");
    assert_eq!(error.error.message, "unknown entity kind 'behaviour'");
    assert_eq!(
        error.error.suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
}

#[specforge_test(
    behavior = "serve_schema_resource",
    verify = "Serve Schema Resource: schema resource serving holds — validation_complete_fired, full_schema_output, kind_filter_supported, mcp_resource_available"
)]
fn serve_schema_contract() {
    let project = project();
    let view = project.view();

    // ensures: full_schema_output
    let full = serde_json::to_value(schema(&view, &SchemaRequest::default()).unwrap()).unwrap();
    assert_eq!(full["entity_kinds"].as_array().unwrap().len(), 3);
    assert!(full["edge_types"].is_array());

    // ensures: kind_filter_supported
    let request = SchemaRequest {
        kind: Some("behavior"),
        ..SchemaRequest::default()
    };
    let filtered = serde_json::to_value(schema(&view, &request).unwrap()).unwrap();
    assert_eq!(filtered["entity_kinds"][0]["name"], "behavior");

    // Without edges, with the rules: what include_edges and
    // include_validation_rules select.
    let request = SchemaRequest {
        edges: false,
        validation_rules: true,
        ..SchemaRequest::default()
    };
    let trimmed = serde_json::to_value(schema(&view, &request).unwrap()).unwrap();
    assert!(trimmed.get("edge_types").is_none(), "{trimmed}");
    assert_eq!(trimmed["validation_rules"][0]["code"], "W901");
    assert_eq!(trimmed["validation_rules"][0]["extension"], "@t/soft");

    // error for unknown kind
    let request = SchemaRequest {
        kind: Some("nonexistent"),
        ..SchemaRequest::default()
    };
    assert!(schema(&view, &request).is_err());
}

#[test]
fn json_schema_refuses_dot() {
    let project = project();
    let error = json_schema(&project.view(), Format::Dot).unwrap_err();
    assert_eq!(error.code, "invalid_input");
    assert_eq!(
        error.message,
        "Unknown format: dot. Expected: graph, context, brief"
    );

    let published: Value = json_schema(&project.view(), Format::Graph).unwrap();
    assert_eq!(
        published["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn schema_states_which_field_a_refusal_is_about() {
    use specforge_ops::schema::SchemaField;
    let request = SchemaRequest {
        kind: Some("behaviour"),
        ..SchemaRequest::default()
    };
    let refusal = schema(&project().view(), &request).unwrap_err();
    assert_eq!(refusal.subject, Some(SchemaField::Kind));
}
