use super::compiled_support::{
    TestProject, built, checked, exported, exported_as, query_document, specs, stats_of,
};
use specforge_ops::analysis::check::CheckOptions;
use specforge_ops::read::export::Format;
use specforge_test_macros::test as specforge_test;

// --- Phase 1b: All entity kinds through check/export/query/trace/stats ---

#[test]
fn check_accepts_all_software_entity_kinds() {
    let (_project, compiled) = built(&[("main.spec", specs::SOFTWARE)]);

    assert!(checked(&compiled, CheckOptions::default()).ok());
}

#[test]
fn check_accepts_all_product_entity_kinds() {
    // Product spec references parse_input from software, include it
    let combined = format!("{}\n{}", specs::SOFTWARE, specs::PRODUCT);
    let (_project, compiled) = built(&[("main.spec", &combined)]);

    assert!(checked(&compiled, CheckOptions::default()).ok());
}

#[test]
fn check_accepts_all_governance_entity_kinds() {
    // Governance references parse_input via mitigations
    let combined = format!("{}\n{}", specs::SOFTWARE, specs::GOVERNANCE);
    let (_project, compiled) = built(&[("main.spec", &combined)]);

    assert!(checked(&compiled, CheckOptions::default()).ok());
}

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "JSON output contains all nodes"
)]
fn export_graph_includes_all_entity_kinds() {
    let (_project, compiled) = built(&[("main.spec", specs::MULTI_EXTENSION)]);

    let parsed = exported(&compiled);
    let nodes = parsed["nodes"].as_array().unwrap();

    let kinds: Vec<&str> = nodes.iter().map(|n| n["kind"].as_str().unwrap()).collect();
    for expected_kind in &[
        "behavior",
        "invariant",
        "event",
        "type",
        "port",
        "feature",
        "journey",
        "deliverable",
        "milestone",
        "module",
        "term",
        "decision",
        "constraint",
        "failure_mode",
    ] {
        assert!(
            kinds.contains(expected_kind),
            "missing kind '{}' in graph export. found: {:?}",
            expected_kind,
            kinds,
        );
    }
}

#[specforge_test(
    behavior = "export_agent_brief_format",
    verify = "brief format includes only IDs, kinds, titles, and edges"
)]
fn export_brief_includes_all_entity_kinds() {
    let (_project, compiled) = built(&[("main.spec", specs::MULTI_EXTENSION)]);

    let parsed =
        serde_json::from_str::<serde_json::Value>(&exported_as(&compiled, Format::Brief)).unwrap();
    let nodes = parsed["nodes"].as_array().unwrap();

    let kinds: Vec<&str> = nodes.iter().map(|n| n["kind"].as_str().unwrap()).collect();
    for expected_kind in &["behavior", "feature", "decision", "constraint"] {
        assert!(
            kinds.contains(expected_kind),
            "missing kind '{}' in brief",
            expected_kind
        );
    }

    // Only IDs, kinds and titles: no contract, fields or source location.
    for node in nodes {
        let keys: std::collections::BTreeSet<&str> = node
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert!(
            keys.is_subset(&["id", "kind", "title"].into_iter().collect()),
            "brief node carries more than id/kind/title: {node}"
        );
        assert!(keys.contains("id") && keys.contains("kind"), "{node}");
    }
    let parse_input = nodes.iter().find(|n| n["id"] == "parse_input").unwrap();
    assert_eq!(parse_input["kind"], "behavior");
    assert!(parse_input["title"].is_string());

    // And the edges, as the full graph export has them.
    let graph = exported(&compiled);
    assert!(!parsed["edges"].as_array().unwrap().is_empty());
    let edge_set = |v: &serde_json::Value| {
        v["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["source"].as_str().unwrap().to_string(),
                    e["target"].as_str().unwrap().to_string(),
                    e["label"].as_str().unwrap().to_string(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(edge_set(&parsed), edge_set(&graph));
}

#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "context format includes entity IDs and contracts"
)]
fn export_context_shows_contracts_for_all_kinds() {
    let project = TestProject::new()
        .enabling(&specs::BUILTINS)
        .file("main.spec", specs::MULTI_EXTENSION);
    let compiled = project.compile();

    let parsed =
        serde_json::from_str::<serde_json::Value>(&exported_as(&compiled, Format::Context))
            .unwrap();
    let nodes = parsed["nodes"].as_array().unwrap();

    // Behaviors have contracts
    let parse_input = nodes.iter().find(|n| n["id"] == "parse_input").unwrap();
    assert!(
        parse_input.get("contract").is_some(),
        "behavior should have contract in context format"
    );
}

#[test]
fn query_by_kind_filter_product_entities() {
    let (_project, compiled) = built(&[("main.spec", specs::MULTI_EXTENSION)]);

    let parsed = query_document(&compiled, "fast_parsing", Some(0), &["feature"]);
    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["kind"], "feature");
}

#[test]
fn query_by_kind_filter_governance_entities() {
    let (_project, compiled) = built(&[("main.spec", specs::MULTI_EXTENSION)]);

    let parsed = query_document(&compiled, "use_treesitter", Some(0), &["decision"]);
    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["kind"], "decision");
}

#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "stats reports correct entity counts"
)]
fn stats_entities_by_kind_counts_all_kinds() {
    let (_project, compiled) = built(&[("main.spec", specs::MULTI_EXTENSION)]);

    let parsed = serde_json::to_value(stats_of(&compiled)).unwrap();

    // 15 entities total in MULTI_EXTENSION_SPEC (2 behaviors + 13 other kinds)
    let total = parsed["entity_count"].as_u64().unwrap();
    assert_eq!(total, 15, "expected 15 entities, got {}", total);

    let by_kind: std::collections::BTreeMap<&str, u64> = parsed["entity_counts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["kind"].as_str().unwrap(),
                row["count"].as_u64().unwrap(),
            )
        })
        .collect();
    for kind in ["behavior", "feature", "decision"] {
        assert!(
            by_kind.contains_key(kind),
            "missing {kind} in entity_counts"
        );
    }
    assert_eq!(by_kind["behavior"], 2, "expected 2 behaviors");
}

#[test]
fn multi_file_project_with_use_imports() {
    let (_project, compiled) = built(&[
        (
            "behaviors.spec",
            r#"
behavior parse_input "Parse Input" {
    contract "The system MUST parse all valid input"
}
behavior emit_output "Emit Output" {
    contract "The system MUST emit structured output"
}
"#,
        ),
        (
            "features.spec",
            r#"
use "behaviors"

feature fast_parsing "Fast Parsing" {
    problem "Users need quick feedback"
    solution "Incremental parsing"
    behaviors [parse_input]
}
"#,
        ),
    ]);

    // Check succeeds with cross-file references
    assert!(checked(&compiled, CheckOptions::default()).ok());

    // Export includes entities from both files
    let parsed = exported(&compiled);
    let nodes = parsed["nodes"].as_array().unwrap();
    assert!(nodes.len() >= 3, "should have entities from both files");
}
