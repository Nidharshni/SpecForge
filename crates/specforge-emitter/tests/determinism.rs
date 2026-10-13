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

// B:deterministic_output — verify property "same input produces identical output across runs"
#[specforge_test(
    behavior = "deterministic_output",
    verify = "same input produces identical output across runs"
)]
fn same_input_produces_identical_json_across_runs() {
    let outputs: Vec<String> = (0..5)
        .map(|_| specforge_emitter::json::emit_json(&build_graph()))
        .collect();
    for output in &outputs[1..] {
        assert_eq!(
            &outputs[0], output,
            "JSON output must be identical across runs"
        );
    }
}

// B:deterministic_output — verify property "same input produces identical output across runs"
// (DOT format)
#[specforge_test(behavior = "deterministic_output")]
fn same_input_produces_identical_dot_across_runs() {
    let outputs: Vec<String> = (0..5)
        .map(|_| {
            specforge_emitter::dot::emit_dot(
                &build_graph(),
                &specforge_emitter::DotOptions::default(),
            )
        })
        .collect();
    for output in &outputs[1..] {
        assert_eq!(
            &outputs[0], output,
            "DOT output must be identical across runs"
        );
    }
}

// B:deterministic_output — verify property "same input produces identical output across runs"
// (brief format)
#[specforge_test(behavior = "deterministic_output")]
fn same_input_produces_identical_brief_across_runs() {
    let outputs: Vec<String> = (0..5)
        .map(|_| crate::export(&build_graph(), specforge_emitter::EmitFormat::Brief, None))
        .collect();
    for output in &outputs[1..] {
        assert_eq!(
            &outputs[0], output,
            "Brief output must be identical across runs"
        );
    }
}

// B:deterministic_output — verify property "same input produces identical output across runs"
// (context format)
#[specforge_test(behavior = "deterministic_output")]
fn same_input_produces_identical_context_across_runs() {
    let outputs: Vec<String> = (0..5)
        .map(|_| crate::export(&build_graph(), specforge_emitter::EmitFormat::Context, None))
        .collect();
    for output in &outputs[1..] {
        assert_eq!(
            &outputs[0], output,
            "Context output must be identical across runs"
        );
    }
}

// B:deterministic_output — verify unit "entity ordering is independent of hashmap iteration"
#[specforge_test(
    behavior = "deterministic_output",
    verify = "entity ordering is independent of hashmap iteration"
)]
fn json_nodes_sorted_by_id() {
    let graph = build_graph();
    let json = specforge_emitter::json::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ids: Vec<&str> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    // Nodes added as zebra, alpha, middle — output must be sorted: alpha, middle, zebra
    assert_eq!(ids, vec!["alpha", "middle", "zebra"]);
}

// B:deterministic_output — verify unit "edge ordering is deterministic"
#[specforge_test(
    behavior = "deterministic_output",
    verify = "edge ordering is independent of hashmap iteration"
)]
fn json_edges_sorted_deterministically() {
    // The same edges, inserted in two opposite orders, none of them sorted.
    let edges = [
        ("zebra", "middle", "invariants"),
        ("alpha", "zebra", "depends_on"),
        ("zebra", "alpha", "behaviors"),
        ("middle", "alpha", "refs"),
        ("zebra", "alpha", "aliases"),
    ];
    let with_edges = |order: Vec<&(&str, &str, &str)>| -> Graph {
        let base = build_graph();
        let mut graph = Graph::new();
        for node in base.nodes() {
            graph.add_node(node.clone());
        }
        for (source, target, label) in order {
            graph.add_edge(Edge {
                source: Sym::new(source),
                target: Sym::new(target),
                label: Sym::new(label),
            });
        }
        graph
    };
    let forward = specforge_emitter::json::emit_json(&with_edges(edges.iter().collect()));
    let backward = specforge_emitter::json::emit_json(&with_edges(edges.iter().rev().collect()));
    assert_eq!(forward, backward, "edge insertion order must not matter");

    // Sorted by source, then target, then label.
    let parsed: serde_json::Value = serde_json::from_str(&forward).unwrap();
    let listed: Vec<(&str, &str, &str)> = parsed["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["source"].as_str().unwrap(),
                e["target"].as_str().unwrap(),
                e["label"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        vec![
            ("alpha", "zebra", "depends_on"),
            ("middle", "alpha", "refs"),
            ("zebra", "alpha", "aliases"),
            ("zebra", "alpha", "behaviors"),
            ("zebra", "middle", "invariants"),
        ]
    );

    // DOT lists edges in the same order.
    let graph = with_edges(edges.iter().rev().collect());
    let dot = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());
    let dot_edges: Vec<&str> = dot.lines().filter(|l| l.contains("->")).collect();
    assert_eq!(
        dot_edges,
        vec![
            "  \"alpha\" -> \"zebra\" [label=\"depends_on\"];",
            "  \"middle\" -> \"alpha\" [label=\"refs\"];",
            "  \"zebra\" -> \"alpha\" [label=\"aliases\"];",
            "  \"zebra\" -> \"alpha\" [label=\"behaviors\"];",
            "  \"zebra\" -> \"middle\" [label=\"invariants\"];",
        ]
    );
}

// B:deterministic_output — verify unit "output contains no timestamps or non-deterministic values"
#[specforge_test(
    behavior = "deterministic_output",
    verify = "output contains no timestamps or non-deterministic values"
)]
fn json_output_contains_no_timestamps() {
    let graph = build_graph();
    let emit_all = |graph: &Graph| -> Vec<String> {
        vec![
            specforge_emitter::json::emit_json(graph),
            crate::export(graph, specforge_emitter::EmitFormat::Context, None),
            crate::export(graph, specforge_emitter::EmitFormat::Brief, None),
            specforge_emitter::dot::emit_dot(graph, &specforge_emitter::DotOptions::default()),
        ]
    };

    // Emitted across a clock tick, every format is byte-identical: nothing
    // time-based or random is in the output.
    let first = emit_all(&graph);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert_eq!(first, emit_all(&graph));

    // Every value in the JSON formats comes from the graph: the only strings
    // are the fixture's own text, the version, and field names; the only
    // numbers are source lines.
    let fixture_text = [
        "zebra",
        "alpha",
        "middle",
        "feature",
        "behavior",
        "invariant",
        "Zebra Feature",
        "Alpha Behavior",
        "Middle Invariant",
        "Contract for zebra",
        "Contract for alpha",
        "Contract for middle",
        "behaviors",
        "invariants",
        "test.spec",
        "1.0.0",
        "1.0",
    ];
    fn check(value: &serde_json::Value, allowed: &[&str], at: &str) {
        match value {
            serde_json::Value::String(s) => {
                assert!(
                    allowed.contains(&s.as_str()),
                    "{at}: unexpected value {s:?}"
                )
            }
            serde_json::Value::Number(n) => assert_eq!(n.as_u64(), Some(1), "{at}: {n}"),
            serde_json::Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    check(item, allowed, &format!("{at}/{i}"));
                }
            }
            serde_json::Value::Object(map) => {
                for (key, item) in map {
                    check(item, allowed, &format!("{at}/{key}"));
                }
            }
            other => panic!("{at}: unexpected value {other}"),
        }
    }
    for output in &first[..3] {
        let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
        check(&parsed, &fixture_text, "#");
    }
}

// B:deterministic_output — verify unit "file emission order is independent of filesystem readdir order"
#[specforge_test(
    behavior = "deterministic_output",
    verify = "file emission order is independent of filesystem readdir order"
)]
fn file_emission_order_independent_of_filesystem() {
    // Build two graphs with nodes from different "files" added in different orders
    let mut graph1 = Graph::new();
    let mut graph2 = Graph::new();

    let files = ["z_file.spec", "a_file.spec", "m_file.spec"];
    let ids = ["node_z", "node_a", "node_m"];

    // Graph1: add in order z, a, m
    for i in 0..3 {
        let mut fields = FieldMap::new();
        fields.push(
            Sym::new("contract"),
            FieldValue::String(format!("Contract {}", ids[i])),
        );
        graph1.add_node(Node {
            id: EntityId {
                raw: Sym::new(ids[i]),
            },
            kind: EntityKind {
                raw: Sym::new("behavior"),
            },
            title: Some(format!("Title {}", ids[i])),
            fields,
            source_span: SourceSpan {
                file: Sym::new(files[i]),
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 0,
            },
            methods: Vec::new(),
        });
    }

    // Graph2: add in reverse order m, a, z
    for i in (0..3).rev() {
        let mut fields = FieldMap::new();
        fields.push(
            Sym::new("contract"),
            FieldValue::String(format!("Contract {}", ids[i])),
        );
        graph2.add_node(Node {
            id: EntityId {
                raw: Sym::new(ids[i]),
            },
            kind: EntityKind {
                raw: Sym::new("behavior"),
            },
            title: Some(format!("Title {}", ids[i])),
            fields,
            source_span: SourceSpan {
                file: Sym::new(files[i]),
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 0,
            },
            methods: Vec::new(),
        });
    }

    let json1 = specforge_emitter::json::emit_json(&graph1);
    let json2 = specforge_emitter::json::emit_json(&graph2);
    assert_eq!(
        json1, json2,
        "output must be identical regardless of node insertion order (simulating different readdir orders)"
    );
}

// B:deterministic_output — verify property "scoped emit is deterministic"
#[specforge_test(
    behavior = "deterministic_output",
    verify = "same input produces identical output across runs"
)]
fn scoped_json_output_is_deterministic() {
    let graph = build_graph();
    let outputs: Vec<_> = (0..5)
        .map(|_| crate::support::scoped_json(&graph, "zebra").unwrap())
        .collect();
    for output in &outputs[1..] {
        assert_eq!(
            &outputs[0], output,
            "scoped JSON output must be identical across runs"
        );
    }
}

// B:deterministic_output — verify property "scoped context emit is deterministic"
#[specforge_test(
    behavior = "deterministic_output",
    verify = "same input produces identical output across runs"
)]
fn scoped_context_output_is_deterministic() {
    let graph = build_graph();
    let outputs: Vec<_> = (0..5)
        .map(|_| crate::support::scoped_context(&graph, "zebra").unwrap())
        .collect();
    for output in &outputs[1..] {
        assert_eq!(
            &outputs[0], output,
            "scoped context output must be identical across runs"
        );
    }
}
