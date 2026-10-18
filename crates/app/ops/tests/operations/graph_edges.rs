use super::compiled_support::{
    built, exported, exported_as, query_document, specs, trace_document,
};
use specforge_ops::read::export::Format;
use specforge_test_macros::test as specforge_test;
use std::collections::BTreeSet;

type EdgeSet = BTreeSet<(String, String, String)>;

/// The graph export of a one-file project: its node ids
/// and its (source, target, label) edges.
fn exported_graph(spec: &str) -> (BTreeSet<String>, EdgeSet) {
    let (_project, compiled) = built(&[("main.spec", spec)]);
    let parsed = exported(&compiled);
    let nodes = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect();
    let edges: Vec<(String, String, String)> = parsed["edges"]
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
        .collect();
    let edge_set: EdgeSet = edges.iter().cloned().collect();
    assert_eq!(edge_set.len(), edges.len(), "no duplicate edges: {edges:?}");
    (nodes, edge_set)
}

fn ids(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn edges(list: &[(&str, &str, &str)]) -> EdgeSet {
    list.iter()
        .map(|(s, t, l)| (s.to_string(), t.to_string(), l.to_string()))
        .collect()
}

// --- Phase 1c: Edge types from reference-list fields, DOT labels, multi-hop ---

#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "graph format includes all nodes and edges"
)]
fn the_features_of_two_behaviors_create_edges() {
    let (nodes, edge_set) = exported_graph(
        r#"
feature gamma "G" { problem "p" solution "s" }
behavior alpha "A" { contract "first" features [gamma] }
behavior beta "B" { contract "second" features [gamma] }
"#,
    );
    assert_eq!(nodes, ids(&["alpha", "beta", "gamma"]));
    assert_eq!(
        edge_set,
        edges(&[
            ("alpha", "gamma", "features"),
            ("beta", "gamma", "features"),
        ])
    );
}

#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "graph format includes all nodes and edges"
)]
fn features_field_creates_edges() {
    let (nodes, edge_set) = exported_graph(
        r#"
feature fast_parsing "F" { problem "p" solution "s" }
behavior parse_input "P" {
    contract "The system MUST parse"
    features [fast_parsing]
}
"#,
    );
    assert_eq!(nodes, ids(&["fast_parsing", "parse_input"]));
    assert_eq!(
        edge_set,
        edges(&[("parse_input", "fast_parsing", "features")])
    );
}

#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "graph format includes all nodes and edges"
)]
fn invariants_field_creates_edges() {
    let (nodes, edge_set) = exported_graph(
        r#"
behavior validate "V" { contract "must validate" invariants [refs_resolved] }
invariant refs_resolved "RR" {
    guarantee "All refs MUST resolve"
}
"#,
    );
    assert_eq!(nodes, ids(&["refs_resolved", "validate"]));
    assert_eq!(
        edge_set,
        edges(&[("validate", "refs_resolved", "invariants")])
    );
}

#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "graph format includes all nodes and edges"
)]
fn mitigations_field_creates_edges() {
    let (nodes, edge_set) = exported_graph(
        r#"
behavior parse_input "P" { contract "must parse" }
failure_mode parser_crash "PC" {
    severity 8
    occurrence 2
    detection 3
    cause "Bad input"
    effect "Crash"
    mitigations [parse_input]
}
"#,
    );
    assert_eq!(nodes, ids(&["parse_input", "parser_crash"]));
    assert_eq!(
        edge_set,
        edges(&[("parser_crash", "parse_input", "mitigations")])
    );
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "edge types match relationship semantics"
)]
fn export_graph_edge_labels_are_field_names() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
feature alpha "A" { problem "p" solution "s" }
behavior gamma "G" { contract "first" features [alpha] }
"#,
    )]);

    let parsed = exported(&compiled);
    let edges = parsed["edges"].as_array().unwrap();
    assert_eq!(
        edges[0]["label"], "features",
        "edge label should match field name"
    );
}

#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "edges are labeled with types"
)]
fn dot_export_shows_edge_labels() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
feature alpha "A" { problem "p" solution "s" }
behavior gamma "G" { contract "first" features [alpha] }
"#,
    )]);

    let stdout = exported_as(&compiled, Format::Dot);
    assert!(stdout.contains("digraph"), "should be a DOT digraph");
    assert!(
        stdout.contains("features"),
        "DOT should contain edge label 'features'"
    );
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "trace from entity shows upstream and downstream connections"
)]
fn trace_follows_edges_across_entity_kinds() {
    let (_project, compiled) = built(&[("main.spec", specs::CROSS_REF)]);

    let parsed = trace_document(&compiled, "graph_validation");

    assert_eq!(parsed["entity_id"], "graph_validation");
    let links = |direction: &str| -> BTreeSet<(String, String, String, u64)> {
        parsed[direction]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                (
                    l["entity_id"].as_str().unwrap().to_string(),
                    l["entity_kind"].as_str().unwrap().to_string(),
                    l["edge_label"].as_str().unwrap().to_string(),
                    l["depth"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    let link = |id: &str, kind: &str, label: &str, depth: u64| {
        (id.to_string(), kind.to_string(), label.to_string(), depth)
    };
    // Upstream crosses behavior and failure_mode, each under its owning
    // field: validate_graph -features-> graph_validation,
    // resolve_refs -features-> graph_validation,
    // unresolved_ref -mitigations-> resolve_refs.
    assert_eq!(
        links("upstream"),
        BTreeSet::from([
            link("validate_graph", "behavior", "features", 1),
            link("resolve_refs", "behavior", "features", 1),
            link("unresolved_ref", "failure_mode", "mitigations", 2),
        ])
    );
    // Nothing leaves the feature: the other side is derived, not written.
    assert_eq!(links("downstream"), BTreeSet::new());
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 1 returns direct neighbors"
)]
fn query_depth_2_traverses_multi_hop() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" features [feat_a] }
feature feat_a "F" { problem "p" solution "s" }
journey dev_journey "DJ" { description "workflow" }
"#,
    )]);

    // Query from feat_a at depth 1 should reach alpha
    let parsed = query_document(&compiled, "feat_a", Some(1), &[]);
    let nodes = parsed["nodes"].as_array().unwrap();
    let ids: Vec<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"feat_a"), "root entity should be in results");
    assert!(
        ids.contains(&"alpha"),
        "depth-1 neighbor should be in results"
    );
}

#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "graph format includes all nodes and edges"
)]
fn multiple_reference_fields_produce_separate_edges() {
    let (nodes, edge_set) = exported_graph(
        r#"
behavior validate "V" { contract "must validate" features [feat_a] invariants [inv_a] }
feature feat_a "F" { problem "p" solution "s" }
invariant inv_a "I" { guarantee "always" }
"#,
    );
    assert_eq!(nodes, ids(&["feat_a", "inv_a", "validate"]));
    assert_eq!(
        edge_set,
        edges(&[
            ("validate", "feat_a", "features"),
            ("validate", "inv_a", "invariants"),
        ])
    );
}

#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "nodes are labeled with IDs"
)]
fn dot_export_all_entity_kinds_as_nodes() {
    let (_project, compiled) = built(&[("main.spec", specs::MULTI_EXTENSION)]);

    let stdout = exported_as(&compiled, Format::Dot);
    assert!(stdout.contains("digraph"));

    // Check some entity IDs are present as nodes
    for id in &[
        "parse_input",
        "fast_parsing",
        "use_treesitter",
        "parser_crash",
    ] {
        assert!(stdout.contains(id), "DOT should contain node '{}'", id);
    }
}

#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "nodes are labeled with IDs"
)]
fn dot_export_cross_kind_edges() {
    let (_project, compiled) = built(&[(
        "main.spec",
        r#"
feature alpha "A" { problem "p" solution "s" }
behavior gamma "G" { contract "first" features [alpha] }
failure_mode fm "FM" { severity 1 occurrence 1 detection 1 cause "x" effect "y" mitigations [alpha] }
"#,
    )]);

    let stdout = exported_as(&compiled, Format::Dot);
    // Should have edges from gamma->alpha and fm->alpha
    assert!(stdout.contains("gamma"), "should have gamma node");
    assert!(stdout.contains("alpha"), "should have alpha node");
    assert!(stdout.contains("fm"), "should have fm node");
}

#[specforge_test(
    behavior = "deterministic_output",
    verify = "same input produces identical output across runs"
)]
fn dot_export_deterministic_output() {
    let (_project, compiled) = built(&[("main.spec", specs::MULTI_EXTENSION)]);

    let stdout1 = exported_as(&compiled, Format::Dot);
    let stdout2 = exported_as(&compiled, Format::Dot);
    assert_eq!(stdout1, stdout2, "DOT output should be deterministic");
}
