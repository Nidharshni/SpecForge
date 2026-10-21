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

// B:export_agent_graph_format — verify unit "graph format includes all fields and metadata"
#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "graph format includes all fields and metadata"
)]
fn emit_graph_includes_all_fields_and_metadata() {
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("contract"),
        FieldValue::String("Must do X".to_string()),
    );
    fields.push(
        Sym::new("status"),
        FieldValue::Identifier("done".to_string()),
    );

    let mut graph = Graph::new();
    graph.add_node(Node {
        id: EntityId {
            raw: Sym::new("alpha"),
        },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: Some("Alpha".to_string()),
        fields,
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    });

    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let node = &parsed["nodes"].as_array().unwrap()[0];
    // Full fidelity: includes fields, file, line
    assert_eq!(node["fields"]["contract"], "Must do X");
    assert_eq!(node["fields"]["status"], "done");
    assert_eq!(node["file"], "test.spec");
    assert_eq!(node["line"], 1);
    assert!(parsed["schema_version"].is_string());
}

// B:export_agent_graph_format — verify unit "graph format includes all nodes and edges"
// (graph format is identical to full JSON — validates full fidelity)
#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "graph format includes all nodes and edges"
)]
fn emit_graph_equals_emit_json() {
    let mut graph = Graph::new();
    // Added out of ID order, with a disconnected node.
    for (id, kind) in [
        ("gamma", "invariant"),
        ("alpha", "behavior"),
        ("delta", "feature"),
        ("beta", "behavior"),
    ] {
        graph.add_node(Node {
            id: EntityId { raw: Sym::new(id) },
            kind: EntityKind {
                raw: Sym::new(kind),
            },
            title: Some(id.to_uppercase()),
            fields: FieldMap::new(),
            source_span: span(),
            methods: Vec::new(),
            obligations: Vec::new(),
        });
    }
    for (source, target, label) in [
        ("beta", "gamma", "invariants"),
        ("alpha", "gamma", "invariants"),
        ("alpha", "beta", "depends_on"),
    ] {
        graph.add_edge(Edge {
            source: Sym::new(source),
            target: Sym::new(target),
            label: Sym::new(label),
        });
    }

    // The graph format is what `specforge export --format graph` emits.
    let graph_output = crate::support::emit_json(&graph);
    assert_eq!(
        graph_output,
        specforge_emitter::emit(&graph, &specforge_emitter::EmitOptions::default()).unwrap()
    );
    let parsed: serde_json::Value = serde_json::from_str(&graph_output).unwrap();

    let ids: Vec<&str> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["alpha", "beta", "delta", "gamma"]);
    assert_eq!(
        parsed["edges"],
        serde_json::json!([
            { "source": "alpha", "target": "beta", "label": "depends_on" },
            { "source": "alpha", "target": "gamma", "label": "invariants" },
            { "source": "beta", "target": "gamma", "label": "invariants" },
        ])
    );
}

// B:export_agent_graph_format — verify unit "output conforms to Graph Protocol schema"
#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "output conforms to Graph Protocol schema"
)]
fn graph_format_conforms_to_graph_protocol_schema() {
    let mut graph = Graph::new();
    graph.add_node(Node {
        id: EntityId {
            raw: Sym::new("alpha"),
        },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: Some("Alpha".to_string()),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    });
    graph.add_node(Node {
        id: EntityId {
            raw: Sym::new("beta"),
        },
        kind: EntityKind {
            raw: Sym::new("feature"),
        },
        title: Some("Beta".to_string()),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    });
    graph.add_edge(Edge {
        source: Sym::new("beta"),
        target: Sym::new("alpha"),
        label: Sym::new("behaviors"),
    });

    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    // Graph Protocol requires: schema_version, nodes array, edges array
    assert!(
        parsed["schema_version"].is_string(),
        "must have schema_version"
    );
    assert!(parsed["nodes"].is_array(), "must have nodes array");
    assert!(parsed["edges"].is_array(), "must have edges array");

    // Each node must have id and kind
    for node in parsed["nodes"].as_array().unwrap() {
        assert!(node["id"].is_string(), "each node must have id");
        assert!(node["kind"].is_string(), "each node must have kind");
    }
}

// B:export_agent_graph_format — verify unit "output includes schema_version field"
#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "output includes schema_version field"
)]
fn graph_format_includes_schema_version() {
    let mut graph = Graph::new();
    graph.add_node(Node {
        id: EntityId {
            raw: Sym::new("alpha"),
        },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: Some("Alpha".to_string()),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    });

    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let version = parsed["schema_version"]
        .as_str()
        .expect("schema_version must be a string");
    assert!(!version.is_empty(), "schema_version must not be empty");
}

// B:export_agent_graph_format — verify integration "structural-only graph exports valid JSON with raw keyword strings as entity kinds"
#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "structural-only graph exports valid JSON with raw keyword strings as entity kinds"
)]
fn structural_only_graph_exports_raw_keywords() {
    let mut graph = Graph::new();
    // Use non-standard keywords (zero-extension structural-only graph)
    graph.add_node(Node {
        id: EntityId {
            raw: Sym::new("my_widget"),
        },
        kind: EntityKind {
            raw: Sym::new("widget"),
        },
        title: Some("My Widget".to_string()),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    });
    graph.add_node(Node {
        id: EntityId {
            raw: Sym::new("my_gadget"),
        },
        kind: EntityKind {
            raw: Sym::new("gadget"),
        },
        title: Some("My Gadget".to_string()),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    });

    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let nodes = parsed["nodes"].as_array().unwrap();
    // Raw keyword strings preserved as-is in kind field
    let kinds: Vec<&str> = nodes.iter().map(|n| n["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"widget"), "raw keyword 'widget' preserved");
    assert!(kinds.contains(&"gadget"), "raw keyword 'gadget' preserved");
    assert!(parsed["schema_version"].is_string());
}

#[test]
fn without_verdicts_no_node_carries_a_coverage_status() {
    use specforge_emitter::{EmitFormat, EmitOptions};
    use std::collections::BTreeMap;

    let mut graph = Graph::new();
    for id in ["alpha", "beta"] {
        graph.add_node(Node {
            id: EntityId { raw: Sym::new(id) },
            kind: EntityKind {
                raw: Sym::new("behavior"),
            },
            title: Some(id.to_string()),
            fields: FieldMap::new(),
            source_span: span(),
            methods: Vec::new(),
            obligations: Vec::new(),
        });
    }
    let verdicts = BTreeMap::from([(
        "alpha".to_string(),
        specforge_coverage::Verdict {
            obligations: 1,
            proven: 1,
            ..Default::default()
        },
    )]);
    for format in [EmitFormat::Json, EmitFormat::Context, EmitFormat::Brief] {
        let emit = |coverage| {
            specforge_emitter::emit(
                &graph,
                &EmitOptions {
                    format,
                    coverage,
                    ..EmitOptions::default()
                },
            )
            .unwrap()
        };
        assert!(!emit(None).contains("coverage_status"), "{format:?}");
        let parsed: serde_json::Value = serde_json::from_str(&emit(Some(&verdicts))).unwrap();
        let nodes = parsed["nodes"].as_array().unwrap();
        let status =
            |id: &str| nodes.iter().find(|n| n["id"] == id).unwrap()["coverage_status"].clone();
        assert_eq!(status("alpha"), "covered", "{format:?}");
        assert_eq!(status("beta"), serde_json::Value::Null, "{format:?}");
    }
}
