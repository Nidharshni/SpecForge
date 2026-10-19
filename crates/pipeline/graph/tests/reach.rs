use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Direction, Edge, Graph, Node};
use std::collections::{HashSet, VecDeque};

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

fn make_edge(source: &str, target: &str, label: &str) -> Edge {
    Edge {
        source: Sym::new(source),
        target: Sym::new(target),
        label: Sym::new(label),
    }
}

/// The walk a trace made before the graph answered it: every edge of the graph scanned for each popped entity.
/// `(id, depth, label)` in discovery order, the root left out.
fn scanned(graph: &Graph, start: &str, direction: Direction) -> Vec<(String, usize, String)> {
    let mut visited = HashSet::from([start.to_string()]);
    let mut queue = VecDeque::from([(start.to_string(), 0usize)]);
    let mut found = Vec::new();
    while let Some((current, depth)) = queue.pop_front() {
        for edge in graph.edges() {
            let (near, far) = match direction {
                Direction::Incoming => (edge.target, edge.source),
                Direction::Outgoing => (edge.source, edge.target),
            };
            if near != current.as_str() {
                continue;
            }
            if visited.insert(far.to_string()) && graph.node(far.as_str()).is_some() {
                found.push((far.to_string(), depth + 1, edge.label.to_string()));
                queue.push_back((far.to_string(), depth + 1));
            }
        }
    }
    found
}

#[test]
fn a_directed_reach_is_the_scan_of_every_edge() {
    let mut graph = Graph::new();
    for i in 0..60 {
        graph.add_node(make_node(&format!("n{i}")));
    }
    let mut x: u64 = 7;
    let mut next = |bound: u64| {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (x >> 33) % bound
    };
    for _ in 0..380 {
        let (s, t, l) = (next(60), next(60), next(3));
        graph.add_edge(make_edge(
            &format!("n{s}"),
            &format!("n{t}"),
            &format!("l{l}"),
        ));
    }
    for k in 0..20 {
        let s = next(60);
        graph.add_edge(make_edge(&format!("n{s}"), &format!("ghost_{k}"), "g"));
        let t = next(60);
        graph.add_edge(make_edge(&format!("ghost_{k}"), &format!("n{t}"), "g"));
    }
    graph.add_edge(make_edge("n3", "n3", "self"));
    for i in 0..60 {
        let root = format!("n{i}");
        for direction in [Direction::Incoming, Direction::Outgoing] {
            let reached = graph.reach_directed(&root, direction).unwrap();
            assert_eq!(reached[0].id, root.as_str());
            assert_eq!(reached[0].via, None);
            let got: Vec<(String, usize, String)> = reached[1..]
                .iter()
                .map(|r| (r.id.to_string(), r.depth, r.via.unwrap().1.to_string()))
                .collect();
            assert_eq!(
                got,
                scanned(&graph, &root, direction),
                "{root} {direction:?}"
            );
        }
    }
}

#[test]
fn a_directed_reach_from_an_unknown_root_is_none() {
    let graph = Graph::new();
    assert!(graph.reach_directed("ghost", Direction::Incoming).is_none());
}

#[test]
fn a_directed_reach_follows_one_way() {
    let mut graph = Graph::new();
    for id in ["top", "left", "right", "bottom", "tail"] {
        graph.add_node(make_node(id));
    }
    for (s, t, l) in [
        ("top", "right", "r"),
        ("top", "left", "l"),
        ("left", "bottom", "lb"),
        ("right", "bottom", "rb"),
        ("bottom", "tail", "t"),
        ("tail", "ghost", "g"),
    ] {
        graph.add_edge(make_edge(s, t, l));
    }
    let walk = |root: &str, direction| -> Vec<(String, String)> {
        graph.reach_directed(root, direction).unwrap()[1..]
            .iter()
            .map(|r| (r.id.to_string(), r.via.unwrap().1.to_string()))
            .collect()
    };
    let pairs = |p: &[(&str, &str)]| -> Vec<(String, String)> {
        p.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    assert_eq!(
        walk("top", Direction::Outgoing),
        pairs(&[
            ("right", "r"),
            ("left", "l"),
            ("bottom", "rb"),
            ("tail", "t")
        ])
    );
    assert_eq!(
        walk("bottom", Direction::Incoming),
        pairs(&[("left", "lb"), ("right", "rb"), ("top", "l")])
    );
}
