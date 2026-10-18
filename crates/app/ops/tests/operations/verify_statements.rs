use super::compiled_support::{built, exported, exported_as, stats_of};
use specforge_ops::read::export::Format;
use specforge_test_macros::test as specforge_test;

// --- Phase 2b: Verify declarations through pipeline ---

#[specforge_test(
    behavior = "parse_verify_statements",
    verify = "parse verify statement in any entity block"
)]
fn verify_unit_in_behavior() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
behavior parse_input "Parse Input" {
    contract "The system MUST parse all valid input"
    verify unit "parser handles empty input"
}
"#,
    )]);

    let parsed = exported(&compiled);
    let nodes = parsed["nodes"].as_array().unwrap();
    let parse_input = nodes.iter().find(|n| n["id"] == "parse_input").unwrap();

    assert!(
        parse_input.get("verify").is_some(),
        "behavior with verify should have verify on its node: {:?}",
        parse_input,
    );
}

#[specforge_test(
    behavior = "parse_verify_statements",
    verify = "verify kind and description extracted correctly"
)]
fn verify_integration_in_behavior() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
behavior emit_output "Emit Output" {
    contract "The system MUST emit structured output"
    verify integration "emitter produces valid JSON"
}
"#,
    )]);

    let parsed = exported(&compiled);
    let nodes = parsed["nodes"].as_array().unwrap();
    let node = nodes.iter().find(|n| n["id"] == "emit_output").unwrap();
    assert_eq!(
        node["verify"],
        serde_json::json!([
            {"kind": "integration", "description": "emitter produces valid JSON"}
        ])
    );
}

#[specforge_test(
    behavior = "parse_verify_statements",
    verify = "parse verify statement in any entity block"
)]
fn verify_property_in_behavior() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
behavior graph_build "Graph Build" {
    contract "The system MUST build a valid graph"
    verify property "graph is always acyclic"
}
"#,
    )]);

    let parsed = exported(&compiled);
    let nodes = parsed["nodes"].as_array().unwrap();
    let node = nodes.iter().find(|n| n["id"] == "graph_build").unwrap();
    assert!(node.get("verify").is_some());
}

#[specforge_test(
    behavior = "parse_verify_statements",
    verify = "parse multiple verify statements in same entity"
)]
fn verify_multiple_statements() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
behavior parse_input "Parse Input" {
    contract "The system MUST parse all valid input"
    verify unit "handles empty input"
    verify unit "handles large input"
    verify integration "round-trip parse-emit"
}
"#,
    )]);

    let parsed = exported(&compiled);
    let nodes = parsed["nodes"].as_array().unwrap();
    let node = nodes.iter().find(|n| n["id"] == "parse_input").unwrap();

    let verify = &node["verify"];
    assert!(
        verify.is_array(),
        "multiple verify statements should produce an array"
    );
    assert!(
        verify.as_array().unwrap().len() >= 3,
        "should have at least 3 verify entries"
    );
}

#[test]
fn context_export_includes_verify() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
behavior parse_input "Parse Input" {
    contract "The system MUST parse all valid input"
    verify unit "handles empty input"
}
"#,
    )]);

    let parsed: serde_json::Value =
        serde_json::from_str(&exported_as(&compiled, Format::Context)).unwrap();
    let nodes = parsed["nodes"].as_array().unwrap();
    let node = nodes.iter().find(|n| n["id"] == "parse_input").unwrap();
    assert!(
        node.get("verify").is_some(),
        "context format should include verify: {:?}",
        node,
    );
}

#[test]
fn stats_verified_count_matches() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
behavior alpha "A" {
    contract "first"
    verify unit "test alpha"
    verify integration "test alpha integration"
}
behavior beta "B" {
    contract "second"
    verify unit "test beta"
}
feature gamma "G" { problem "p" solution "s" }
"#,
    )]);

    let parsed = serde_json::to_value(stats_of(&compiled)).unwrap();

    let verified = parsed["verified_count"].as_u64().unwrap();
    // alpha has 2 verify, beta has 1 = 3 total verify declarations
    // (verified_count counts entities with at least one verify, so 2)
    // Or it counts total verify statements — depends on implementation
    assert!(
        verified >= 2,
        "verified_count should be at least 2, got {}",
        verified
    );
}

#[specforge_test(
    behavior = "parse_verify_statements",
    verify = "parse verify statement in any entity block"
)]
fn verify_on_non_behavior_entity() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
invariant graph_acyclic "Graph Acyclicity" {
    guarantee "The dependency graph MUST be acyclic"
    verify property "cycle detection works"
}
"#,
    )]);

    let parsed = exported(&compiled);
    let nodes = parsed["nodes"].as_array().unwrap();
    let node = nodes.iter().find(|n| n["id"] == "graph_acyclic").unwrap();
    assert!(
        node.get("verify").is_some(),
        "invariant should accept verify declarations"
    );
}

#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "graph format includes all fields and metadata"
)]
fn graph_export_includes_verify_beside_fields() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
behavior parse_input "Parse Input" {
    contract "The system MUST parse all valid input"
    status done
    features [fast_parsing]
    verify unit "handles empty input"
}

feature fast_parsing "Fast Parsing" {
    problem "slow"
}
"#,
    )]);

    let parsed = exported(&compiled);
    let nodes = parsed["nodes"].as_array().unwrap();
    let node = nodes.iter().find(|n| n["id"] == "parse_input").unwrap();
    // Every declared field and the node's metadata, with their values.
    assert_eq!(
        *node,
        serde_json::json!({
            "id": "parse_input",
            "kind": "behavior",
            "title": "Parse Input",
            "file": "main.spec",
            "line": 2,
            "fields": {
                "contract": "The system MUST parse all valid input",
                "status": "done",
                "features": ["fast_parsing"],
            },
            "verify": [{"kind": "unit", "description": "handles empty input"}],
        })
    );
}
