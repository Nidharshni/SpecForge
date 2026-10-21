use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};

fn node(id: &str, kind: &str) -> Node {
    let span = SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 1,
        end_line: 1,
        end_col: 1,
    };
    Node::new(id, kind, span)
}

fn edge(source: &str, target: &str, label: &str) -> Edge {
    Edge {
        source: Sym::new(source),
        target: Sym::new(target),
        label: Sym::new(label),
    }
}

/// top -> {left, right} -> bottom -> tail, five nodes and five edges.
fn diamond() -> Graph {
    let mut graph = Graph::new();
    for id in ["top", "left", "right", "bottom", "tail"] {
        graph.add_node(node(id, "behavior"));
    }
    graph.add_edge(edge("top", "right", "r"));
    graph.add_edge(edge("top", "left", "l"));
    graph.add_edge(edge("left", "bottom", "lb"));
    graph.add_edge(edge("right", "bottom", "rb"));
    graph.add_edge(edge("bottom", "tail", "t"));
    graph
}

#[test]
fn induced_keeps_its_nodes_and_every_edge_between_them() {
    let graph = diamond();
    let sub = graph.induced(["tail", "bottom", "left", "nowhere"].map(Sym::new));
    let mut ids: Vec<&str> = sub.nodes().iter().map(|n| n.id.raw.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["bottom", "left", "tail"]);
    let labels: Vec<&str> = sub.edges().iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, ["lb", "t"], "the edges among them, in edge order");
    assert!(graph.induced([]).nodes().is_empty());
}
