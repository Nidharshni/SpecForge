//! A token budget and a scoped export (plan 07, 07-D1): the scope entity is
//! the export's subject, so the budget cuts every other entity first.

use serde_json::Value;
use specforge_emitter::{EmitFormat, EmitOptions, emit, estimate_tokens};

const SRC: &str = r#"
invariant hub "Hub invariant with a long title to cost tokens" { guarantee "g" }
behavior leaf "Leaf" { contract "c" invariants [hub] }
behavior b1 "B1" { contract "c" invariants [hub] }
behavior b2 "B2" { contract "c" invariants [hub] }
"#;

fn ids(document: &Value, key: &str) -> Vec<String> {
    document[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["id"].as_str().unwrap().to_string())
        .collect()
}

/// Pins today's defect: the budget ranks the scope entity like any other and
/// cuts it. Flipped by the fix.
#[test]
#[ignore = "flipped by T3"]
fn a_budget_never_cuts_the_scope_entity() {
    let (graph, _) = specforge_graph::build_graph(&[specforge_parser::parse(SRC, "main.spec")]);
    let output = emit(
        &graph,
        &EmitOptions {
            format: EmitFormat::Brief,
            scope: Some("leaf"),
            token_budget: Some(80),
            ..Default::default()
        },
    )
    .unwrap();
    let document: Value = serde_json::from_str(&output).unwrap();
    assert!(ids(&document, "nodes").contains(&"leaf".to_string()));
    let truncated = document["token_budget"]["truncated_entities"]
        .as_array()
        .unwrap();
    assert!(!truncated.iter().any(|id| id == "leaf"));
    assert!(estimate_tokens(&output) <= 80);

    let output = emit(
        &graph,
        &EmitOptions {
            format: EmitFormat::Brief,
            scope: Some("hub"),
            kind_filter: vec!["behavior"],
            token_budget: Some(50),
            ..Default::default()
        },
    )
    .unwrap();
    let document: Value = serde_json::from_str(&output).unwrap();
    assert!(ids(&document, "nodes").contains(&"hub".to_string()));
}
