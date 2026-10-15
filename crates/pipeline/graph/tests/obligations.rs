//! An entity's `verify` statements are its own member, never a field (ADR 0094): the delta
//! reports an edit of them as an aspect, and an incremental build keeps them as a cold build does.

use specforge_common::{SourceSpan, Sym};
use specforge_graph::{
    FileChange, Graph, GraphBuild, GraphConfig, Node, NodeAspect, compute_graph_delta,
};
use specforge_parser::{VerifyStatement, parse};
use specforge_test_macros::test as specforge_test;

fn span() -> SourceSpan {
    SourceSpan {
        file: Sym::new("a.spec"),
        start_line: 1,
        start_col: 1,
        end_line: 1,
        end_col: 1,
    }
}

fn payload(description: &str) -> Graph {
    let mut graph = Graph::new();
    graph.add_node(Node {
        obligations: vec![VerifyStatement {
            kind: "unit".into(),
            description: description.into(),
        }],
        ..Node::new("Payload", "type", span())
    });
    graph
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "a verify statement edit is the obligations aspect, not a changed field"
)]
fn a_statement_edit_is_the_obligations_aspect() {
    let delta = compute_graph_delta(&payload("it works"), &payload("it holds"));
    assert_eq!(delta.modified_nodes.len(), 1);
    let modified = &delta.modified_nodes[0];
    assert_eq!(modified.changed_aspects, [NodeAspect::Obligations]);
    assert!(modified.changed_fields.is_empty(), "{modified:?}");
}

const BEFORE: &str = "type Payload \"Payload\" {\n  verify unit \"it works\"\n}\n";
const AFTER: &str = "type Payload \"Payload\" {\n  verify unit \"it holds\"\n}\n";

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "an incremental build compares each node's obligations"
)]
fn an_incremental_edit_of_a_statement_matches_the_cold_build() {
    let file = |text| parse(text, "a.spec");
    let mut build = GraphBuild::of([file(BEFORE)], GraphConfig::default());
    build.set_verify(true);

    let applied = build.apply([FileChange::Parsed(file(AFTER))]);

    assert_eq!(applied.verification, Some(Ok(())));
    let node = build.graph().node("Payload").unwrap();
    assert_eq!(node.obligations.len(), 1);
    assert_eq!(node.obligations[0].description, "it holds");
    let cold = GraphBuild::of([file(AFTER)], GraphConfig::default());
    assert_eq!(
        node.obligations,
        cold.graph().node("Payload").unwrap().obligations
    );
}
