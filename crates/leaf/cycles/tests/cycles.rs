//! Which nodes lie on a cycle, and one closed walk through each (ADR 0196).
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use std::collections::{BTreeMap, BTreeSet};

use specforge_cycles::{Cycle, cycles};
use specforge_test_macros::test as specforge_test;

fn graph(edges: &[(&'static str, &'static str)]) -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    let mut graph: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
    for (source, target) in edges {
        graph.entry(*source).or_default().insert(*target);
    }
    graph
}

fn cycle(members: &[&'static str], path: &[&'static str]) -> Cycle<&'static str> {
    Cycle {
        members: members.to_vec(),
        path: path.to_vec(),
    }
}

#[test]
fn a_node_whose_cycle_closes_through_a_finished_node_is_a_member() {
    let found = cycles(&graph(&[
        ("alpha", "beta"),
        ("alpha", "delta"),
        ("beta", "gamma"),
        ("gamma", "alpha"),
        ("delta", "beta"),
    ]));

    assert_eq!(
        found,
        [cycle(
            &["alpha", "beta", "delta", "gamma"],
            &[
                "alpha", "beta", "gamma", "alpha", "delta", "beta", "gamma", "alpha"
            ]
        )]
    );
}

#[test]
fn a_feeder_is_not_a_member() {
    let found = cycles(&graph(&[("feeder", "a"), ("a", "b"), ("b", "a")]));

    assert_eq!(found, [cycle(&["a", "b"], &["a", "b", "a"])]);
}

#[test]
fn a_self_edge_is_a_cycle_of_one() {
    let found = cycles(&graph(&[("a", "a"), ("b", "a")]));

    assert_eq!(found, [cycle(&["a"], &["a", "a"])]);
}

#[test]
fn a_lone_node_and_a_chain_are_no_cycle() {
    assert!(cycles(&graph(&[("a", "b"), ("b", "c")])).is_empty());
    assert!(cycles::<&str>(&BTreeMap::new()).is_empty());
}

#[test]
fn disjoint_cycles_come_in_order_of_their_smallest_member() {
    let found = cycles(&graph(&[("y", "x"), ("x", "y"), ("b", "a"), ("a", "b")]));

    assert_eq!(
        found,
        [
            cycle(&["a", "b"], &["a", "b", "a"]),
            cycle(&["x", "y"], &["x", "y", "x"]),
        ]
    );
}

/// Every directed graph of four nodes, self-edges included: 2^16 of them.
fn every_graph_of_four_nodes() -> impl Iterator<
    Item = (
        BTreeMap<&'static str, BTreeSet<&'static str>>,
        [[bool; 4]; 4],
    ),
> {
    const NAMES: [&str; 4] = ["n0", "n1", "n2", "n3"];
    (0u32..1 << 16).map(|mask| {
        let mut edges: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        let mut reach = [[false; 4]; 4];
        for (s, source) in NAMES.iter().enumerate() {
            for (t, target) in NAMES.iter().enumerate() {
                if mask & (1 << (s * 4 + t)) != 0 {
                    edges.entry(source).or_default().insert(target);
                    reach[s][t] = true;
                }
            }
        }
        for k in 0..4 {
            for i in 0..4 {
                for j in 0..4 {
                    reach[i][j] |= reach[i][k] && reach[k][j];
                }
            }
        }
        (edges, reach)
    })
}

#[specforge_test(
    constraint = "product_cycle_detection_correctness",
    verify = "cycle detection has zero false negatives on every directed graph of four nodes"
)]
fn every_graph_of_four_nodes_has_every_node_on_a_cycle_reported() {
    for (mask, (edges, reach)) in every_graph_of_four_nodes().enumerate() {
        let found = cycles(&edges);
        let members: BTreeSet<&str> = found
            .iter()
            .flat_map(|c| c.members.iter().copied())
            .collect();
        let on_a_cycle: BTreeSet<&str> = (0..4)
            .filter(|&i| reach[i][i])
            .map(|i| ["n0", "n1", "n2", "n3"][i])
            .collect();
        assert_eq!(members, on_a_cycle, "graph {mask}");
        for c in &found {
            assert_eq!(c.path.first(), c.members.first(), "graph {mask}");
            assert_eq!(c.path.first(), c.path.last(), "graph {mask}");
            for step in c.path.windows(2) {
                assert!(edges[step[0]].contains(step[1]), "graph {mask}: {c:?}");
            }
            let walked: BTreeSet<&str> = c.path.iter().copied().collect();
            assert_eq!(walked, c.members.iter().copied().collect(), "graph {mask}");
        }
    }
}

#[specforge_test(
    constraint = "product_cycle_detection_correctness",
    verify = "cycle detection has zero false positives on every directed graph of four nodes"
)]
fn no_graph_of_four_nodes_has_a_node_off_every_cycle_reported() {
    for (mask, (edges, reach)) in every_graph_of_four_nodes().enumerate() {
        for c in cycles(&edges) {
            for member in &c.members {
                let i: usize = member[1..].parse().unwrap();
                assert!(reach[i][i], "graph {mask}: {member} reaches itself");
            }
        }
    }
}
