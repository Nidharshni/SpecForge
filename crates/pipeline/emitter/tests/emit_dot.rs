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
    }
}

// B:serialize_dot_visualization — verify unit "DOT output is valid Graphviz syntax"
#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "DOT output is valid Graphviz syntax"
)]
fn empty_graph_produces_valid_dot() {
    // An empty graph is a complete digraph with no statements.
    let graph = Graph::new();
    let dot = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());
    assert_eq!(
        dot,
        "digraph specforge {\n  rankdir=LR;\n  node [shape=box];\n}\n"
    );

    // A title with quotes and a newline stays inside its quoted label.
    let mut graph = Graph::new();
    graph.add_node(node("q", "behavior", Some("say \"hi\"\nthen }")));
    graph.add_node(node("r", "feature", None));
    graph.add_edge(Edge {
        source: Sym::new("r"),
        target: Sym::new("q"),
        label: Sym::new("behaviors"),
    });
    let dot = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());
    assert_eq!(
        dot,
        concat!(
            "digraph specforge {\n",
            "  rankdir=LR;\n",
            "  node [shape=box];\n",
            "  \"q\" [label=\"q\\nsay \\\"hi\\\"\\nthen }\"];\n",
            "  \"r\" [label=\"r\"];\n",
            "  \"r\" -> \"q\" [label=\"behaviors\"];\n",
            "}\n",
        )
    );
}

// B:serialize_dot_visualization — verify unit "nodes are labeled with IDs"
#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "nodes are labeled with IDs"
)]
fn dot_nodes_labeled_with_id_and_title() {
    let mut graph = Graph::new();
    graph.add_node(node("alpha", "behavior", Some("Alpha Behavior")));

    let dot = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());
    assert!(dot.contains("alpha"), "node ID in DOT output");
    assert!(dot.contains("Alpha Behavior"), "node title in DOT output");
}

// B:serialize_dot_visualization — verify unit "edges are labeled with types"
#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "edges are labeled with types"
)]
fn dot_edges_labeled_with_type() {
    let mut graph = Graph::new();
    graph.add_node(node("feat_a", "feature", Some("Feature A")));
    graph.add_node(node("beh_b", "behavior", None));
    graph.add_edge(Edge {
        source: Sym::new("feat_a"),
        target: Sym::new("beh_b"),
        label: Sym::new("behaviors"),
    });

    let dot = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());
    assert!(
        dot.contains("feat_a") && dot.contains("beh_b"),
        "edge endpoints in DOT"
    );
    assert!(dot.contains("behaviors"), "edge label in DOT");
}

#[test]
fn dot_node_default_shape_is_box() {
    let mut graph = Graph::new();
    graph.add_node(node("alpha", "behavior", Some("Alpha")));

    let dot = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());
    assert!(dot.contains("box"), "default shape is box");
}

// B:serialize_dot_visualization — verify unit "labels toggle emits bare IDs"
#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "labels toggle emits bare IDs"
)]
fn dot_labels_toggle_drops_titles() {
    let mut graph = Graph::new();
    graph.add_node(node("alpha", "behavior", Some("Alpha Behavior")));

    let dot = specforge_emitter::dot::emit_dot(
        &graph,
        &specforge_emitter::DotOptions {
            labels: false,
            ..Default::default()
        },
    );
    assert!(dot.contains("alpha"), "node ID still emitted");
    assert!(
        !dot.contains("Alpha Behavior"),
        "title must be dropped when labels are off: {dot}"
    );
}

// B:serialize_dot_visualization — verify unit "kind filter drops other kinds"
#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "kind filter drops other kinds"
)]
fn dot_kind_filter_drops_nodes_and_edges() {
    let mut graph = Graph::new();
    graph.add_node(node("feat_a", "feature", None));
    graph.add_node(node("beh_b", "behavior", None));
    graph.add_edge(Edge {
        source: Sym::new("feat_a"),
        target: Sym::new("beh_b"),
        label: Sym::new("behaviors"),
    });

    let behaviors = vec!["behavior".to_string()];
    let dot = specforge_emitter::dot::emit_dot(
        &graph,
        &specforge_emitter::DotOptions {
            kind_filter: Some(&behaviors),
            ..Default::default()
        },
    );
    assert!(dot.contains("beh_b"), "kept kind present");
    assert!(!dot.contains("feat_a"), "filtered kind absent: {dot}");
    assert!(
        !dot.contains("behaviors"),
        "edge with filtered endpoint must be dropped: {dot}"
    );
}

// B:serialize_dot_visualization — verify unit "clusters group by declaring extension"
#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "clusters group by declaring extension"
)]
fn dot_cluster_by_extension_groups_nodes() {
    let mut registry = specforge_registry::KindRegistry::new();
    registry.register(specforge_registry::KindRegistryEntry {
        kind_name: "behavior".to_string(),
        source_extension: "@specforge/software".to_string(),
        testable: false,
        supports_verify: false,
        allowed_verify_kinds: Vec::new(),
        lifecycle_field: None,
        ..Default::default()
    });
    registry.register(specforge_registry::KindRegistryEntry {
        kind_name: "term".to_string(),
        source_extension: "@specforge/product".to_string(),
        testable: false,
        supports_verify: false,
        allowed_verify_kinds: Vec::new(),
        lifecycle_field: None,
        ..Default::default()
    });
    let mut graph = Graph::new();
    graph.add_node(node("beh_b", "behavior", None));
    graph.add_node(node("term_t", "term", None));

    let dot = specforge_emitter::dot::emit_dot(
        &graph,
        &specforge_emitter::DotOptions {
            kind_registry: Some(&registry),
            cluster_by_extension: true,
            ..Default::default()
        },
    );
    assert!(
        dot.contains("subgraph cluster_specforge_software"),
        "software cluster present: {dot}"
    );
    assert!(
        dot.contains("subgraph cluster_specforge_product"),
        "product cluster present: {dot}"
    );
    // Clusters iterate in extension-name order: @specforge/product first.
    let product = dot.find("cluster_specforge_product").unwrap();
    let software = dot.find("cluster_specforge_software").unwrap();
    let term = dot.find("\"term_t\"").unwrap();
    let beh = dot.find("\"beh_b\"").unwrap();
    assert!(product < software, "clusters sorted by extension: {dot}");
    assert!(
        product < term && term < software,
        "term_t inside the product cluster: {dot}"
    );
    assert!(software < beh, "beh_b inside the software cluster: {dot}");
}
