use specforge_common::{Diagnostic, Severity, SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_parser::{FieldMap, FieldValue, VerifyStatement};
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

fn node_with_fields(id: &str, kind: &str, contract: &str, status: &str) -> Node {
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("contract"),
        FieldValue::String(contract.to_string()),
    );
    fields.push(
        Sym::new("status"),
        FieldValue::Identifier(status.to_string()),
    );
    Node {
        title: Some(format!("Title {}", id)),
        fields,
        ..Node::new(id, kind, span())
    }
}

fn testable_node(id: &str) -> Node {
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("contract"),
        FieldValue::String("The system MUST work".to_string()),
    );
    Node {
        title: Some(format!("Title {}", id)),
        fields,
        obligations: vec![VerifyStatement {
            kind: "unit".to_string(),
            description: "it works".to_string(),
        }],
        ..Node::new(id, "behavior", span())
    }
}

/// A node with a description, so tests can check prose is left out.
fn described_node(id: &str) -> Node {
    let mut node = testable_node(id);
    node.fields.push(
        Sym::new("description"),
        FieldValue::String("Long prose that explains the entity at length".to_string()),
    );
    node
}

/// The IDs of the `nodes` array of a JSON export, in output order.
fn node_ids(parsed: &serde_json::Value) -> Vec<&str> {
    parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect()
}

/// Each line of a DOT document with every quoted string replaced by `Q`.
/// Panics on a quote that is not closed on its line or an escape that is
/// not one DOT allows, so a statement split by a stray quote or newline
/// cannot pass.
fn dot_statement_shapes(dot: &str) -> Vec<String> {
    dot.lines()
        .map(|line| {
            let mut shape = String::new();
            let mut chars = line.trim().chars();
            while let Some(ch) = chars.next() {
                if ch != '"' {
                    shape.push(ch);
                    continue;
                }
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some('"' | '\\' | 'n') => {}
                            other => panic!("bad escape {other:?} in: {line}"),
                        },
                        Some(_) => {}
                        None => panic!("unterminated quoted string in: {line}"),
                    }
                }
                shape.push('Q');
            }
            shape
        })
        .collect()
}

fn build_graph() -> Graph {
    let mut graph = Graph::new();
    graph.add_node(node_with_fields("a", "feature", "feature A", "planned"));
    graph.add_node(testable_node("b"));
    graph.add_node(testable_node("c"));
    graph.add_edge(Edge {
        source: "a".into(),
        target: "b".into(),
        label: "behaviors".into(),
    });
    graph.add_edge(Edge {
        source: "b".into(),
        target: "c".into(),
        label: "depends_on".into(),
    });
    graph
}

// === serialize_json_graph contract ===

// B:serialize_json_graph — verify contract "requires/ensures consistency for JSON graph serialization"
#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "Serialize JSON Graph: JSON graph serialization holds — validation_complete_fired, all_nodes_serialized, all_edges_serialized, schema_version_present, valid_json_produced, render_complete_emitted"
)]
fn json_graph_contract_finalized_graph_produces_valid_output() {
    // Requires: graph is finalized (built with nodes + edges)
    // Ensures: valid JSON with schema_version, all nodes, all edges, source locations
    let graph = build_graph();
    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(
        parsed["schema_version"].is_string(),
        "must include schema_version"
    );
    assert_eq!(
        parsed["nodes"].as_array().unwrap().len(),
        3,
        "all nodes present"
    );
    assert_eq!(
        parsed["edges"].as_array().unwrap().len(),
        2,
        "all edges present"
    );

    for node in parsed["nodes"].as_array().unwrap() {
        assert!(node["file"].is_string(), "must include source file");
        assert!(node["line"].is_number(), "must include source line");
    }
}

// === serialize_dot_visualization contract ===

// B:serialize_dot_visualization — verify contract "requires/ensures consistency for DOT visualization"
#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "Serialize DOT Visualization: DOT visualization holds — validation_complete_fired, valid_dot_produced, nodes_labeled, edges_labeled, render_complete_emitted"
)]
fn dot_contract_finalized_graph_produces_valid_dot() {
    // Requires: graph is finalized
    // Ensures: valid DOT; every node labeled with its ID and title; every
    // edge labeled with its type.
    let graph = build_graph();
    let dot = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());

    assert_eq!(
        dot,
        concat!(
            "digraph specforge {\n",
            "  rankdir=LR;\n",
            "  node [shape=box];\n",
            "  \"a\" [label=\"a\\nTitle a\"];\n",
            "  \"b\" [label=\"b\\nTitle b\"];\n",
            "  \"c\" [label=\"c\\nTitle c\"];\n",
            "  \"a\" -> \"b\" [label=\"behaviors\"];\n",
            "  \"b\" -> \"c\" [label=\"depends_on\"];\n",
            "}\n",
        )
    );
    // valid_dot_produced: every line is a statement of the DOT grammar.
    assert_eq!(
        dot_statement_shapes(&dot),
        vec![
            "digraph specforge {",
            "rankdir=LR;",
            "node [shape=box];",
            "Q [label=Q];",
            "Q [label=Q];",
            "Q [label=Q];",
            "Q -> Q [label=Q];",
            "Q -> Q [label=Q];",
            "}",
        ]
    );
}

// === export_agent_context_format contract ===

// B:export_agent_context_format — verify contract "requires/ensures consistency for agent context export"
#[specforge_test(
    behavior = "export_agent_context_format",
    verify = "Export Agent Context Format: agent context export holds — validation_complete_fired, token_optimized_output, schema_version_present, scope_enforced, invalid_scope_diagnosed, export_complete_emitted"
)]
fn context_contract_includes_contracts_and_verify_omits_prose() {
    // Requires: finalized graph
    let mut graph = build_graph();
    graph.add_node(described_node("d")); // disconnected from a -> b -> c
    let json = crate::export(
        &graph,
        specforge_emitter::EmitFormat::Context,
        Some(&crate::support::headline_registry(&["behavior", "feature"])),
    );
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // schema_version_present
    assert_eq!(parsed["schema_version"], "1.0.0");

    // token_optimized_output: contract, status and verify kept; the prose
    // description and source locations left out.
    let nodes = parsed["nodes"].as_array().unwrap();
    let d = nodes.iter().find(|n| n["id"] == "d").unwrap();
    assert_eq!(d["contract"], "The system MUST work");
    assert_eq!(
        d["verify"],
        serde_json::json!([{ "kind": "unit", "description": "it works" }])
    );
    let a = nodes.iter().find(|n| n["id"] == "a").unwrap();
    assert_eq!(a["status"], "planned");
    assert!(
        !json.contains("Long prose"),
        "description text must not appear: {json}"
    );
    for node in nodes {
        assert!(node.get("description").is_none(), "{node}");
        assert!(node.get("file").is_none(), "{node}");
    }

    // scope_enforced: scoped at a, only the connected a, b, c.
    let scoped = crate::support::scoped_context(&graph, "a").unwrap();
    let scoped: serde_json::Value = serde_json::from_str(&scoped).unwrap();
    assert_eq!(node_ids(&scoped), vec!["a", "b", "c"]);
    assert_eq!(scoped["edges"].as_array().unwrap().len(), 2);

    // invalid_scope_diagnosed: E003 naming the entity — through
    // the call `specforge export --format context --scope` makes.
    let err = specforge_emitter::emit(
        &graph,
        &specforge_emitter::EmitOptions {
            format: specforge_emitter::EmitFormat::Context,
            scope: Some("ghost"),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "unresolved entity 'ghost' — not found in graph"
    );
    assert_eq!(err.code(), Some(specforge_diagnostics::codes::E003));
}

// === export_agent_graph_format contract ===

// B:export_agent_graph_format — verify contract "requires/ensures consistency for agent graph export"
#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "Export Agent Graph Format: agent graph export holds — validation_complete_fired, full_fidelity_output, schema_version_present, scope_enforced, invalid_scope_diagnosed, export_complete_emitted"
)]
fn graph_format_contract_finalized_graph_produces_full_output() {
    // Requires: finalized graph
    let mut graph = build_graph();
    graph.add_node(described_node("d")); // disconnected from a -> b -> c
    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // schema_version_present
    assert_eq!(parsed["schema_version"], "1.0.0");

    // full_fidelity_output: every node with every field and its location,
    // every edge.
    assert_eq!(node_ids(&parsed), vec!["a", "b", "c", "d"]);
    let d = &parsed["nodes"][3];
    assert_eq!(d["kind"], "behavior");
    assert_eq!(d["title"], "Title d");
    assert_eq!(d["file"], "test.spec");
    assert_eq!(d["line"], 1);
    assert_eq!(
        d["fields"],
        serde_json::json!({
            "contract": "The system MUST work",
            "description": "Long prose that explains the entity at length",
        })
    );
    assert_eq!(
        d["verify"],
        serde_json::json!([{ "kind": "unit", "description": "it works" }])
    );
    assert_eq!(parsed["nodes"][0]["fields"]["status"], "planned");
    assert_eq!(
        parsed["edges"],
        serde_json::json!([
            { "source": "a", "target": "b", "label": "behaviors" },
            { "source": "b", "target": "c", "label": "depends_on" },
        ])
    );

    // scope_enforced: scoped at c, only the connected a, b, c.
    let scoped = crate::support::scoped_json(&graph, "c").unwrap();
    let scoped: serde_json::Value = serde_json::from_str(&scoped).unwrap();
    assert_eq!(node_ids(&scoped), vec!["a", "b", "c"]);
    assert_eq!(scoped["edges"].as_array().unwrap().len(), 2);

    // invalid_scope_diagnosed: E003 naming the entity — through
    // the call `specforge export --format graph --scope` makes.
    let err = specforge_emitter::emit(
        &graph,
        &specforge_emitter::EmitOptions {
            scope: Some("ghost"),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "unresolved entity 'ghost' — not found in graph"
    );
    assert_eq!(err.code(), Some(specforge_diagnostics::codes::E003));
}

// === enforce_token_budget contract ===

// B:enforce_token_budget — verify contract "requires/ensures consistency for token budget enforcement"
#[specforge_test(
    behavior = "enforce_token_budget",
    verify = "Enforce Token Budget: token budget enforcement holds — validation_complete_fired, budget_respected, truncation_metadata_produced, valid_subgraph_after_truncation, token_budget_applied_emitted"
)]
fn budget_contract_within_budget_no_truncation() {
    // Requires: graph + budget
    let graph = build_graph(); // a -> b -> c: b is the most central
    let emit = |budget: usize| -> serde_json::Value {
        serde_json::from_str(&crate::support::budgeted_json(&graph, budget)).unwrap()
    };

    // Within budget: everything, and no truncation metadata.
    let roomy = emit(100_000);
    assert_eq!(node_ids(&roomy), vec!["a", "b", "c"]);
    assert!(roomy.get("token_budget").is_none(), "{roomy}");

    // Over budget: the least central entity (a, degree 1, before c by id)
    // goes first.
    let tight = emit(180);
    let meta = &tight["token_budget"];
    // truncation_metadata_produced
    assert_eq!(meta["strategy"], "prioritize");
    assert_eq!(meta["budget_tokens"], 180);
    assert_eq!(meta["truncated_entities"], serde_json::json!(["a"]));
    // Kept entities are listed most central last; compare as sets.
    let kept = |v: &serde_json::Value| -> Vec<String> {
        let mut ids: Vec<String> = node_ids(v).iter().map(|s| s.to_string()).collect();
        ids.sort();
        ids
    };
    assert_eq!(kept(&tight), vec!["b", "c"]);
    // budget_respected
    let estimated = meta["estimated_tokens"].as_u64().unwrap() as usize;
    assert!(estimated <= 180, "estimated {estimated} over budget 180");
    // The estimate is the output's real cost: a budget of exactly that keeps
    // the same entities, one token less has to drop another.
    assert_eq!(kept(&emit(estimated)), vec!["b", "c"]);
    assert_eq!(kept(&emit(estimated - 1)), vec!["b"]);
    // valid_subgraph_after_truncation: no edge to or from a.
    assert_eq!(
        tight["edges"],
        serde_json::json!([{ "source": "b", "target": "c", "label": "depends_on" }])
    );
}

// === deterministic_output contract ===

// B:deterministic_output — verify contract "requires/ensures consistency for deterministic output"
#[specforge_test(
    behavior = "deterministic_output",
    verify = "Deterministic Output: deterministic output holds — validation_complete_fired, byte_identical_output, no_nondeterministic_values"
)]
fn deterministic_contract_same_input_identical_output() {
    // Requires: same graph input
    // Ensures: identical output across all formats
    let graph = build_graph();

    let json1 = crate::support::emit_json(&graph);
    let json2 = crate::support::emit_json(&graph);
    assert_eq!(json1, json2, "JSON must be deterministic");

    let dot1 = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());
    let dot2 = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());
    assert_eq!(dot1, dot2, "DOT must be deterministic");

    let brief1 = crate::export(&graph, specforge_emitter::EmitFormat::Brief, None);
    let brief2 = crate::export(&graph, specforge_emitter::EmitFormat::Brief, None);
    assert_eq!(brief1, brief2, "brief must be deterministic");

    let ctx1 = crate::export(&graph, specforge_emitter::EmitFormat::Context, None);
    let ctx2 = crate::export(&graph, specforge_emitter::EmitFormat::Context, None);
    assert_eq!(ctx1, ctx2, "context must be deterministic");
}

// === present_diagnostics_as_json contract ===

// B:present_diagnostics_as_json — verify contract
#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "Present Diagnostics as JSON: JSON diagnostic presentation holds — diagnostics_collected, one_shape_everywhere, location_both_ways, data_only_when_typed"
)]
fn diagnostic_json_contract_complete_fields() {
    let diags = vec![
        Diagnostic::new(specforge_common::codes::E001, "unresolved")
            .with_span(SourceSpan {
                file: "test.spec".into(),
                start_line: 5,
                start_col: 10,
                end_line: 5,
                end_col: 20,
            })
            .with_suggestion("did you mean 'foo'?"),
    ];

    let json = specforge_common::serialize_diagnostics(&diags);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_array());

    let entry = &parsed[0];
    assert_eq!(entry["code"], "E001");
    assert_eq!(entry["severity"], "Error");
    assert_eq!(entry["file"], "test.spec");
    assert_eq!(entry["line"], 5);
    assert_eq!(entry["column"], 10);
    assert_eq!(entry["suggestion"], "did you mean 'foo'?");
}

#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "diagnostics are presented as one JSON array"
)]
fn diagnostic_json_array() {
    let diags = vec![
        Diagnostic::new(specforge_common::codes::E001, "err"),
        Diagnostic::untyped("W001", Severity::Warning, "warn"),
    ];
    let json = specforge_common::serialize_diagnostics(&diags);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 2);
}

#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "each diagnostic carries code, severity, message, file, line and column"
)]
fn diagnostic_json_all_fields() {
    let diags = vec![
        Diagnostic::untyped("E042", Severity::Error, "test msg").with_span(SourceSpan {
            file: "a.spec".into(),
            start_line: 3,
            start_col: 7,
            end_line: 3,
            end_col: 15,
        }),
    ];
    let json = specforge_common::serialize_diagnostics(&diags);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let e = &parsed[0];
    assert_eq!(e["code"], "E042");
    assert_eq!(e["severity"], "Error");
    assert_eq!(e["message"], "test msg");
    assert_eq!(e["file"], "a.spec");
    assert_eq!(e["line"], 3);
    assert_eq!(e["column"], 7);
}

#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "the presented JSON is valid and parseable"
)]
fn diagnostic_json_valid_parseable() {
    let diags = vec![Diagnostic::new(
        specforge_common::codes::E001,
        "msg with \"quotes\"",
    )];
    let json = specforge_common::serialize_diagnostics(&diags);
    let result: Result<serde_json::Value, _> = serde_json::from_str(&json);
    assert!(result.is_ok(), "output must be valid JSON");
}

#[test]
fn diagnostic_exit_code_unaffected_by_format() {
    let diags = vec![Diagnostic::new(specforge_common::codes::E001, "err")];
    // Exit code should be based on severity regardless of format
    let exit = specforge_common::compute_exit_code(&diags);
    assert_eq!(exit, 1, "errors should produce exit 1 regardless of format");
    // Also verify JSON is still produced
    let json = specforge_common::serialize_diagnostics(&diags);
    assert!(!json.is_empty());
}

#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "suggestion is included when available"
)]
fn diagnostic_suggestion_included() {
    let diags = vec![
        Diagnostic::new(specforge_common::codes::E001, "unresolved")
            .with_suggestion("did you mean 'bar'?"),
    ];
    let json = specforge_common::serialize_diagnostics(&diags);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed[0]["suggestion"], "did you mean 'bar'?");
}

// ============================================================
// serialize_json_graph — remaining verify statements
// ============================================================

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "JSON output contains all nodes"
)]
fn json_graph_all_nodes() {
    let graph = build_graph();
    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 3);
}

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "JSON output contains all edges"
)]
fn json_graph_all_edges() {
    let graph = build_graph();
    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["edges"].as_array().unwrap().len(), 2);
}

#[specforge_test(behavior = "serialize_json_graph", verify = "output is valid JSON")]
fn json_graph_valid_json() {
    let graph = build_graph();
    let json = crate::support::emit_json(&graph);
    let result: Result<serde_json::Value, _> = serde_json::from_str(&json);
    assert!(result.is_ok(), "output must be valid JSON");
}

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "output includes schema_version field"
)]
fn json_graph_schema_version() {
    let graph = build_graph();
    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["schema_version"].is_string());
}

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "empty graph produces valid JSON with empty nodes and edges arrays"
)]
fn json_graph_empty() {
    let graph = Graph::new();
    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["edges"].as_array().unwrap().len(), 0);
}

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "schema is included even for empty graph"
)]
fn json_graph_empty_has_schema() {
    let graph = Graph::new();
    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed["schema_version"].is_string(),
        "empty graph must still have schema_version"
    );
}

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "structural-only graph (zero extensions) produces valid Graph Protocol JSON with raw keywords in kind field"
)]
fn json_graph_structural_only() {
    let mut graph = Graph::new();
    graph.add_node(node_with_fields("x", "custom_kind", "c", "active"));
    let json = crate::support::emit_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let node = &parsed["nodes"].as_array().unwrap()[0];
    assert_eq!(
        node["kind"], "custom_kind",
        "raw keyword preserved in kind field"
    );
}

// ============================================================
// serialize_dot_visualization — remaining verify statements
// ============================================================

#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "DOT output is valid Graphviz syntax"
)]
fn dot_valid_syntax() {
    // IDs and titles with quotes, backslashes and newlines must stay inside
    // their quoted strings.
    let mut graph = build_graph();
    let mut hostile = testable_node("say \"hi\"");
    hostile.title = Some("back\\slash }\n\"; digraph x {".to_string());
    graph.add_node(hostile);
    graph.add_edge(Edge {
        source: "a".into(),
        target: "say \"hi\"".into(),
        label: "behaviors".into(),
    });
    let dot = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());

    assert_eq!(
        dot_statement_shapes(&dot),
        vec![
            "digraph specforge {",
            "rankdir=LR;",
            "node [shape=box];",
            "Q [label=Q];",
            "Q [label=Q];",
            "Q [label=Q];",
            "Q [label=Q];",
            "Q -> Q [label=Q];",
            "Q -> Q [label=Q];",
            "Q -> Q [label=Q];",
            "}",
        ],
        "{dot}"
    );
    assert!(
        dot.contains(
            "  \"say \\\"hi\\\"\" [label=\"say \\\"hi\\\"\\nback\\\\slash }\\n\\\"; digraph x {\"];\n"
        ),
        "{dot}"
    );
    assert!(
        dot.contains("  \"a\" -> \"say \\\"hi\\\"\" [label=\"behaviors\"];\n"),
        "{dot}"
    );
}

#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "nodes are labeled with IDs"
)]
fn dot_nodes_labeled() {
    let mut graph = build_graph();
    // An untitled node with no edges: labeled with its ID alone.
    let mut bare = testable_node("d");
    bare.title = None;
    graph.add_node(bare);
    let dot = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());

    // Each node has its own statement, labeled with its ID (and title).
    let statements: Vec<&str> = dot
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('"') && !l.contains("->"))
        .collect();
    assert_eq!(
        statements,
        vec![
            "\"a\" [label=\"a\\nTitle a\"];",
            "\"b\" [label=\"b\\nTitle b\"];",
            "\"c\" [label=\"c\\nTitle c\"];",
            "\"d\" [label=\"d\"];",
        ]
    );
}

#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "edges are labeled with types"
)]
fn dot_edges_labeled() {
    let graph = build_graph();
    let dot = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());
    assert!(
        dot.contains("behaviors"),
        "edge label 'behaviors' must be present"
    );
    assert!(
        dot.contains("depends_on"),
        "edge label 'depends_on' must be present"
    );
}

#[test]
fn dot_node_shapes() {
    let graph = build_graph();
    let dot = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());
    assert!(dot.contains("shape="), "nodes must have shape attribute");
}

// C13-00: registry-declared dot_shape/dot_color/dot_fillcolor must reach the
// emitted DOT; kinds without declarations keep the default.
#[specforge_test(
    behavior = "serialize_dot_visualization",
    verify = "node shapes use extension-defined dot_shape"
)]
fn dot_emits_registry_declared_styles() {
    use specforge_registry::{KindRegistry, KindRegistryEntry};

    let graph = build_graph();

    let mut registry = KindRegistry::new();
    registry.register(KindRegistryEntry {
        kind_name: "feature".to_string(),
        source_extension: "@test/x".to_string(),
        testable: false,
        supports_verify: false,
        allowed_verify_kinds: Vec::new(),
        lifecycle_field: None,
        declared: specforge_registry::EntityKindDescriptor {
            dot_shape: Some("hexagon".to_string()),
            dot_color: Some("firebrick".to_string()),
            dot_fillcolor: Some("#ffeeee".to_string()),
            ..Default::default()
        },
    });

    let dot = specforge_emitter::dot::emit_dot(
        &graph,
        &specforge_emitter::DotOptions {
            kind_registry: Some(&registry),
            ..Default::default()
        },
    );
    assert!(
        dot.contains("shape=\"hexagon\""),
        "registry shape emitted: {dot}"
    );
    assert!(
        dot.contains("color=\"firebrick\""),
        "registry color emitted"
    );
    assert!(
        dot.contains("fillcolor=\"#ffeeee\""),
        "registry fillcolor emitted"
    );

    // No registry: default emission (no per-node shape overrides).
    let plain = specforge_emitter::dot::emit_dot(&graph, &specforge_emitter::DotOptions::default());
    assert!(
        !plain.contains("shape=\"hexagon\""),
        "default emission must not invent registry styles"
    );
}

// C9-04: machine formats serialize compact — no pretty-print whitespace.
#[test]
fn machine_formats_serialize_compact() {
    let graph = build_graph();
    let options = specforge_emitter::EmitOptions {
        format: specforge_emitter::EmitFormat::Json,
        ..Default::default()
    };
    let json = specforge_emitter::emit(&graph, &options).unwrap();
    assert!(
        !json.contains("\n  \""),
        "machine JSON must not contain pretty-print indentation"
    );
    let pretty_bytes = serde_json::to_string_pretty(
        &serde_json::from_slice::<serde_json::Value>(json.as_bytes()).unwrap(),
    )
    .unwrap()
    .len();
    assert!(
        json.len() < pretty_bytes,
        "compact output must be smaller than pretty output"
    );

    let ctx = specforge_emitter::EmitOptions {
        format: specforge_emitter::EmitFormat::Context,
        ..Default::default()
    };
    let context = specforge_emitter::emit(&graph, &ctx).unwrap();
    assert!(!context.contains("\n  \""), "context must be compact too");

    let brief = specforge_emitter::EmitOptions {
        format: specforge_emitter::EmitFormat::Brief,
        ..Default::default()
    };
    let brief_out = specforge_emitter::emit(&graph, &brief).unwrap();
    assert!(!brief_out.contains("\n  \""), "brief must be compact too");
}

// C1-10: token budget applies to the agent formats (context/brief), not just
// schemaless JSON, and they are fitted as the graph export is: within the
// budget, with a `token_budget` block naming what was dropped.
#[test]
fn budget_truncates_context_and_brief() {
    let graph = build_graph(); // 3 nodes, 2 edges
    for format in [
        specforge_emitter::EmitFormat::Json,
        specforge_emitter::EmitFormat::Context,
        specforge_emitter::EmitFormat::Brief,
    ] {
        let full = specforge_emitter::emit(
            &graph,
            &specforge_emitter::EmitOptions {
                format,
                ..Default::default()
            },
        )
        .unwrap();
        let budget = specforge_emitter::estimate_tokens(&full) / 2;
        let truncated = specforge_emitter::emit(
            &graph,
            &specforge_emitter::EmitOptions {
                format,
                token_budget: Some(budget),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            specforge_emitter::estimate_tokens(&truncated) <= budget,
            "{format:?}: over the budget of {budget}: {truncated}"
        );
        assert!(
            truncated.len() < full.len(),
            "{format:?}: budgeted output must be strictly smaller ({} vs {})",
            truncated.len(),
            full.len()
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&truncated).expect("budgeted JSON still parses");
        assert_eq!(
            parsed["token_budget"]["strategy"], "prioritize",
            "{format:?}"
        );
        assert_eq!(
            parsed["token_budget"]["budget_tokens"], budget,
            "{format:?}"
        );
        assert!(
            !parsed["token_budget"]["truncated_entities"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{format:?}: {truncated}"
        );
    }

    // No budget: unchanged full output.
    let full = specforge_emitter::emit(
        &graph,
        &specforge_emitter::EmitOptions {
            format: specforge_emitter::EmitFormat::Brief,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(full.contains("\"edges\":"), "full brief keeps edges");
}
