use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Degree, Edge, Graph, Node};

fn make_node(id: &str) -> Node {
    let span = SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 1,
        end_line: 1,
        end_col: 1,
    };
    Node::new(id, "behavior", span)
}

fn add(graph: &mut Graph, source: &str, target: &str) {
    graph.add_edge(Edge {
        source: Sym::new(source),
        target: Sym::new(target),
        label: Sym::new("l"),
    });
}

#[test]
fn degree_counts_the_edges_to_and_from_other_entities() {
    let mut graph = Graph::new();
    for id in ["a", "b", "c", "loop"] {
        graph.add_node(make_node(id));
    }
    add(&mut graph, "a", "b");
    add(&mut graph, "a", "a");
    add(&mut graph, "c", "a");
    add(&mut graph, "a", "ghost");
    add(&mut graph, "loop", "loop");
    assert_eq!(
        graph.degree("a"),
        Degree {
            incoming: 1,
            outgoing: 2
        }
    );
    assert_eq!(
        graph.degree("ghost"),
        Degree {
            incoming: 1,
            outgoing: 0
        }
    );
    assert_eq!(graph.degree("nobody"), Degree::default());
    assert!(graph.degree("loop").is_unconnected());
}
