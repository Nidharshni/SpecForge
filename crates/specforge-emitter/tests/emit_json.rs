use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_parser::{
    EntityId, EntityKind, FieldMap, FieldValue, SpannedRef, VerifyStatement, parse_expression,
};
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

fn node(id: &str, kind: &str, title: Option<&str>) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: title.map(|s| s.to_string()),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
    }
}

// B:serialize_json_graph — verify unit "output is valid JSON"
// B:serialize_json_graph — verify unit "empty graph produces valid JSON with empty nodes and edges arrays"
#[specforge_test(behavior = "serialize_json_graph", verify = "output is valid JSON")]
fn empty_graph_produces_valid_json_with_empty_arrays() {
    let graph = Graph::new();
    let json = specforge_emitter::json::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    assert_eq!(parsed["nodes"], serde_json::json!([]));
    assert_eq!(parsed["edges"], serde_json::json!([]));
    assert!(
        parsed["schema_version"].is_string(),
        "schema_version must be present"
    );
}

// B:serialize_json_graph — verify unit "JSON output contains all nodes"
#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "JSON output contains all nodes"
)]
fn json_contains_all_nodes() {
    let mut graph = Graph::new();
    graph.add_node(node("alpha", "behavior", Some("Alpha Behavior")));
    graph.add_node(node("beta", "feature", Some("Beta Feature")));

    let json = specforge_emitter::json::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let nodes = parsed["nodes"].as_array().expect("nodes is array");
    assert_eq!(nodes.len(), 2);

    // Nodes sorted by ID for determinism
    assert_eq!(nodes[0]["id"], "alpha");
    assert_eq!(nodes[0]["kind"], "behavior");
    assert_eq!(nodes[0]["title"], "Alpha Behavior");
    assert_eq!(nodes[1]["id"], "beta");
    assert_eq!(nodes[1]["kind"], "feature");
    assert_eq!(nodes[1]["title"], "Beta Feature");
}

// B:serialize_json_graph — verify unit "JSON output contains all edges"
#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "JSON output contains all edges"
)]
fn json_contains_all_edges() {
    let mut graph = Graph::new();
    graph.add_node(node("feat_a", "feature", Some("Feature A")));
    graph.add_node(node("beh_b", "behavior", None));
    graph.add_edge(Edge {
        source: Sym::new("feat_a"),
        target: Sym::new("beh_b"),
        label: Sym::new("behaviors"),
    });

    let json = specforge_emitter::json::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let edges = parsed["edges"].as_array().expect("edges is array");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["source"], "feat_a");
    assert_eq!(edges[0]["target"], "beh_b");
    assert_eq!(edges[0]["label"], "behaviors");
}

// B:serialize_json_graph — verify unit "output includes schema_version field"
#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "output includes schema_version field"
)]
fn json_includes_schema_version() {
    let graph = Graph::new();
    let json = specforge_emitter::json::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let version = parsed["schema_version"]
        .as_str()
        .expect("schema_version is string");
    assert!(!version.is_empty());
}

// B:serialize_json_graph — verify unit "JSON output contains all nodes"
// (covers field serialization within nodes)
#[specforge_test(behavior = "serialize_json_graph")]
fn json_includes_fields() {
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("contract"),
        FieldValue::String("The system MUST do X".to_string()),
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
    });

    let json = specforge_emitter::json::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let node = &parsed["nodes"].as_array().unwrap()[0];
    let fields = &node["fields"];
    assert_eq!(fields["contract"], "The system MUST do X");
    assert_eq!(fields["status"], "done");
}

// B:serialize_json_graph — verify unit "empty graph produces valid JSON with empty nodes and edges arrays"
#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "empty graph produces valid JSON with empty nodes and edges arrays"
)]
fn empty_graph_valid_json_with_empty_arrays() {
    let graph = Graph::new();
    let json = specforge_emitter::json::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    assert_eq!(parsed["nodes"], serde_json::json!([]));
    assert_eq!(parsed["edges"], serde_json::json!([]));
}

// B:serialize_json_graph — verify unit "schema is included even for empty graph"
#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "schema is included even for empty graph"
)]
fn schema_included_even_for_empty_graph() {
    let graph = Graph::new();
    let json = specforge_emitter::json::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let version = parsed["schema_version"]
        .as_str()
        .expect("schema_version is string");
    assert!(!version.is_empty());
}

// B:serialize_json_graph — verify integration "structural-only graph (zero extensions) produces valid Graph Protocol JSON with raw keywords in kind field"
#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "structural-only graph (zero extensions) produces valid Graph Protocol JSON with raw keywords in kind field"
)]
fn structural_only_graph_produces_valid_json_with_raw_keywords() {
    let mut graph = Graph::new();
    // Use non-standard keywords (simulating zero-extension / structural-only graph)
    graph.add_node(node("my_entity", "custom_kind", Some("Custom Entity")));
    graph.add_node(node("another", "unknown_type", None));

    let json = specforge_emitter::json::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let nodes = parsed["nodes"].as_array().unwrap();
    // Nodes are sorted by ID: "another" < "my_entity"
    let kinds: Vec<&str> = nodes.iter().map(|n| n["kind"].as_str().unwrap()).collect();
    assert!(
        kinds.contains(&"custom_kind"),
        "raw keyword preserved in kind field"
    );
    assert!(
        kinds.contains(&"unknown_type"),
        "raw keyword preserved in kind field"
    );
    assert!(parsed["schema_version"].is_string());
}

// B:serialize_json_graph — verify unit "JSON output contains all nodes"
// (covers source location metadata within nodes)
#[specforge_test(behavior = "serialize_json_graph")]
fn json_includes_source_location() {
    let mut graph = Graph::new();
    graph.add_node(Node {
        id: EntityId {
            raw: Sym::new("alpha"),
        },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: None,
        fields: FieldMap::new(),
        source_span: SourceSpan {
            file: Sym::new("behaviors/core.spec"),
            start_line: 10,
            start_col: 0,
            end_line: 25,
            end_col: 1,
        },
        methods: Vec::new(),
    });

    let json = specforge_emitter::json::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let node = &parsed["nodes"].as_array().unwrap()[0];
    assert_eq!(node["file"], "behaviors/core.spec");
    assert_eq!(node["line"], 10);
}

// -- A graph built with no extension still exports (moved from the
// registry's detection tests).

#[specforge_test(
    behavior = "graceful_degradation_without_extensions",
    verify = "structural parsing works without extensions"
)]
fn structural_parsing_works_without_extensions() {
    let source = "behavior my_beh \"Title\" {\n  contract \"test\"\n}\n";
    let parsed = specforge_parser::parse(source, "test.spec");
    assert_eq!(parsed.entities.len(), 1);
}

#[specforge_test(
    behavior = "graceful_degradation_without_extensions",
    verify = "graph built with generic nodes"
)]
fn the_graph_is_built_with_generic_nodes() {
    let source = "behavior my_beh \"Title\" {\n  contract \"test\"\n}\n";
    let parsed = specforge_parser::parse(source, "test.spec");
    let (graph, _) = specforge_graph::build_graph(&[parsed]);
    assert_eq!(graph.node_count(), 1);
    assert_eq!(graph.node("my_beh").unwrap().kind.raw, "behavior");
}

#[specforge_test(
    behavior = "graceful_degradation_without_extensions",
    verify = "specforge export produces valid JSON from structural-only graph"
)]
fn export_is_valid_json_for_a_structural_only_graph() {
    let source = "thing my_thing \"A Thing\" {\n  data \"hello\"\n}\n";
    let parsed = specforge_parser::parse(source, "test.spec");
    let (graph, _) = specforge_graph::build_graph(&[parsed]);
    let json = specforge_emitter::json::emit_json(&graph);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(value.is_object());
}

#[specforge_test(
    behavior = "graceful_degradation_without_extensions",
    verify = "generic entity nodes appear as nodes in exported graph"
)]
fn generic_nodes_appear_in_the_exported_graph() {
    let source = "thing my_thing \"A Thing\" {\n  data \"hello\"\n}\n";
    let parsed = specforge_parser::parse(source, "test.spec");
    let (graph, _) = specforge_graph::build_graph(&[parsed]);
    let json = specforge_emitter::json::emit_json(&graph);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let nodes = value["nodes"].as_array().unwrap();
    assert!(nodes.iter().any(|n| n["id"] == "my_thing"));
}

#[specforge_test(
    behavior = "graceful_degradation_without_extensions",
    verify = "references between generic entities produce edges"
)]
fn references_between_generic_entities_produce_edges() {
    let source = "thing a \"A\" {\n  refs [b]\n}\nthing b \"B\" {\n  data \"ok\"\n}\n";
    let parsed = specforge_parser::parse(source, "test.spec");
    let (graph, _) = specforge_graph::build_graph(&[parsed]);
    let json = specforge_emitter::json::emit_json(&graph);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let edges = value["edges"].as_array().unwrap();
    assert!(
        edges
            .iter()
            .any(|e| e["source"] == "a" && e["target"] == "b"),
        "expected edge from a to b, got: {edges:?}"
    );
}

// -- The wire form of a field value (ADR-NEW-01, plan 01).

fn exported_fields(fields: FieldMap) -> serde_json::Value {
    let mut graph = Graph::new();
    let mut n = node("alpha", "behavior", None);
    n.fields = fields;
    graph.add_node(n);
    let json = specforge_emitter::json::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    parsed["nodes"][0]["fields"].clone()
}

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "an expression group is written as its conjuncts, one per line, never as a parse tree"
)]
fn an_expression_group_is_written_as_its_conjuncts_one_per_line() {
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("metric"),
        FieldValue::Expression(vec![
            parse_expression("a < 1").unwrap(),
            parse_expression("b >= 2ms").unwrap(),
        ]),
    );
    let written = exported_fields(fields);
    assert_eq!(written["metric"], serde_json::json!("a < 1\nb >= 2ms"));
}

#[test]
fn every_other_value_is_written_as_today() {
    let mut inner = FieldMap::new();
    inner.push(Sym::new("m"), FieldValue::Integer(1));
    let mut block = FieldMap::new();
    block.push(Sym::new("k"), FieldValue::String("v".into()));
    block.push(Sym::new("n"), FieldValue::Block(inner));

    let reference = |id: &str| SpannedRef {
        id: id.to_string(),
        span: span(),
    };
    let mut fields = FieldMap::new();
    fields.push(Sym::new("string"), FieldValue::String("s".into()));
    fields.push(Sym::new("identifier"), FieldValue::Identifier("id".into()));
    fields.push(Sym::new("date"), FieldValue::Date("2026-10-13".into()));
    fields.push(
        Sym::new("union"),
        FieldValue::TypeUnion(vec!["a".into(), "b".into()]),
    );
    fields.push(Sym::new("integer"), FieldValue::Integer(3));
    fields.push(Sym::new("boolean"), FieldValue::Boolean(true));
    fields.push(
        Sym::new("references"),
        FieldValue::ReferenceList(vec![reference("x"), reference("y")]),
    );
    fields.push(
        Sym::new("variants"),
        FieldValue::VariantList(vec!["a".into(), "b".into()]),
    );
    fields.push(
        Sym::new("strings"),
        FieldValue::StringList(vec!["s".into()]),
    );
    fields.push(
        Sym::new("verify"),
        FieldValue::VerifyList(vec![VerifyStatement {
            kind: "unit".into(),
            description: "d".into(),
        }]),
    );
    fields.push(Sym::new("block"), FieldValue::Block(block));
    fields.push(
        Sym::new("mixed"),
        FieldValue::MixedList(vec![FieldValue::Integer(1), FieldValue::String("a".into())]),
    );

    assert_eq!(
        exported_fields(fields),
        serde_json::json!({
            "string": "s",
            "identifier": "id",
            "date": "2026-10-13",
            "union": "a | b",
            "integer": 3,
            "boolean": true,
            "references": ["x", "y"],
            "variants": ["a", "b"],
            "strings": ["s"],
            "verify": [{"kind": "unit", "description": "d"}],
            "block": {"k": "v", "n": {"m": 1}},
            "mixed": [1, "a"],
        })
    );
}
