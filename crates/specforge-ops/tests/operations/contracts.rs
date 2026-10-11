use specforge_common::{Diagnostic, Severity, SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_ops::plan::PlanGapKind;
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
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: Some(format!("Title {}", id)),
        fields,
        source_span: span(),
        methods: Vec::new(),
    }
}

fn testable_node(id: &str) -> Node {
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("contract"),
        FieldValue::String("The system MUST work".to_string()),
    );
    fields.push(
        Sym::new("verify"),
        FieldValue::VerifyList(vec![VerifyStatement {
            kind: "unit".to_string(),
            description: "it works".to_string(),
        }]),
    );
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: Some(format!("Title {}", id)),
        fields,
        source_span: span(),
        methods: Vec::new(),
    }
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

// === compute_traceability_chain contract ===

// No event bus exists in the codebase: like the other emitter contracts,
// the *_emitted clause is not observable here.
#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "Compute Traceability Chain: traceability chain computation holds — validation_complete_fired, full_chain_traversed, missing_links_flagged, trace_chain_computed_emitted"
)]
fn trace_contract_entity_in_graph_produces_chain() {
    // Requires (validation_complete_fired): a finalized graph.
    // Ensures (full_chain_traversed): the full chain, both directions,
    // every hop.
    let graph = build_graph();
    let links = |links: &[specforge_ops::trace::TraceLink]| -> Vec<(String, String, usize)> {
        links
            .iter()
            .map(|l| {
                assert_eq!(l.status, specforge_ops::trace::TraceLinkStatus::Resolved);
                (l.entity_id.clone(), l.edge_label.clone(), l.depth)
            })
            .collect()
    };
    let missing = |chain: &specforge_ops::trace::TraceChain| -> Vec<(String, String, String)> {
        chain
            .missing
            .iter()
            .map(|m| {
                assert_eq!(m.status, specforge_ops::trace::TraceLinkStatus::Missing);
                (
                    m.from.clone(),
                    m.edge_label.clone(),
                    m.expected_kind.clone(),
                )
            })
            .collect()
    };

    let trace = crate::view_support::chain_in(&graph, crate::trace_support::build(), "b").unwrap();
    assert_eq!(trace.entity_id, "b");
    assert_eq!(trace.entity_kind, "behavior");
    assert_eq!(
        links(&trace.upstream),
        vec![("a".to_string(), "behaviors".to_string(), 1)]
    );
    assert_eq!(
        links(&trace.downstream),
        vec![("c".to_string(), "depends_on".to_string(), 1)]
    );

    // Ensures (missing_links_flagged): b's feature link is declared from
    // a's side; b declares no invariant.
    assert_eq!(
        missing(&trace),
        vec![(
            "b".to_string(),
            "invariants".to_string(),
            "invariant".to_string()
        )]
    );

    // From the root the chain reaches the leaf two hops away, and the root
    // has the one edge its kind expects.
    let root = crate::view_support::chain_in(&graph, crate::trace_support::build(), "a").unwrap();
    assert!(root.upstream.is_empty());
    assert_eq!(
        links(&root.downstream),
        vec![
            ("b".to_string(), "behaviors".to_string(), 1),
            ("c".to_string(), "depends_on".to_string(), 2),
        ]
    );
    assert!(root.missing.is_empty(), "{:?}", root.missing);

    // c: no feature lists it and it declares nothing its kind expects.
    let leaf = crate::view_support::chain_in(&graph, crate::trace_support::build(), "c").unwrap();
    assert_eq!(
        missing(&leaf),
        vec![
            (
                "c".to_string(),
                "features".to_string(),
                "feature".to_string()
            ),
            (
                "c".to_string(),
                "invariants".to_string(),
                "invariant".to_string()
            ),
        ]
    );

    // The JSON carries the Graph Protocol schema_version and the gaps.
    let project = crate::view_support::Project::of_graph(graph, crate::trace_support::build());
    let outcome =
        specforge_ops::trace::trace(&project.view(), specforge_ops::trace::Target::Entity("c"))
            .unwrap();
    let json = serde_json::to_value(&outcome).unwrap();
    assert_eq!(json["schema_version"], specforge_emitter::SCHEMA_VERSION);
    assert_eq!(json["missing"][0]["status"], "missing");
    assert_eq!(json["upstream"][0]["status"], "resolved");
}

// === compute_project_statistics contract ===

// B:compute_project_statistics — verify contract "requires/ensures consistency for project statistics computation"
#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "Compute Project Statistics: project statistics computation holds — validation_complete_fired, entity_counts_produced, coverage_computed, zero_testable_safe"
)]
fn stats_contract_graph_with_diagnostics_produces_complete_stats() {
    // Requires: graph + diagnostics collected
    // Ensures: all stat fields populated correctly
    let graph = build_graph();
    let diagnostics = vec![
        Diagnostic::new(specforge_common::codes::E001, "err"),
        Diagnostic::untyped("W002", Severity::Warning, "warn"),
    ];

    // One testable behavior without a verify, and a verified feature: the
    // feature is not testable, so it must not count toward coverage.
    let mut graph = graph;
    graph.add_node(node_with_fields("d", "behavior", "unverified", "planned"));
    let mut verified_feature = testable_node("e");
    verified_feature.kind = EntityKind {
        raw: Sym::new("feature"),
    };
    graph.add_node(verified_feature);

    let stats = crate::view_support::stats_of(&graph, &["behavior"], &diagnostics);
    assert_eq!(stats.entity_count, 5);
    assert_eq!(stats.edge_count, 2);
    // entity_counts_produced: counts grouped by kind.
    assert_eq!(
        stats.entity_counts,
        [("behavior", 3), ("feature", 2)]
            .into_iter()
            .map(|(kind, count)| specforge_ops::stats::KindCount {
                kind: kind.to_string(),
                count,
            })
            .collect::<Vec<_>>()
    );
    // coverage_computed: b and c of the three behaviors are verified.
    assert_eq!(stats.testable_count, 3);
    assert!(
        (stats.declared_pct - 200.0 / 3.0).abs() < 1e-9,
        "2 of 3 testable verified: {}",
        stats.declared_pct
    );
    assert_eq!(stats.diagnostic_summary.errors, 1);
    assert_eq!(stats.diagnostic_summary.warnings, 1);

    // zero_testable_safe: no testable kinds, or a testable kind with no
    // entities, reports 0%, not NaN.
    for testable in [&[][..], &["event"][..]] {
        let none = crate::view_support::stats_of(&graph, testable, &[]);
        assert_eq!(none.testable_count, 0);
        assert_eq!(none.declared_pct, 0.0, "testable kinds {testable:?}");
    }
}

// === validate_agent_plan contract ===

// B:validate_agent_plan — verify contract "requires/ensures consistency for agent plan validation"
#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "Validate Agent Implementation Plan: agent plan validation holds — validation_complete_fired, unresolvable_ids_diagnosed, missing_entries_warned, ordering_validated, structured_report_produced, plan_validated_emitted"
)]
fn plan_contract_validates_ids_coverage_ordering() {
    // Requires: finalized graph + plan JSON
    // Ensures: unresolvable IDs → errors, missing testable → warnings, wrong order → violations
    let graph = build_graph();
    let plan = serde_json::json!({
        "entries": [
            { "entity_id": "nonexistent", "action": "implement" },
            { "entity_id": "b", "action": "implement" },
        ]
    });

    let result = crate::view_support::plan_check(&graph, &["behavior"], &plan);
    assert!(
        !crate::view_support::gap_texts(&result, PlanGapKind::UnresolvedEntity).is_empty(),
        "unresolvable IDs must produce errors"
    );
    assert!(
        !crate::view_support::gap_texts(&result, PlanGapKind::MissingPlanEntry).is_empty(),
        "missing testable must produce warnings"
    );
    let report = serde_json::to_value(&result.gaps).unwrap();
    assert!(report.is_array(), "structured report required");
}

// === serialize_traceability_data contract ===

// No event bus exists in the codebase: like the other emitter contracts,
// the *_emitted clause is not observable here.
#[specforge_test(
    behavior = "serialize_traceability_data",
    verify = "Serialize Traceability Data: traceability data serialization holds — validation_complete_fired, full_trace_serialized, gaps_included, graph_protocol_conformance, render_complete_emitted"
)]
fn trace_data_contract_all_entities_traced() {
    // Requires (validation_complete_fired): a finalized graph.
    let graph = build_graph();
    let traces = crate::view_support::every_chain(&graph, crate::trace_support::build());
    let json = serde_json::to_string_pretty(&traces).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // graph_protocol_conformance: the protocol's schema_version, and a
    // traces array.
    assert_eq!(parsed["schema_version"], specforge_emitter::SCHEMA_VERSION);
    let traces = parsed["traces"].as_array().unwrap();

    // full_trace_serialized: one chain per entity, in ID order, each with
    // its links from root to leaf.
    let ids: Vec<&str> = traces
        .iter()
        .map(|t| t["entity_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["a", "b", "c"]);
    let hops = |trace: &serde_json::Value, direction: &str| -> Vec<(String, u64, String)> {
        trace[direction]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                (
                    l["entity_id"].as_str().unwrap().to_string(),
                    l["depth"].as_u64().unwrap(),
                    l["status"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    assert_eq!(
        hops(&traces[0], "downstream"),
        vec![
            ("b".to_string(), 1, "resolved".to_string()),
            ("c".to_string(), 2, "resolved".to_string()),
        ]
    );
    assert_eq!(
        hops(&traces[2], "upstream"),
        vec![
            ("b".to_string(), 1, "resolved".to_string()),
            ("a".to_string(), 2, "resolved".to_string()),
        ]
    );

    // gaps_included: every missing link, with a missing status, from the
    // entity that lacks it to the kind it should reach.
    let gaps: Vec<(String, String, String, String)> = traces
        .iter()
        .flat_map(|t| t["missing"].as_array().unwrap().iter())
        .map(|m| {
            (
                m["from"].as_str().unwrap().to_string(),
                m["edge_label"].as_str().unwrap().to_string(),
                m["expected_kind"].as_str().unwrap().to_string(),
                m["status"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let gap = |from: &str, label: &str, kind: &str| {
        (
            from.to_string(),
            label.to_string(),
            kind.to_string(),
            "missing".to_string(),
        )
    };
    assert_eq!(
        gaps,
        vec![
            gap("b", "invariants", "invariant"),
            gap("c", "features", "feature"),
            gap("c", "invariants", "invariant"),
        ]
    );
}

#[specforge_test(
    behavior = "serialize_traceability_data",
    verify = "full trace covers all root entities across registered edge types"
)]
fn trace_data_full_trace_covers_all_roots() {
    let graph = build_graph();
    let traces = crate::view_support::every_chain(&graph, Default::default()).chains;
    let ids: Vec<&str> = traces.iter().map(|t| t.entity_id.as_str()).collect();
    assert!(ids.contains(&"a"), "root entity a must be traced");
    assert!(ids.contains(&"b"), "mid entity b must be traced");
    assert!(ids.contains(&"c"), "leaf entity c must be traced");
}

#[specforge_test(
    behavior = "serialize_traceability_data",
    verify = "gaps in chain are highlighted"
)]
fn trace_data_gaps_highlighted() {
    // A behavior linked to nothing: both edges its kind expects are gaps
    // in the full trace, with the registered edge type each would be.
    let mut graph = Graph::new();
    graph.add_node(testable_node("isolated"));
    let traces = crate::view_support::every_chain(&graph, crate::trace_support::build());
    let json = serde_json::to_string_pretty(&traces).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["traces"][0]["missing"],
        serde_json::json!([
            {
                "from": "isolated",
                "from_kind": "behavior",
                "edge_label": "features",
                "edge_type": "behavior_features",
                "expected_kind": "feature",
                "depth": 1,
                "required": false,
                "status": "missing"
            },
            {
                "from": "isolated",
                "from_kind": "behavior",
                "edge_label": "invariants",
                "edge_type": "behavior_invariants",
                "expected_kind": "invariant",
                "depth": 1,
                "required": false,
                "status": "missing"
            }
        ])
    );

    // Without expectations nothing is missing.
    let bare = crate::view_support::every_chain(&graph, Default::default()).chains;
    assert!(bare[0].missing.is_empty());
}

#[specforge_test(
    behavior = "serialize_traceability_data",
    verify = "output conforms to Graph Protocol schema"
)]
fn trace_data_output_conforms_to_schema() {
    let graph = build_graph();
    let traces = crate::view_support::every_chain(&graph, Default::default());
    let json = serde_json::to_string_pretty(&traces).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["schema_version"].is_string());
    assert!(parsed["traces"].is_array());
    for trace in parsed["traces"].as_array().unwrap() {
        assert!(trace["entity_id"].is_string());
        assert!(trace["upstream"].is_array());
        assert!(trace["downstream"].is_array());
    }
}
