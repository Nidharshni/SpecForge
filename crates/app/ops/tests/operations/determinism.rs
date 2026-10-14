use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue};
use specforge_test::prelude::*;

fn span() -> SourceSpan {
    SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 0,
    }
}

fn build_graph() -> Graph {
    let mut graph = Graph::new();

    // Add nodes in non-alphabetical order to test determinism
    for (id, kind, title) in [
        ("zebra", "feature", "Zebra Feature"),
        ("alpha", "behavior", "Alpha Behavior"),
        ("middle", "invariant", "Middle Invariant"),
    ] {
        let mut fields = FieldMap::new();
        fields.push(
            Sym::new("contract"),
            FieldValue::String(format!("Contract for {}", id)),
        );
        graph.add_node(Node {
            id: EntityId { raw: Sym::new(id) },
            kind: EntityKind {
                raw: Sym::new(kind),
            },
            title: Some(title.to_string()),
            fields,
            source_span: span(),
            methods: Vec::new(),
        });
    }

    graph.add_edge(Edge {
        source: Sym::new("zebra"),
        target: Sym::new("alpha"),
        label: Sym::new("behaviors"),
    });
    graph.add_edge(Edge {
        source: Sym::new("zebra"),
        target: Sym::new("middle"),
        label: Sym::new("invariants"),
    });

    graph
}

// B:deterministic_output — verify property "stats output is deterministic"
#[specforge_test(
    behavior = "deterministic_output",
    verify = "same input produces identical output across runs"
)]
fn stats_output_is_deterministic() {
    let outputs: Vec<_> = (0..5)
        .map(|_| {
            let stats = crate::view_support::stats_of(&build_graph(), &[], &[]);
            format!("{:?}", stats)
        })
        .collect();
    for output in &outputs[1..] {
        assert_eq!(
            &outputs[0], output,
            "stats output must be identical across runs"
        );
    }
}

// B:deterministic_output — verify property "trace output is deterministic"
#[specforge_test(
    behavior = "deterministic_output",
    verify = "same input produces identical output across runs"
)]
fn trace_output_is_deterministic() {
    let graph = build_graph();
    let outputs: Vec<_> = (0..5)
        .map(|_| {
            let project = crate::view_support::Project::of_graph(graph.clone(), Default::default());
            let outcome = specforge_ops::trace::trace(
                &project.view(),
                specforge_ops::trace::Target::Entity("alpha"),
            )
            .unwrap();
            serde_json::to_string_pretty(&outcome).unwrap()
        })
        .collect();
    for output in &outputs[1..] {
        assert_eq!(
            &outputs[0], output,
            "trace output must be identical across runs"
        );
    }
}
