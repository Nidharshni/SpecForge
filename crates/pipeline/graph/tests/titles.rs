//! Every block that becomes a node has a title: E076 otherwise (ADR 0148).

use specforge_graph::build_graph;
use specforge_parser::parse;

#[test]
#[ignore = "flipped by T8"]
fn a_block_with_no_title_is_e076() {
    let (graph, diagnostics) = build_graph(&[parse("type X {\n  id string\n}\n", "a.spec")]);

    let codes: Vec<_> = diagnostics.iter().map(|d| d.code.to_string()).collect();
    assert_eq!(codes, ["E076"], "{diagnostics:?}");
    assert!(graph.nodes().iter().any(|n| n.id.raw == "X"));
}
