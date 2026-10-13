use crate::e2e_fixtures::*;
use specforge_test_macros::test as specforge_test;

// --- Query depth tests ---

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 0 returns only the target entity"
)]
fn query_depth_0_returns_only_root() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    let output = specforge_cmd()
        .args(["query", "beh_middle", "--depth=0"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1, "depth 0 should return only the root entity");
    assert_eq!(nodes[0]["id"], "beh_middle");
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 0 returns only the target entity"
)]
fn query_depth_0_edges_behavior() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    let output = specforge_cmd()
        .args(["query", "beh_middle", "--depth=0"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let edges = parsed["edges"].as_array().unwrap();
    // With only one node, no edges can survive (both endpoints must be present)
    assert_eq!(edges.len(), 0, "depth 0 single node should have no edges");
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "multiple kind filters combine as union"
)]
fn query_multiple_kind_filters() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    let output = specforge_cmd()
        .args([
            "query",
            "beh_middle",
            "--depth=2",
            "--kind=behavior",
            "--kind=invariant",
        ])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();

    let kinds: Vec<&str> = nodes.iter().map(|n| n["kind"].as_str().unwrap()).collect();
    for kind in &kinds {
        assert!(
            *kind == "behavior" || *kind == "invariant",
            "all nodes should be behavior or invariant, got '{}'",
            kind
        );
    }
    assert!(nodes.len() >= 2, "should include at least root + inv_deep");
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "kind filter restricts results to specified entity kinds"
)]
fn query_kind_filter_no_match_returns_root_only() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    let output = specforge_cmd()
        .args([
            "query",
            "beh_middle",
            "--depth=2",
            "--kind=nonexistent_kind",
        ])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(
        nodes.len(),
        1,
        "only root should survive when kind filter matches nothing"
    );
    assert_eq!(nodes[0]["id"], "beh_middle");
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "kind filter restricts results to specified entity kinds"
)]
fn query_kind_filter_prunes_edges() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    // Query with kind=behavior should prune edges to non-behavior nodes
    let output = specforge_cmd()
        .args(["query", "beh_middle", "--depth=3", "--kind=behavior"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();
    let edges = parsed["edges"].as_array().unwrap();

    let node_ids: Vec<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();

    // Unfiltered, depth 3 reaches feat_root and inv_deep; --kind=behavior
    // keeps only behaviors.
    assert_eq!(node_ids, ["beh_middle"], "{parsed}");
    for node in nodes {
        assert_eq!(node["kind"], "behavior", "{node}");
    }
    assert!(edges.is_empty(), "edges to filtered-out nodes are pruned");
    let unfiltered = specforge_cmd()
        .args(["query", "beh_middle", "--depth=3"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    let unfiltered = parse_json_stdout(&unfiltered);
    assert_eq!(unfiltered["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(unfiltered["edges"].as_array().unwrap().len(), 3);

    // Every edge endpoint must reference an existing node
    for edge in edges {
        let source = edge["source"].as_str().unwrap();
        let target = edge["target"].as_str().unwrap();
        assert!(
            node_ids.contains(&source),
            "edge source '{}' not in filtered nodes {:?}",
            source,
            node_ids
        );
        assert!(
            node_ids.contains(&target),
            "edge target '{}' not in filtered nodes {:?}",
            target,
            node_ids
        );
    }
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "an unknown --kind is reported with I020 and the closest kind"
)]
fn query_reports_an_unknown_kind() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    let output = specforge_cmd()
        .args(["query", "beh_middle", "--kind=behaviour", "--kind=behavior"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    // The notice is on stderr; the document on stdout is the behavior-only
    // query, unchanged by it.
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr,
        "info[I020]: unknown entity kind 'behaviour'\n  = help: did you mean 'behavior'?\n"
    );
    let parsed = parse_json_stdout(&output);
    let ids: Vec<&str> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["beh_middle"], "{parsed}");
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth 1 returns direct neighbors"
)]
fn query_isolated_entity_depth_1() {
    let spec = format!("{DEEP_CHAIN_SPEC}\n{ISOLATED_SPEC}");
    let dir = setup_project(&[("main.spec", &spec)]);
    let query = |id: &str| {
        let output = specforge_cmd()
            .args(["query", id, "--depth=1"])
            .arg("--path")
            .arg(dir.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        let parsed = parse_json_stdout(&output);
        let mut ids: Vec<String> = parsed["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap().to_string())
            .collect();
        ids.sort();
        ids
    };

    // inv_deep's one neighbor is beh_middle; feat_root is two hops away.
    assert_eq!(query("inv_deep"), ["beh_middle", "inv_deep"]);
    // beh_middle neighbors both feat_root and inv_deep.
    assert_eq!(query("beh_middle"), ["beh_middle", "feat_root", "inv_deep"]);
    // An isolated node has no neighbors.
    assert_eq!(query("isolated_node"), ["isolated_node"]);
}

#[specforge_test(
    invariant = "graph_traversal_integrity",
    verify = "traversal from any node visits every reachable node exactly once"
)]
fn query_handles_cycles() {
    let dir = setup_project(&[("main.spec", CYCLE_SPEC)]);

    let output = specforge_cmd()
        .args(["query", "cycle_a", "--depth=10"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    // Must terminate (not hang) and succeed
    assert!(
        output.status.success(),
        "query on cyclic graph should terminate"
    );
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();

    // Should contain all cycle nodes without duplicates
    let ids: Vec<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();
    let unique: std::collections::BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(
        ids.len(),
        unique.len(),
        "no duplicate nodes in query results"
    );
    assert_eq!(
        unique,
        std::collections::BTreeSet::from(["cycle_a", "cycle_b", "cycle_c"]),
        "every node reachable around the cycle is visited"
    );
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "depth N returns all entities within N hops"
)]
fn query_large_depth_on_small_graph() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    let output = specforge_cmd()
        .args(["query", "beh_middle", "--depth=100"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    let nodes = parsed["nodes"].as_array().unwrap();
    // Should get all connected nodes (beh_middle + feat_root + inv_deep, maybe typ_leaf if connected)
    assert!(
        nodes.len() >= 3,
        "depth 100 on 4-node graph should return all connected nodes, got {}",
        nodes.len()
    );
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "output includes schema_version field"
)]
fn query_output_has_schema_version() {
    let dir = setup_project(&[("main.spec", ISOLATED_SPEC)]);

    let output = specforge_cmd()
        .args(["query", "isolated_node", "--depth=0"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    assert!(
        parsed["schema_version"].is_string(),
        "query output must include schema_version field"
    );
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "output conforms to Graph Protocol schema"
)]
fn query_nodes_have_required_fields() {
    let dir = setup_project(&[("main.spec", DEEP_CHAIN_SPEC)]);

    let output = specforge_cmd()
        .args(["query", "beh_middle", "--depth=2"])
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);

    // The Graph Protocol shape, as the published schema declares it for a
    // node and an edge (`specforge schema --publish`).
    let published = specforge_cmd()
        .args(["schema", "--publish"])
        .arg(dir.path())
        .output()
        .unwrap();
    let published = parse_json_stdout(&published);
    let props = &published["properties"];
    for key in ["format_version", "schema_version", "nodes", "edges"] {
        assert!(parsed.get(key).is_some(), "missing top-level '{key}'");
    }
    assert!(parsed["schema_version"].is_string());
    for key in parsed.as_object().unwrap().keys() {
        assert!(
            props.get(key).is_some(),
            "top-level '{key}' is not in the Graph Protocol"
        );
    }
    // The reply is a document the published schema admits.
    assert_eq!(
        specforge_common::shape::violations(&published, &parsed),
        Vec::<String>::new()
    );
    let nodes = parsed["nodes"].as_array().unwrap();
    let edges = parsed["edges"].as_array().unwrap();
    assert_eq!(nodes.len(), 3, "{parsed}");
    assert_eq!(edges.len(), 3, "{parsed}");
    let beh = nodes.iter().find(|n| n["id"] == "beh_middle").unwrap();
    assert_eq!(beh["kind"], "behavior");
    assert_eq!(beh["title"], "Behavior Middle");
    assert_eq!(beh["fields"]["contract"], "The system MUST validate");
}
