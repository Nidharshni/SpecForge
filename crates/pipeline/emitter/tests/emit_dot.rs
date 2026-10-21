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
        obligations: Vec::new(),
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
    use crate::support::{declaration, kind};
    let registries = specforge_registry::build_registries(vec![
        declaration(
            "@specforge/software",
            vec![kind("behavior", vec![])],
            vec![],
        ),
        declaration("@specforge/product", vec![kind("term", vec![])], vec![]),
    ]);
    let mut graph = Graph::new();
    graph.add_node(node("beh_b", "behavior", None));
    graph.add_node(node("term_t", "term", None));

    let dot = specforge_emitter::dot::emit_dot(
        &graph,
        &specforge_emitter::DotOptions {
            registries: &registries,
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

/// `behavior b` writing `consumes` and `uses` towards `event e`, through the edge types
/// `BehaviorConsumesEvent` (declared with `style`, `color`, `arrowhead`) and `BehaviorUsesEvent`
/// (declaring none), and the DOT of the graph it builds.
fn styled_dot(style: Option<&str>, color: Option<&str>, arrowhead: Option<&str>) -> String {
    use crate::support::{declaration, kind};
    use specforge_protocol_types::{EdgeTypeDescriptor, FieldDescriptor};

    let writer = |name: &str, edge: &str| FieldDescriptor {
        name: name.to_string(),
        field_type: "reference_list".to_string(),
        edge: Some(edge.to_string()),
        target_kind: Some("event".to_string()),
        ..Default::default()
    };
    let edge_type = |label: &str| EdgeTypeDescriptor {
        label: label.to_string(),
        source_kind: Some("behavior".to_string()),
        target_kind: Some("event".to_string()),
        ..Default::default()
    };
    let registries = specforge_registry::build_registries(vec![declaration(
        "@test/ext",
        vec![
            kind(
                "behavior",
                vec![
                    writer("consumes", "BehaviorConsumesEvent"),
                    writer("uses", "BehaviorUsesEvent"),
                ],
            ),
            kind("event", vec![]),
        ],
        vec![
            EdgeTypeDescriptor {
                edge_style: style.map(str::to_string),
                edge_color: color.map(str::to_string),
                edge_arrowhead: arrowhead.map(str::to_string),
                ..edge_type("BehaviorConsumesEvent")
            },
            edge_type("BehaviorUsesEvent"),
        ],
    )]);
    let mut graph = Graph::new();
    graph.add_node(node("b", "behavior", None));
    graph.add_node(node("e", "event", None));
    for label in ["consumes", "uses", "unwritten"] {
        graph.add_edge(Edge {
            source: Sym::new("b"),
            target: Sym::new("e"),
            label: Sym::new(label),
        });
    }
    specforge_emitter::dot::emit_dot(
        &graph,
        &specforge_emitter::DotOptions {
            registries: &registries,
            ..Default::default()
        },
    )
}

#[specforge_test(
    behavior = "render_extension_defined_edge_styles",
    verify = "edge_style appears in DOT edge attributes"
)]
fn an_edge_is_drawn_in_its_declared_style() {
    let dot = styled_dot(Some("dashed"), None, None);
    assert!(
        dot.contains("\"b\" -> \"e\" [label=\"consumes\" style=\"dashed\"];"),
        "{dot}"
    );
}

#[specforge_test(
    behavior = "render_extension_defined_edge_styles",
    verify = "edge_color sets DOT edge color attribute"
)]
fn declared_edge_color_and_arrowhead_are_written() {
    let dot = styled_dot(Some("dashed"), Some("#E65100"), Some("vee"));
    assert!(
        dot.contains("[label=\"consumes\" style=\"dashed\" color=\"#E65100\" arrowhead=\"vee\"];"),
        "{dot}"
    );
    let hostile = styled_dot(None, Some("\"]; x"), None);
    assert!(hostile.contains("color=\"\\\"]; x\""), "{hostile}");
}

#[specforge_test(
    behavior = "render_extension_defined_edge_styles",
    verify = "default style is solid when edge_style not specified"
)]
fn an_edge_type_without_a_style_is_drawn_by_default() {
    let dot = styled_dot(Some("dashed"), None, None);
    assert!(dot.contains("\"b\" -> \"e\" [label=\"uses\"];"), "{dot}");
    assert!(
        dot.contains("\"b\" -> \"e\" [label=\"unwritten\"];"),
        "{dot}"
    );
}
