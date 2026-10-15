use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue};
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

fn node(id: &str, kind: &str, title: Option<&str>) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: title.map(|s| s.to_string()),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    }
}

fn node_with_contract(id: &str, kind: &str, title: &str, contract: &str) -> Node {
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("contract"),
        FieldValue::String(contract.to_string()),
    );
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: Some(title.to_string()),
        fields,
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    }
}

// B:export_agent_brief_format — verify unit "brief format includes only IDs, kinds, titles, and edges"
#[specforge_test(
    behavior = "export_agent_brief_format",
    verify = "brief format includes only IDs, kinds, titles, and edges"
)]
fn brief_includes_only_ids_kinds_titles_and_edges() {
    let mut graph = Graph::new();
    graph.add_node(node_with_contract(
        "alpha",
        "behavior",
        "Alpha",
        "The system MUST do alpha things with lots of verbose prose.",
    ));
    graph.add_node(node("beta", "feature", Some("Beta")));
    graph.add_edge(Edge {
        source: Sym::new("beta"),
        target: Sym::new("alpha"),
        label: Sym::new("behaviors"),
    });

    let json = crate::export(&graph, specforge_emitter::EmitFormat::Brief, None);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2);

    // Brief format: only id, kind, title — no contract or other fields
    assert_eq!(nodes[0]["id"], "alpha");
    assert_eq!(nodes[0]["kind"], "behavior");
    assert_eq!(nodes[0]["title"], "Alpha");
    assert!(
        nodes[0].get("contract").is_none(),
        "brief must not include contract"
    );
    assert!(
        nodes[0].get("fields").is_none(),
        "brief must not include fields"
    );

    // Has edges
    let edges = parsed["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);

    // Has schema_version
    assert!(parsed["schema_version"].is_string());
}

#[test]
fn brief_is_smaller_than_full_json() {
    let mut graph = Graph::new();
    graph.add_node(node_with_contract(
        "alpha",
        "behavior",
        "Alpha",
        "The system MUST do alpha things with lots of verbose prose that makes the output larger.",
    ));

    let brief = crate::export(&graph, specforge_emitter::EmitFormat::Brief, None);
    let full = crate::support::emit_json(&graph);
    assert!(
        brief.len() < full.len(),
        "brief ({}) should be smaller than full ({})",
        brief.len(),
        full.len()
    );
}

// B:export_agent_brief_format — verify unit "output conforms to Graph Protocol schema"
#[specforge_test(
    behavior = "export_agent_brief_format",
    verify = "output conforms to Graph Protocol schema"
)]
fn brief_conforms_to_graph_protocol_schema() {
    let mut graph = Graph::new();
    graph.add_node(node_with_contract(
        "alpha",
        "behavior",
        "Alpha",
        "contract text",
    ));
    graph.add_node(node("beta", "feature", Some("Beta")));
    graph.add_edge(Edge {
        source: Sym::new("beta"),
        target: Sym::new("alpha"),
        label: Sym::new("behaviors"),
    });

    let json = crate::export(&graph, specforge_emitter::EmitFormat::Brief, None);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    // Graph Protocol requires: schema_version, nodes array, edges array
    assert!(parsed["schema_version"].is_string());
    assert!(parsed["nodes"].is_array());
    assert!(parsed["edges"].is_array());

    // Each node must have id and kind
    for node in parsed["nodes"].as_array().unwrap() {
        assert!(node["id"].is_string(), "node must have id");
        assert!(node["kind"].is_string(), "node must have kind");
    }
}

// B:export_agent_brief_format — verify contract "requires/ensures consistency for agent brief export"
#[specforge_test(
    behavior = "export_agent_brief_format",
    verify = "Export Agent Brief Format: agent brief export holds — validation_complete_fired, minimal_representation, schema_version_present, export_complete_emitted"
)]
fn brief_export_contract() {
    // Requires: graph is finalized (validation_complete)
    // Ensures: minimal representation (IDs, kinds, titles, edges), schema_version present
    let mut graph = Graph::new();
    graph.add_node(node_with_contract(
        "a",
        "behavior",
        "A",
        "The system MUST do X",
    ));
    graph.add_node(node("b", "feature", Some("B")));
    graph.add_edge(Edge {
        source: "b".into(),
        target: "a".into(),
        label: "behaviors".into(),
    });

    let json = crate::export(&graph, specforge_emitter::EmitFormat::Brief, None);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Minimal representation: IDs, kinds, titles present
    let nodes = parsed["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2);
    assert!(nodes[0]["id"].is_string());
    assert!(nodes[0]["kind"].is_string());

    // No fields/contract in brief format
    for node in nodes {
        assert!(
            node.get("contract").is_none(),
            "brief must not include contract"
        );
        assert!(
            node.get("fields").is_none(),
            "brief must not include fields"
        );
    }

    // Schema version present
    assert!(parsed["schema_version"].is_string());
}

// B:export_agent_brief_format — verify unit "output includes schema_version field"
#[specforge_test(
    behavior = "export_agent_brief_format",
    verify = "output includes schema_version field"
)]
fn brief_schema_version_matches_graph_format() {
    let mut graph = Graph::new();
    graph.add_node(node("x", "behavior", Some("X")));

    let brief: serde_json::Value = serde_json::from_str(&crate::export(
        &graph,
        specforge_emitter::EmitFormat::Brief,
        None,
    ))
    .unwrap();
    let full: serde_json::Value = serde_json::from_str(&crate::support::emit_json(&graph)).unwrap();
    assert_eq!(brief["schema_version"], "1.0.0", "{brief}");
    assert_eq!(
        brief["schema_version"], full["schema_version"],
        "brief and graph formats must use same schema_version"
    );
}

/// Three kinds, an edge between two of them, one with prose.
fn multi_kind_graph() -> Graph {
    let mut graph = Graph::new();
    graph.add_node(node_with_contract(
        "alpha",
        "behavior",
        "Alpha",
        "The system MUST do alpha things.",
    ));
    graph.add_node(node("beta", "feature", Some("Beta")));
    graph.add_node(node("gamma", "invariant", None));
    graph.add_edge(Edge {
        source: Sym::new("beta"),
        target: Sym::new("alpha"),
        label: Sym::new("behaviors"),
    });
    graph
}

#[specforge_test(
    behavior = "export_agent_brief_format",
    verify = "brief format includes only IDs, kinds, titles, and edges"
)]
fn brief_export_of_the_multi_kind_graph() {
    insta::assert_snapshot!(
        "brief_export_of_the_multi_kind_graph",
        crate::export(
            &multi_kind_graph(),
            specforge_emitter::EmitFormat::Brief,
            None
        )
    );
}
