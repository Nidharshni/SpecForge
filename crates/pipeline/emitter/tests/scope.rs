use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_parser::{EntityId, EntityKind, FieldMap};
use specforge_test::prelude::*;

fn span() -> SourceSpan {
    SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 0,
    }
}

fn node(id: &str, kind: &str) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: Some(format!("Title {}", id)),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
    }
}

/// A -> B -> C, D (disconnected)
fn build_chain_graph() -> Graph {
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature"));
    graph.add_node(node("b", "behavior"));
    graph.add_node(node("c", "invariant"));
    graph.add_node(node("d", "behavior")); // disconnected
    graph.add_edge(Edge {
        source: Sym::new("a"),
        target: Sym::new("b"),
        label: Sym::new("behaviors"),
    });
    graph.add_edge(Edge {
        source: Sym::new("b"),
        target: Sym::new("c"),
        label: Sym::new("invariants"),
    });
    graph
}

// B:export_agent_graph_format — verify unit "scoped export returns only reachable subgraph"
#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "scoped export returns only reachable subgraph"
)]
fn scoped_json_returns_only_reachable_subgraph() {
    let graph = build_chain_graph();
    let json = crate::support::scoped_json(&graph, "b").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let nodes = parsed["nodes"].as_array().unwrap();
    let ids: Vec<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();

    // Scoped at "b": reachable = b (root), a (incoming), c (outgoing)
    assert!(ids.contains(&"b"), "root must be included");
    assert!(ids.contains(&"a"), "upstream neighbor must be included");
    assert!(ids.contains(&"c"), "downstream neighbor must be included");
    assert!(!ids.contains(&"d"), "disconnected node must be excluded");
}

// B:export_agent_context_format — verify unit "scoped export returns only reachable subgraph"
#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "scoped export returns only reachable subgraph"
)]
fn scoped_context_returns_only_reachable_subgraph() {
    let graph = build_chain_graph();
    let json = crate::support::scoped_context(&graph, "a").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let nodes = parsed["nodes"].as_array().unwrap();
    let ids: Vec<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();

    // Scoped at "a": reachable = a, b, c (through chain)
    assert!(ids.contains(&"a"));
    assert!(ids.contains(&"b"));
    assert!(ids.contains(&"c"));
    assert!(!ids.contains(&"d"));
}

#[test]
fn scoped_export_on_nonexistent_entity_returns_error() {
    let graph = build_chain_graph();
    let result = crate::support::scoped_json(&graph, "nonexistent");
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert_eq!(err.code(), Some(specforge_diagnostics::codes::E003));
}

// B:export_agent_graph_format — verify unit "non-existent scope entity produces E003 and exit code 1"
#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "non-existent scope entity produces E003 and exit code 1"
)]
fn graph_scoped_export_on_nonexistent_entity_returns_e001() {
    let graph = build_chain_graph();
    // Scoped at an ID that is not an entity (though "a" is a prefix of it).
    for scope in ["nonexistent", "ab"] {
        let err = specforge_emitter::emit(
            &graph,
            &specforge_emitter::EmitOptions {
                scope: Some(scope),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("unresolved entity '{scope}' — not found in graph")
        );
        // The failure is the catalogued E003; its message carries no code.
        assert_eq!(err.code(), Some(specforge_diagnostics::codes::E003));
    }
    // An existing scope is not an error.
    assert!(crate::support::scoped_json(&graph, "a").is_ok());
}

// B:export_agent_graph_format — verify unit "scoped export returns only reachable subgraph"
// (validates edges are also scoped correctly)
#[specforge_test(behavior = "export_agent_graph_format")]
fn scoped_edges_only_between_reachable_nodes() {
    let graph = build_chain_graph();
    let json = crate::support::scoped_json(&graph, "a").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let edges = parsed["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2); // a->b, b->c
}

// B:export_agent_graph_format — verify unit "scoped export returns only reachable subgraph"
// (edge case: disconnected leaf returns single node)
#[specforge_test(behavior = "export_agent_graph_format")]
fn scoped_leaf_node_returns_single_node() {
    let graph = build_chain_graph();
    // "d" is disconnected, so scope returns just d
    let json = crate::support::scoped_json(&graph, "d").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["id"], "d");

    let edges = parsed["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 0);
}
