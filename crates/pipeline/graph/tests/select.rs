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

/// f (feature) -> b1 (behavior) -> i (invariant), and an unconnected x (behavior).
fn chain() -> Graph {
    let mut graph = Graph::new();
    for (id, kind) in [
        ("f", "feature"),
        ("b1", "behavior"),
        ("i", "invariant"),
        ("x", "behavior"),
    ] {
        graph.add_node(node(id, kind));
    }
    graph.add_edge(edge("f", "b1", "features"));
    graph.add_edge(edge("b1", "i", "invariants"));
    graph
}

fn names(ids: impl Iterator<Item = Sym>) -> Vec<&'static str> {
    ids.map(Sym::as_str).collect()
}

#[test]
fn a_selection_is_what_its_root_reaches_of_the_kinds_it_keeps() {
    let selected = chain().select(Some("b1"), Some(1), &["invariant"]).unwrap();
    assert_eq!(names(selected.ids()), ["i"]);
    let reached: Vec<&str> = selected.reached().iter().map(|r| r.id.as_str()).collect();
    assert_eq!(reached, ["b1", "f", "i"]);
    assert_eq!(selected.root().map(Sym::as_str), Some("b1"));
    assert_eq!(names(selected.drawn()), ["i", "b1"]);
    assert!(selected.contains("i") && !selected.contains("b1"));
}

#[test]
fn a_selection_without_a_root_is_every_entity_of_its_kinds() {
    let graph = chain();
    let selected = graph.select(None, Some(3), &["behavior"]).unwrap();
    assert_eq!(names(selected.ids()), ["b1", "x"]);
    assert_eq!(selected.depth(), None);
    assert!(selected.reached().is_empty());
    let whole = graph.select(None, None, &[]).unwrap();
    assert_eq!(names(whole.ids()), ["b1", "f", "i", "x"]);
    assert!(whole.is_whole() && !selected.is_whole());
}

#[test]
fn the_root_is_selected_when_its_kind_is_kept() {
    let graph = chain();
    let kept = graph.select(Some("b1"), Some(0), &["behavior"]).unwrap();
    assert_eq!(names(kept.ids()), ["b1"]);
    let dropped = graph.select(Some("b1"), Some(0), &["feature"]).unwrap();
    assert_eq!(dropped.ids().count(), 0);
    assert_eq!(names(dropped.drawn()), ["b1"]);
}

#[test]
fn a_root_no_node_declares_selects_nothing() {
    let graph = chain();
    assert!(graph.select(Some("ghost"), None, &[]).is_none());
    assert!(graph.select(Some("b"), None, &[]).is_none());
}

#[test]
fn a_kind_no_entity_has_keeps_nothing() {
    let graph = chain();
    for kind in ["", "widget"] {
        let selected = graph.select(None, None, &[kind]).unwrap();
        assert_eq!(selected.ids().count(), 0);
        assert!(!selected.is_whole());
        assert_eq!(names(selected.kinds().iter().copied()), [kind]);
    }
}
