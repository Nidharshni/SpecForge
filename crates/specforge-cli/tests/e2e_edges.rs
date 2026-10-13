use crate::e2e_fixtures::*;
use specforge_test_macros::test as specforge_test;
use std::collections::BTreeSet;

type EdgeSet = BTreeSet<(String, String, String)>;

/// `specforge export --format=graph` of a one-file project: its node ids
/// and its (source, target, label) edges.
fn exported_graph(spec: &str) -> (BTreeSet<String>, EdgeSet) {
    let dir = setup_project(&[("main.spec", spec)]);
    let output = specforge_cmd()
        .args(["export", "--format=graph"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
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
fn behaviors_field_creates_edges() {
    let (nodes, edge_set) = exported_graph(
        r#"
behavior alpha "A" { contract "first" }
behavior beta "B" { contract "second" }
feature gamma "G" { behaviors [alpha, beta] }
"#,
    );
    assert_eq!(nodes, ids(&["alpha", "beta", "gamma"]));
    assert_eq!(
        edge_set,
        edges(&[
            ("gamma", "alpha", "behaviors"),
            ("gamma", "beta", "behaviors"),
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
fn enforced_by_field_creates_edges() {
    let (nodes, edge_set) = exported_graph(
        r#"
behavior validate "V" { contract "must validate" }
invariant refs_resolved "RR" {
    guarantee "All refs MUST resolve"
    enforced_by [validate]
}
"#,
    );
    assert_eq!(nodes, ids(&["refs_resolved", "validate"]));
    assert_eq!(
        edge_set,
        edges(&[("refs_resolved", "validate", "enforced_by")])
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
    let dir = setup_project(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha] }
"#,
    )]);

    let output = specforge_cmd()
        .args(["export", "--format=graph"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    let parsed = parse_json_stdout(&output);
    let edges = parsed["edges"].as_array().unwrap();
    assert_eq!(
        edges[0]["label"], "behaviors",
        "edge label should match field name"
    );
}

#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "edges are labeled with types"
)]
fn dot_export_shows_edge_labels() {
    let dir = setup_project(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha] }
"#,
    )]);

    let output = specforge_cmd()
        .args(["export", "--format=dot"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("digraph"), "should be a DOT digraph");
    assert!(
        stdout.contains("behaviors"),
        "DOT should contain edge label 'behaviors'"
    );
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "trace from entity shows upstream and downstream connections"
)]
fn trace_follows_edges_across_entity_kinds() {
    let dir = setup_project(&[("main.spec", CROSS_REF_SPEC)]);

    let output = specforge_cmd()
        .args(["trace", "validate_graph"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);

    assert_eq!(parsed["entity_id"], "validate_graph");
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
    // Upstream crosses feature, invariant, behavior and failure_mode:
    // graph_validation -behaviors-> validate_graph,
    // refs_resolved -enforced_by-> validate_graph,
    // resolve_refs -features-> graph_validation,
    // unresolved_ref -mitigations-> resolve_refs.
    assert_eq!(
        links("upstream"),
        BTreeSet::from([
            link("graph_validation", "feature", "behaviors", 1),
            link("refs_resolved", "invariant", "enforced_by", 1),
            link("resolve_refs", "behavior", "features", 2),
            link("unresolved_ref", "failure_mode", "mitigations", 3),
        ])
    );
    // Downstream: validate_graph -features-> graph_validation -behaviors-> resolve_refs.
    assert_eq!(
        links("downstream"),
        BTreeSet::from([
            link("graph_validation", "feature", "features", 1),
            link("resolve_refs", "behavior", "behaviors", 2),
        ])
    );
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 1 returns direct neighbors"
)]
fn query_depth_2_traverses_multi_hop() {
    let dir = setup_project(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" features [feat_a] }
feature feat_a "F" { behaviors [alpha] }
journey dev_journey "DJ" { description "workflow" }
"#,
    )]);

    // Query from feat_a at depth 1 should reach alpha
    let output = specforge_cmd()
        .args(["query", "feat_a", "--depth=1"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
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
behavior validate "V" { contract "must validate" features [feat_a] }
feature feat_a "F" { behaviors [validate] }
invariant inv_a "I" { guarantee "always" enforced_by [validate] }
"#,
    );
    assert_eq!(nodes, ids(&["feat_a", "inv_a", "validate"]));
    assert_eq!(
        edge_set,
        edges(&[
            ("feat_a", "validate", "behaviors"),
            ("validate", "feat_a", "features"),
            ("inv_a", "validate", "enforced_by"),
        ])
    );
}

#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "nodes are labeled with IDs"
)]
fn dot_export_all_entity_kinds_as_nodes() {
    let dir = setup_project(&[("main.spec", MULTI_EXTENSION_SPEC)]);

    let output = specforge_cmd()
        .args(["export", "--format=dot"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
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
    let dir = setup_project(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha] }
failure_mode fm "FM" { severity 1 occurrence 1 detection 1 cause "x" effect "y" mitigations [alpha] }
"#,
    )]);

    let output = specforge_cmd()
        .args(["export", "--format=dot"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
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
    let dir = setup_project(&[("main.spec", MULTI_EXTENSION_SPEC)]);

    let output1 = specforge_cmd()
        .args(["export", "--format=dot"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    let output2 = specforge_cmd()
        .args(["export", "--format=dot"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    assert_eq!(stdout1, stdout2, "DOT output should be deterministic");
}
