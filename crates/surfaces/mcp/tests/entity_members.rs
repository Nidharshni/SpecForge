//! What an entity holds is written once (ADR 0222): inspect and the context prompt carry an entity's
//! methods and its fields' annotations as the graph export writes them.

use crate::support::*;
use serde_json::{Value, json};
use specforge_test::prelude::*;

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

fn served() -> Served {
    TestProject::new()
        .file("a.spec", TEXT)
        .serve(&[TestExtension::software()])
}

fn tool(server: &mut Served, name: &str, args: Value) -> Value {
    serde_json::from_str(&tool_text(&call_tool(server, name, args))).unwrap()
}

#[specforge_test(
    behavior = "inspect_entity_facts",
    verify = "an entity's facts carry its methods and its fields' annotations as the graph export writes them"
)]
fn inspect_answers_methods_and_annotations() {
    let mut server = served();
    for (id, key) in [("order_store", "methods"), ("Order", "annotations")] {
        let inspected = tool(&mut server, "specforge.inspect", json!({"entity_id": id}));
        let queried = tool(
            &mut server,
            "specforge.query",
            json!({"entity_id": id, "depth": 0}),
        );
        assert_eq!(
            queried["nodes"][0][key], inspected[key],
            "{id}: {inspected}"
        );
        assert!(inspected[key].is_array() || inspected[key].is_object());
    }
}

#[specforge_test(
    behavior = "provide_mcp_context_prompt",
    verify = "the context prompt carries the entity's methods and its fields' annotations, a structural constraint's too"
)]
fn the_context_prompt_carries_methods_and_annotations() {
    let mut server = served();
    let resp = get_prompt(
        &mut server,
        "specforge://prompts/context",
        json!({"entity_id": "order_store", "structural_constraints": ["Order"]}),
    );
    let payload: Value = serde_json::from_str(&prompt_text(&resp)).unwrap();
    assert_eq!(payload["methods"][0]["name"], "save", "{payload}");
    assert_eq!(
        payload["structural_constraint_entities"][0]["annotations"]["id"],
        json!([{"name": "optional"}, {"name": "readonly"}]),
        "{payload}"
    );
}
