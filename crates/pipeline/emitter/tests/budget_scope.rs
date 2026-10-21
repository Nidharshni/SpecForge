//! A token budget and a scoped export (plan 07, 07-D1): the scope entity is
//! the export's subject, so the budget cuts every other entity first.

use serde_json::Value;
use specforge_diagnostics::codes;
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

#[specforge_test_macros::test(
    behavior = "enforce_token_budget",
    verify = "a token budget never cuts the scope entity of a scoped export"
)]
fn a_budget_never_cuts_the_scope_entity() {
    let (graph, _) = specforge_graph::build_graph(&[specforge_parser::parse(SRC, "main.spec")]);
    let selected = graph.select(Some("leaf"), None, &[]).unwrap();
    let output = emit(
        &graph,
        &EmitOptions {
            format: EmitFormat::Brief,
            selected: Some(&selected),
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

    let selected = graph.select(Some("hub"), None, &["behavior"]).unwrap();
    let output = emit(
        &graph,
        &EmitOptions {
            format: EmitFormat::Brief,
            selected: Some(&selected),
            token_budget: Some(80),
            ..Default::default()
        },
    )
    .unwrap();
    let document: Value = serde_json::from_str(&output).unwrap();
    assert!(ids(&document, "nodes").contains(&"hub".to_string()));
}

#[specforge_test_macros::test(
    behavior = "enforce_token_budget",
    verify = "a scoped export whose budget cannot hold its scope entity alone fails with E062"
)]
fn a_budget_below_the_scope_entity_alone_is_e062() {
    let (graph, _) = specforge_graph::build_graph(&[specforge_parser::parse(SRC, "main.spec")]);
    let selected = graph.select(Some("leaf"), None, &[]).unwrap();
    let error = emit(
        &graph,
        &EmitOptions {
            format: EmitFormat::Brief,
            selected: Some(&selected),
            token_budget: Some(40),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code(), Some(codes::E062));
    assert!(error.to_string().contains("scope entity `leaf` alone"));
}
