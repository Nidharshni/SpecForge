//! W061: the entities on a cycle, however the references close it and
//! however they are written (ADR 0196).

use specforge_common::{DiagnosticData, SourceSpan, Sym};
use specforge_graph::{Cycle, Edge, Graph, Node, build_graph};
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

/// The W061s of `source`.
fn w061s(source: &str) -> Vec<specforge_common::Diagnostic> {
    let (_, diagnostics) = build_graph(&[parse(source, "main.spec")]);
    diagnostics
        .into_iter()
        .filter(|d| d.code == "W061")
        .collect()
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "W061 names every entity of a cycle, one whose cycle closes through an entity already walked included"
)]
fn w061_names_every_entity_of_a_tangle_of_cycles() {
    let found = w061s(
        r#"
behavior alpha "A" { contract "c" depends_on [beta, delta] }
behavior beta "B" { contract "c" depends_on [gamma] }
behavior gamma "C" { contract "c" depends_on [alpha] }
behavior delta "D" { contract "c" depends_on [beta] }
"#,
    );

    assert_eq!(found.len(), 1, "{found:?}");
    let data = found[0].data.as_deref().expect("W061 carries data");
    let path: Vec<String> = [
        "alpha", "beta", "gamma", "alpha", "delta", "beta", "gamma", "alpha",
    ]
    .map(String::from)
    .into();
    assert_eq!(data, &DiagnosticData::ReferenceCycle { path });
    assert_eq!(data.entities(), ["alpha", "beta", "gamma", "delta"]);
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "W061 is the same whatever order the references are written in"
)]
fn w061_is_the_same_whatever_order_the_references_are_written_in() {
    let written = |first: &str, second: &str| {
        w061s(&format!(
            r#"
behavior alpha "A" {{ contract "c" depends_on [{first}, {second}] }}
behavior beta "B" {{ contract "c" depends_on [gamma] }}
behavior gamma "C" {{ contract "c" depends_on [alpha] }}
"#
        ))
    };

    let (forward, backward) = (written("beta", "gamma"), written("gamma", "beta"));

    assert_eq!(forward, backward);
    assert_eq!(forward.len(), 1);
    assert_eq!(
        forward[0].message,
        "reference cycle detected: alpha -> beta -> gamma -> alpha"
    );
}

/// A graph of behaviors `ids`, with the `depends_on` edges `edges`.
fn graph_of(ids: &[&str], edges: &[(&str, &str)]) -> Graph {
    let span = SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 1,
        end_line: 1,
        end_col: 1,
    };
    let mut graph = Graph::new();
    for id in ids {
        graph.add_node(Node::new(id, "behavior", span.clone()));
    }
    for (source, target) in edges {
        graph.add_edge(Edge {
            source: Sym::new(source),
            target: Sym::new(target),
            label: Sym::new("depends_on"),
        });
    }
    graph
}

fn strings(cycle: &Cycle<Sym>) -> (Vec<&str>, Vec<&str>) {
    let names = |nodes: &'_ [Sym]| nodes.iter().map(|n| n.as_str()).collect::<Vec<_>>();
    (names(&cycle.members), names(&cycle.path))
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "detects cycles in directed graph"
)]
fn cycles_finds_a_simple_cycle() {
    let graph = graph_of(&["a", "b", "c"], &[("a", "b"), ("b", "c"), ("c", "a")]);

    let cycles = graph.cycles();

    assert_eq!(cycles.len(), 1);
    assert_eq!(
        strings(&cycles[0]),
        (vec!["a", "b", "c"], vec!["a", "b", "c", "a"])
    );
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "no false positives for acyclic graph"
)]
fn cycles_is_empty_for_a_dag() {
    let graph = graph_of(&["a", "b", "c"], &[("c", "a"), ("c", "b"), ("b", "a")]);

    assert!(graph.cycles().is_empty(), "DAG should have no cycles");
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "detects self-referencing cycle"
)]
fn cycles_finds_a_self_loop() {
    let graph = graph_of(&["a"], &[("a", "a")]);

    let cycles = graph.cycles();

    assert_eq!(cycles.len(), 1);
    assert_eq!(strings(&cycles[0]), (vec!["a"], vec!["a", "a"]));
}
