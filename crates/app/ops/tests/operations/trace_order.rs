use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_ops::read::trace::TraceLink;
use specforge_test::prelude::*;

use crate::view_support::chain;

fn node(id: &str) -> Node {
    Node::new(
        id,
        "behavior",
        SourceSpan {
            file: Sym::new("test.spec"),
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 0,
        },
    )
}

/// r -y-> b, r -z-> a, r -x-> a, a -p-> t, b -q-> t, added in that order.
fn two_ways() -> Graph {
    let mut graph = Graph::new();
    for id in ["r", "a", "b", "t"] {
        graph.add_node(node(id));
    }
    for (source, target, label) in [
        ("r", "b", "y"),
        ("r", "a", "z"),
        ("r", "a", "x"),
        ("a", "t", "p"),
        ("b", "t", "q"),
    ] {
        graph.add_edge(Edge {
            source: source.into(),
            target: target.into(),
            label: label.into(),
        });
    }
    graph
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "a link carries the label of the edge it was first reached over, nearer entities first and each entity's edges in written order"
)]
fn a_link_carries_the_label_it_was_first_reached_over() {
    let graph = two_ways();
    let links = |links: &[TraceLink]| -> Vec<(String, usize, String)> {
        links
            .iter()
            .map(|l| (l.entity_id.clone(), l.depth, l.edge_label.clone()))
            .collect()
    };
    let expect = |expected: &[(&str, usize, &str)]| -> Vec<(String, usize, String)> {
        expected
            .iter()
            .map(|(id, depth, label)| (id.to_string(), *depth, label.to_string()))
            .collect()
    };
    // b is reached first (r -y-> b is written first), so t is reached from b; a's first edge is z.
    let down = chain(&graph, "r");
    assert_eq!(
        links(&down.downstream),
        expect(&[("a", 1, "z"), ("b", 1, "y"), ("t", 2, "q")])
    );
    // a -p-> t is written before b -q-> t; r -z-> a before r -x-> a.
    let up = chain(&graph, "t");
    assert_eq!(
        links(&up.upstream),
        expect(&[("a", 1, "p"), ("b", 1, "q"), ("r", 2, "z")])
    );
}
