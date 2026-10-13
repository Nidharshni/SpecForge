use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue, parse_expression};
use specforge_project::{EdgeChange, GraphDelta, NodeAspect, NodeChange, compute_graph_delta};
use specforge_test::prelude::*;

fn make_node(id: &str, kind: &str, file: &str, line: usize) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: Some(id.to_string()),
        fields: FieldMap::new(),
        source_span: SourceSpan {
            file: Sym::new(file),
            start_line: line,
            start_col: 0,
            end_line: line,
            end_col: 0,
        },
        methods: Vec::new(),
    }
}

fn edge(source: &str, target: &str, label: &str) -> Edge {
    Edge {
        source: Sym::new(source),
        target: Sym::new(target),
        label: Sym::new(label),
    }
}

fn change(source: &str, target: &str, label: &str) -> EdgeChange {
    EdgeChange {
        source: source.to_string(),
        target: target.to_string(),
        label: label.to_string(),
    }
}

fn node_change(id: &str) -> NodeChange {
    NodeChange {
        id: id.to_string(),
        kind: "type".to_string(),
        file: "a.spec".to_string(),
        line: 9,
    }
}

fn ids(nodes: &[NodeChange]) -> Vec<&str> {
    nodes.iter().map(|n| n.id.as_str()).collect()
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "added nodes appear in delta"
)]
fn added_nodes_appear_in_delta() {
    let mut new = Graph::new();
    new.add_node(make_node("foo", "behavior", "a.spec", 1));

    let delta = compute_graph_delta(&Graph::new(), &new);

    assert_eq!(
        delta.added_nodes,
        [NodeChange {
            id: "foo".into(),
            kind: "behavior".into(),
            file: "a.spec".into(),
            line: 1,
        }]
    );
    assert!(delta.removed_nodes.is_empty() && delta.modified_nodes.is_empty());
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "removed nodes appear in delta"
)]
fn removed_nodes_appear_in_delta() {
    let mut old = Graph::new();
    old.add_node(make_node("bar", "feature", "b.spec", 5));

    let delta = compute_graph_delta(&old, &Graph::new());

    assert_eq!(ids(&delta.removed_nodes), ["bar"]);
    assert_eq!(delta.removed_nodes[0].kind, "feature");
    assert!(delta.added_nodes.is_empty());
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "modified nodes list changed fields"
)]
fn modified_nodes_list_changed_fields() {
    let with_status = |status: &str, line: usize| {
        let mut node = make_node("baz", "behavior", "c.spec", line);
        node.fields
            .push(Sym::new("status"), FieldValue::String(status.to_string()));
        node
    };
    let mut old = Graph::new();
    old.add_node(with_status("draft", 1));
    let mut new = Graph::new();
    new.add_node(with_status("done", 1));

    let delta = compute_graph_delta(&old, &new);

    assert!(delta.added_nodes.is_empty() && delta.removed_nodes.is_empty());
    assert_eq!(delta.modified_nodes.len(), 1);
    assert_eq!(delta.modified_nodes[0].id, "baz");
    assert_eq!(delta.modified_nodes[0].changed_fields, ["status"]);

    // Moved, not modified.
    let mut moved = Graph::new();
    moved.add_node(with_status("draft", 7));
    assert!(compute_graph_delta(&old, &moved).is_empty());
}

/// A graph holding `alpha`, carrying a formal `metric` expression parsed
/// from `src`, declared at `line`.
fn graph_with_metric(src: &str, line: usize) -> Graph {
    let mut node = make_node("alpha", "behavior", "a.spec", line);
    node.fields.push(
        Sym::new("metric"),
        FieldValue::Expression(vec![parse_expression(src).unwrap()]),
    );
    node.source_span.end_line = line + 4;
    let mut graph = Graph::new();
    graph.add_node(node);
    graph
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "shifted expression positions are not a modification"
)]
fn expression_positions_are_not_a_modification() {
    // Same expression, shifted: the entity moved and the expression's
    // columns moved with it.
    let before = graph_with_metric("latency < 100ms", 1);
    let after = graph_with_metric("   latency < 100ms", 9);
    let delta = compute_graph_delta(&before, &after);
    assert!(delta.is_empty(), "{:?}", delta.modified_nodes);

    // A changed bound is a modification.
    let tighter = graph_with_metric("latency < 50ms", 1);
    assert_eq!(
        compute_graph_delta(&before, &tighter)
            .modified_nodes
            .iter()
            .map(|n| n.id.as_str())
            .collect::<Vec<_>>(),
        ["alpha"]
    );
}

#[test]
fn a_changed_expression_is_a_modification() {
    let before = graph_with_metric("a < 1", 1);
    let after = graph_with_metric("a < 2", 1);
    let delta = compute_graph_delta(&before, &after);
    assert_eq!(delta.modified_nodes.len(), 1, "{:?}", delta.modified_nodes);
    assert_eq!(delta.modified_nodes[0].changed_fields, ["metric"]);
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "added and removed edges appear in delta"
)]
fn added_and_removed_edges_appear_in_delta() {
    let mut old = Graph::new();
    old.add_node(make_node("a", "behavior", "a.spec", 1));
    old.add_node(make_node("b", "feature", "a.spec", 5));
    old.add_edge(edge("a", "b", "features"));

    let mut new = Graph::new();
    new.add_node(make_node("a", "behavior", "a.spec", 1));
    new.add_node(make_node("b", "feature", "a.spec", 5));
    new.add_node(make_node("c", "invariant", "a.spec", 10));
    new.add_edge(edge("a", "c", "invariants"));

    let delta = compute_graph_delta(&old, &new);

    assert_eq!(delta.removed_edges, [change("a", "b", "features")]);
    assert_eq!(delta.added_edges, [change("a", "c", "invariants")]);
    // a's outgoing edges changed: it is modified.
    assert_eq!(delta.modified_nodes[0].id, "a");
    assert!(delta.modified_nodes[0].changed_fields.is_empty());
    assert_eq!(delta.modified_nodes[0].changed_aspects, [NodeAspect::Edges]);
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "affected files listed in delta"
)]
fn affected_files_listed_in_delta() {
    let mut old = Graph::new();
    old.add_node(make_node("x", "behavior", "file1.spec", 1));
    let mut new = Graph::new();
    new.add_node(make_node("y", "feature", "file2.spec", 1));

    let delta = compute_graph_delta(&old, &new);

    assert_eq!(delta.affected_files, ["file1.spec", "file2.spec"]);
}

#[specforge_test(
    invariant = "graph_delta_determinism",
    verify = "GraphDelta arrays are sorted by EntityId.raw"
)]
fn delta_arrays_sorted_by_entity_id() {
    let mut new = Graph::new();
    new.add_node(make_node("z_last", "behavior", "a.spec", 1));
    new.add_node(make_node("a_first", "feature", "a.spec", 5));
    new.add_node(make_node("m_middle", "invariant", "a.spec", 10));

    let delta = compute_graph_delta(&Graph::new(), &new);

    assert_eq!(ids(&delta.added_nodes), ["a_first", "m_middle", "z_last"]);
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "Compute Graph Delta: graph delta computation holds — previous_graph_available"
)]
fn compute_graph_delta_contract() {
    let mut old = Graph::new();
    old.add_node(make_node("a", "behavior", "a.spec", 1));
    old.add_node(make_node("b", "feature", "a.spec", 5));
    old.add_edge(edge("b", "a", "behaviors"));

    let mut new = Graph::new();
    let mut new_a = make_node("a", "behavior", "a.spec", 1);
    new_a
        .fields
        .push(Sym::new("status"), FieldValue::String("done".to_string()));
    new.add_node(new_a);
    new.add_node(make_node("c", "type", "a.spec", 10));

    let delta = compute_graph_delta(&old, &new);

    assert_eq!(ids(&delta.added_nodes), ["c"]);
    assert_eq!(ids(&delta.removed_nodes), ["b"]);
    assert_eq!(delta.modified_nodes.len(), 1);
    assert_eq!(delta.modified_nodes[0].id, "a");
    assert_eq!(delta.removed_edges, [change("b", "a", "behaviors")]);
    assert_eq!(compute_graph_delta(&new, &new), GraphDelta::default());
    assert_eq!(delta.applies(&old, &new), Ok(()));
}

/// old: a, b, c with b->a, b->c. new: a, b, d with b->a, b->d.
fn old_and_new() -> (Graph, Graph) {
    let mut old = Graph::new();
    old.add_node(make_node("a", "behavior", "a.spec", 1));
    old.add_node(make_node("b", "feature", "a.spec", 5));
    old.add_node(make_node("c", "type", "a.spec", 9));
    old.add_edge(edge("b", "a", "behaviors"));
    old.add_edge(edge("b", "c", "types"));
    let mut new = Graph::new();
    new.add_node(make_node("a", "behavior", "a.spec", 1));
    new.add_node(make_node("b", "feature", "a.spec", 5));
    new.add_node(make_node("d", "type", "a.spec", 9));
    new.add_edge(edge("b", "a", "behaviors"));
    new.add_edge(edge("b", "d", "types"));
    (old, new)
}

#[specforge_test(
    behavior = "validate_delta_correctness",
    verify = "delta applied to old graph equals new graph"
)]
fn the_true_delta_applies() {
    let (old, new) = old_and_new();
    let good = GraphDelta {
        added_nodes: vec![node_change("d")],
        removed_nodes: vec![node_change("c")],
        added_edges: vec![change("b", "d", "types")],
        removed_edges: vec![change("b", "c", "types")],
        ..GraphDelta::default()
    };
    assert_eq!(good.applies(&old, &new), Ok(()));
    assert_eq!(compute_graph_delta(&old, &new).applies(&old, &new), Ok(()));
}

#[specforge_test(
    behavior = "validate_delta_correctness",
    verify = "a discrepancy is reported with a message naming what differs"
)]
fn a_wrong_delta_is_rejected_naming_what_differs() {
    let (old, new) = old_and_new();
    let good = GraphDelta {
        added_nodes: vec![node_change("d")],
        removed_nodes: vec![node_change("c")],
        added_edges: vec![change("b", "d", "types")],
        removed_edges: vec![change("b", "c", "types")],
        ..GraphDelta::default()
    };

    // Counts balance, but the added edge points at the wrong target.
    let wrong_edge = GraphDelta {
        added_edges: vec![change("b", "x", "types")],
        ..good.clone()
    };
    let err = wrong_edge.applies(&old, &new).unwrap_err();
    assert!(
        err.contains("edge mismatch") && err.contains("b -types-> d"),
        "{err}"
    );

    // Counts balance, but it removes a node that never existed instead of c.
    let wrong_node = GraphDelta {
        removed_nodes: vec![node_change("zzz")],
        ..good.clone()
    };
    let err = wrong_node.applies(&old, &new).unwrap_err();
    assert!(
        err.contains("node mismatch") && err.contains("\"c\""),
        "{err}"
    );

    // A node it adds is not in the new graph.
    let phantom = GraphDelta {
        added_nodes: vec![node_change("d"), node_change("phantom")],
        ..good.clone()
    };
    let err = phantom.applies(&old, &new).unwrap_err();
    assert!(err.contains("phantom"), "{err}");

    // A node it calls modified is gone.
    let gone = GraphDelta {
        modified_nodes: vec![specforge_project::ModifiedNodeChange {
            id: "c".into(),
            changed_fields: Vec::new(),
            changed_aspects: vec![NodeAspect::Title],
            file: "a.spec".into(),
            line: 9,
        }],
        ..good
    };
    let err = gone.applies(&old, &new).unwrap_err();
    assert!(err.contains("modified node 'c'"), "{err}");
}

/// With `--verify-incremental` (or a debug build) every update checks its
/// delta against the graphs it joins (the session's tests); the check is
/// [`GraphDelta::applies`].
#[specforge_test(
    behavior = "validate_delta_correctness",
    verify = "Validate Delta Correctness: delta correctness validation holds — graph_delta_available, debug_mode_active, delta_verified, validation_event_emitted"
)]
fn validate_delta_correctness_contract() {
    let old = Graph::new();
    let mut new = Graph::new();
    new.add_node(make_node("a", "behavior", "a.spec", 1));
    new.add_node(make_node("b", "feature", "a.spec", 5));
    new.add_edge(edge("b", "a", "behaviors"));

    let delta = compute_graph_delta(&old, &new);
    assert_eq!(delta.applies(&old, &new), Ok(()));
    assert!(GraphDelta::default().applies(&old, &new).is_err());
}

/// A graph holding `x`, of kind `kind`, with a field named `kind` holding
/// `field` when given.
fn graph_with_kind_field(kind: &str, field: Option<&str>) -> Graph {
    let mut node = make_node("x", kind, "a.spec", 1);
    if let Some(field) = field {
        node.fields
            .push(Sym::new("kind"), FieldValue::String(field.to_string()));
    }
    let mut graph = Graph::new();
    graph.add_node(node);
    graph
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "a field named like an aspect is a changed field, never the aspect"
)]
fn a_field_named_like_an_aspect_is_a_field() {
    let field_changed = compute_graph_delta(
        &graph_with_kind_field("type", Some("struct")),
        &graph_with_kind_field("type", Some("enum")),
    );
    assert_eq!(field_changed.modified_nodes[0].changed_fields, ["kind"]);
    assert!(field_changed.modified_nodes[0].changed_aspects.is_empty());

    let kind_changed = compute_graph_delta(
        &graph_with_kind_field("type", None),
        &graph_with_kind_field("event", None),
    );
    assert!(kind_changed.modified_nodes[0].changed_fields.is_empty());
    assert_eq!(
        kind_changed.modified_nodes[0].changed_aspects,
        [NodeAspect::Kind]
    );
}
