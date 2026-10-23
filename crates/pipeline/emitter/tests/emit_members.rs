//! What an entity holds is written once (ADR 0222): the graph, context and markdown exports write an
//! entity's methods and its fields' annotations.

use serde_json::{Value, json};
use specforge_emitter::EmitFormat;
use specforge_graph::{Graph, Node};
use specforge_test_macros::test as specforge_test;

use crate::support::{build_of, field, kind};

const TEXT: &str = r#"type Order "Order" {
  id string @optional @readonly
  note string @since "1.2"
  address {
    street string @optional
  }
}
port order_store "Order store" {
  direction outbound
  method save(order: Order @optional, force?: bool) -> Result<void, never>
}
"#;

/// The graph of [`TEXT`]: each parsed entity as the node the build keeps, with its methods.
fn graph() -> Graph {
    let mut graph = Graph::new();
    for entity in specforge_parser::parse(TEXT, "a.spec").entities {
        graph.add_node(Node {
            title: entity.title,
            fields: entity.fields,
            methods: entity.methods,
            ..Node::new(
                entity.id.raw.as_str(),
                entity.kind.raw.as_str(),
                entity.span,
            )
        });
    }
    graph
}

fn node(export: &str, id: &str) -> Value {
    let parsed: Value = serde_json::from_str(export).unwrap();
    parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id)
        .unwrap_or_else(|| panic!("{id} in {export}"))
        .clone()
}

fn save() -> Value {
    json!([{
        "name": "save",
        "params": [
            {"name": "order", "type": "Order", "annotations": [{"name": "optional"}]},
            {"name": "force", "type": "bool", "optional": true},
        ],
        "returns": "Result<void, never>",
    }])
}

#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "graph format writes each entity's methods and its fields' annotations"
)]
fn a_graph_node_writes_its_methods_and_its_fields_annotations() {
    let export = crate::export(&graph(), EmitFormat::Json, None);
    let port = node(&export, "order_store");
    assert_eq!(port["methods"], save(), "{port}");
    assert!(port.get("annotations").is_none(), "{port}");
    let order = node(&export, "Order");
    assert_eq!(
        order["annotations"],
        json!({
            "address.street": [{"name": "optional"}],
            "id": [{"name": "optional"}, {"name": "readonly"}],
            "note": [{"name": "since", "value": "1.2"}],
        }),
        "{order}"
    );
    assert_eq!(
        order["fields"],
        json!({"address": {"street": "string"}, "id": "string", "note": "string"})
    );
    assert!(order.get("methods").is_none(), "{order}");
}

#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "context format keeps an entity's methods and the annotations of the fields it keeps"
)]
fn a_context_node_keeps_its_methods_and_its_fields_annotations() {
    let build = build_of(vec![
        kind("type", vec![field("note", true, false)]),
        kind("port", vec![]),
    ]);
    let export = crate::export(&graph(), EmitFormat::Context, Some(&build));
    assert_eq!(node(&export, "order_store")["methods"], save());
    assert_eq!(
        node(&export, "Order")["annotations"],
        json!({"note": [{"name": "since", "value": "1.2"}]})
    );
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "a section lists its methods, and each field's annotations follow its value"
)]
fn a_section_lists_methods_and_annotations() {
    let export = crate::export(&graph(), EmitFormat::Markdown, None);
    assert!(
        export.contains(
            "**Methods**\n\n- `save(order: Order @optional, force?: bool) -> Result<void, never>`"
        ),
        "{export}"
    );
    assert!(
        export.contains("| id | string @optional @readonly |"),
        "{export}"
    );
    assert!(
        export.contains("| note | string @since \"1.2\" |"),
        "{export}"
    );
    assert!(export.contains("- street: string @optional"), "{export}");
}
