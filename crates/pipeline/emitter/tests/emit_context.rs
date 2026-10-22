use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue, VerifyStatement};
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

fn rich_node() -> Node {
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("contract"),
        FieldValue::String("The system MUST do X".to_string()),
    );
    fields.push(
        Sym::new("description"),
        FieldValue::String("A very long verbose prose field that agents don't need".to_string()),
    );
    fields.push(
        Sym::new("status"),
        FieldValue::Identifier("done".to_string()),
    );
    Node {
        id: EntityId {
            raw: Sym::new("alpha"),
        },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: Some("Alpha Behavior".to_string()),
        fields,
        source_span: span(),
        methods: Vec::new(),
        obligations: vec![VerifyStatement {
            kind: "unit".to_string(),
            description: "it works".to_string(),
        }],
    }
}

// B:export_agent_context_format — verify unit "context format includes entity IDs and contracts"
#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "context format includes entity IDs and contracts"
)]
fn context_includes_contracts_and_verify() {
    let mut graph = Graph::new();
    graph.add_node(rich_node());

    let json = crate::export(
        &graph,
        specforge_emitter::EmitFormat::Context,
        Some(&crate::support::headline_build(&["behavior", "feature"])),
    );
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let node = &parsed["nodes"].as_array().unwrap()[0];

    // Context includes contract
    assert_eq!(node["contract"], "The system MUST do X");

    // Context includes verify declarations
    let verify = node["verify"].as_array().unwrap();
    assert_eq!(verify.len(), 1);
    assert_eq!(verify[0]["kind"], "unit");

    // Context includes status
    assert_eq!(node["status"], "done");
}

// B:export_agent_context_format — verify unit "context format omits verbose prose fields"
#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "context format omits verbose prose fields"
)]
fn context_omits_verbose_prose_fields() {
    let mut graph = Graph::new();
    graph.add_node(rich_node());

    let json = crate::export(&graph, specforge_emitter::EmitFormat::Context, None);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let node = &parsed["nodes"].as_array().unwrap()[0];

    // Context omits description (verbose prose)
    assert!(
        node.get("description").is_none() || node["description"].is_null(),
        "context must omit description"
    );
}

// B:export_agent_context_format — verify unit "output includes schema_version field"
// B:export_agent_context_format — verify unit "output conforms to Graph Protocol schema"
#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "output includes schema_version field"
)]
fn context_includes_edges_and_schema_version() {
    let mut graph = Graph::new();
    graph.add_node(rich_node());
    let mut node_b = Node {
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
    };
    node_b.fields.push(
        Sym::new("contract"),
        FieldValue::String("Feature contract".to_string()),
    );
    graph.add_node(node_b);
    graph.add_edge(Edge {
        source: Sym::new("beta"),
        target: Sym::new("alpha"),
        label: Sym::new("behaviors"),
    });

    let json = crate::export(&graph, specforge_emitter::EmitFormat::Context, None);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    assert!(parsed["schema_version"].is_string());
    assert_eq!(parsed["edges"].as_array().unwrap().len(), 1);
}

// B:export_agent_context_format — verify unit "output conforms to Graph Protocol schema"
#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "output conforms to Graph Protocol schema"
)]
fn context_conforms_to_graph_protocol_schema() {
    let mut graph = Graph::new();
    graph.add_node(rich_node());

    let json = crate::export(&graph, specforge_emitter::EmitFormat::Context, None);
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

// B:export_agent_context_format — verify unit "context format omits verbose prose fields"
// (demonstrates token optimization by size comparison)
#[specforge_test(behavior = "export_agent_context_format")]
fn context_is_smaller_than_full_json() {
    let mut graph = Graph::new();
    graph.add_node(rich_node());

    let context = crate::export(&graph, specforge_emitter::EmitFormat::Context, None);
    let full = crate::support::emit_json(&graph);
    assert!(
        context.len() < full.len(),
        "context ({}) should be smaller than full ({})",
        context.len(),
        full.len()
    );
}

#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "context format keeps each entity's normative fields"
)]
fn context_keeps_normative_fields() {
    use crate::support::{build_of, field, kind};

    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("guarantee"),
        FieldValue::String("Ids MUST be unique".to_string()),
    );
    fields.push(
        Sym::new("description"),
        FieldValue::String("Long prose about ids".to_string()),
    );
    let mut graph = Graph::new();
    graph.add_node(Node {
        id: EntityId {
            raw: Sym::new("unique_ids"),
        },
        kind: EntityKind {
            raw: Sym::new("invariant"),
        },
        title: None,
        fields,
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    });
    let registries = build_of(vec![kind(
        "invariant",
        vec![
            field("guarantee", true, false),
            field("description", false, false),
        ],
    )]);

    let emit = |registries| {
        let json = crate::export(&graph, specforge_emitter::EmitFormat::Context, registries);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        parsed["nodes"][0].clone()
    };
    let node = emit(Some(&registries));
    assert_eq!(node["fields"]["guarantee"], "Ids MUST be unique", "{node}");
    assert!(
        node["fields"].get("description").is_none(),
        "prose stays out"
    );
    assert!(
        emit(None).get("fields").is_none(),
        "no extension, no fields"
    );
}

#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "context format includes entity IDs and contracts"
)]
fn context_lifts_the_fields_an_extension_declares_headline() {
    use crate::support::headline_build;

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
    for (id, kind) in [("b", "behavior"), ("t", "type")] {
        graph.add_node(Node {
            id: EntityId { raw: Sym::new(id) },
            kind: EntityKind {
                raw: Sym::new(kind),
            },
            title: None,
            fields: fields.clone(),
            source_span: span(),
            methods: Vec::new(),
            obligations: Vec::new(),
        });
    }
    // Only `behavior` declares the two as headline fields; on `type` they
    // are struct members.
    let registries = headline_build(&["behavior"]);
    let json = crate::export(
        &graph,
        specforge_emitter::EmitFormat::Context,
        Some(&registries),
    );
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let node = |id: &str| {
        parsed["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap()
            .clone()
    };
    let b = node("b");
    assert_eq!(b["contract"], "The system MUST do X", "{b}");
    assert_eq!(b["status"], "done", "{b}");
    assert!(b.get("fields").is_none(), "a headline is not repeated: {b}");
    let t = node("t");
    assert!(
        t.get("contract").is_none() && t.get("status").is_none(),
        "{t}"
    );
}

#[test]
fn a_headline_field_named_coverage_status_is_not_lifted() {
    use crate::support::{build_of, field, kind};
    use std::collections::BTreeMap;

    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("coverage_status"),
        FieldValue::String("x".to_string()),
    );
    let mut graph = Graph::new();
    graph.add_node(Node {
        id: EntityId { raw: Sym::new("b") },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: None,
        fields,
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    });
    let registries = build_of(vec![kind(
        "behavior",
        vec![field("coverage_status", false, true)],
    )]);
    let verdicts = BTreeMap::from([("b".to_string(), specforge_coverage::Status::Covered)]);
    let emit = |coverage| {
        specforge_emitter::emit(
            &graph,
            &specforge_emitter::EmitOptions {
                format: specforge_emitter::EmitFormat::Context,
                registries: &registries,
                coverage,
                ..Default::default()
            },
        )
        .unwrap()
    };
    let plain: serde_json::Value = serde_json::from_str(&emit(None)).unwrap();
    assert!(plain["nodes"][0].get("coverage_status").is_none());
    let text = emit(Some(&verdicts));
    assert_eq!(text.matches("\"coverage_status\"").count(), 1, "{text}");
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["nodes"][0]["coverage_status"], "covered");
}
