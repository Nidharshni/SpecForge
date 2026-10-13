use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_ops::plan::PlanGapKind;
use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue, VerifyStatement};
use specforge_test::prelude::*;

use crate::view_support::{Project, gap_texts, registries};

fn span() -> SourceSpan {
    SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 0,
    }
}

fn node(id: &str, kind: &str) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: Some(format!("Title {}", id)),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
    }
}

fn testable_node(id: &str) -> Node {
    let mut fields = FieldMap::new();
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

/// a(feature) -> b(behavior, testable) -> c(behavior, testable)
fn build_graph() -> Graph {
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature"));
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

// B:validate_agent_plan — verify unit "plan with all valid entity IDs passes validation"
#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "plan with all valid entity IDs passes validation"
)]
fn plan_with_all_valid_entity_ids_passes() {
    let graph = build_graph();
    let plan = serde_json::json!({
        "entries": [
            { "entity_id": "b", "action": "implement" },
            { "entity_id": "c", "action": "implement" },
        ]
    });

    let result = crate::view_support::plan_check(&graph, &["behavior"], &plan);
    let unresolved = gap_texts(&result, PlanGapKind::UnresolvedEntity);
    assert!(unresolved.is_empty(), "no errors expected: {unresolved:?}");
}

// B:validate_agent_plan — verify unit "plan referencing nonexistent entity ID produces E003"
#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "plan referencing nonexistent entity ID produces E003"
)]
fn plan_referencing_nonexistent_entity_produces_error() {
    let graph = build_graph();
    let plan = serde_json::json!({
        "entries": [
            { "entity_id": "b", "action": "implement" },
            { "entity_id": "nonexistent", "action": "implement" },
        ]
    });

    let result = crate::view_support::plan_check(&graph, &["behavior"], &plan);
    let unresolved = gap_texts(&result, PlanGapKind::UnresolvedEntity);
    assert!(
        unresolved
            .iter()
            .any(|e| e.contains("E003") && e.contains("nonexistent")),
        "should report E003 for nonexistent: {unresolved:?}"
    );
}

// B:validate_agent_plan — verify unit "testable entity missing from plan produces warning"
#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "testable entity missing from plan produces warning"
)]
fn testable_entity_missing_from_plan_produces_warning() {
    let graph = build_graph();
    // Plan only covers "b", missing "c"
    let plan = serde_json::json!({
        "entries": [
            { "entity_id": "b", "action": "implement" },
        ]
    });

    let result = crate::view_support::plan_check(&graph, &["behavior"], &plan);
    // One warning, for c; none for the covered b or the untestable a.
    assert_eq!(
        gap_texts(&result, PlanGapKind::MissingPlanEntry),
        ["testable entity 'c' (behavior) is not covered by the plan"]
    );
    assert_eq!(result.gaps.len(), 1);
    assert_eq!(
        result.gaps[0].kind,
        specforge_ops::plan::PlanGapKind::MissingPlanEntry
    );
    assert_eq!(result.gaps[0].target, "c");
    // A plan covering both behaviors warns about nothing.
    let full = serde_json::json!({
        "entries": [
            { "entity_id": "c", "action": "implement" },
            { "entity_id": "b", "action": "implement" },
        ]
    });
    let covered = crate::view_support::plan_check(&graph, &["behavior"], &full);
    assert!(covered.gaps.is_empty(), "{:?}", covered.gaps);
}

// B:validate_agent_plan — verify unit "plan dependency order contradicting graph produces diagnostic"
#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "plan dependency order contradicting graph produces diagnostic"
)]
fn plan_dependency_order_contradicting_graph_produces_diagnostic() {
    let graph = build_graph();
    // Graph has edge b -> c (b references c, so c should be implemented before b).
    // Plan lists b before c — wrong order.
    let plan = serde_json::json!({
        "entries": [
            { "entity_id": "b", "action": "implement" },
            { "entity_id": "c", "action": "implement" },
        ]
    });

    let result = crate::view_support::plan_check(&graph, &["behavior"], &plan);
    let ordering = gap_texts(&result, PlanGapKind::Ordering);
    assert!(
        ordering.iter().any(|v| v.contains("b") && v.contains("c")),
        "should flag ordering violation: {ordering:?}"
    );
}

// B:validate_agent_plan — verify contract "requires/ensures consistency for agent plan validation"
#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "Validate Agent Implementation Plan: agent plan validation holds — validation_complete_fired, unresolvable_ids_diagnosed, missing_entries_warned, ordering_validated, structured_report_produced, plan_validated_emitted"
)]
fn plan_validation_contract_consistency() {
    // Requires: graph is finalized (we pass a built graph)
    // Ensures: unresolvable IDs diagnosed, missing entries warned, ordering validated, structured report
    let graph = build_graph();
    let plan = serde_json::json!({
        "entries": [
            { "entity_id": "nonexistent", "action": "implement" },
            { "entity_id": "b", "action": "implement" },
        ]
    });

    let result = crate::view_support::plan_check(&graph, &["behavior"], &plan);

    // unresolvable_ids_diagnosed: E003 for the unknown ID.
    assert_eq!(
        gap_texts(&result, PlanGapKind::UnresolvedEntity),
        ["E003: unresolved entity 'nonexistent' in plan — not found in graph"]
    );
    // missing_entries_warned: c is testable but not in the plan.
    assert_eq!(
        gap_texts(&result, PlanGapKind::MissingPlanEntry),
        ["testable entity 'c' (behavior) is not covered by the plan"]
    );
    assert!(gap_texts(&result, PlanGapKind::Ordering).is_empty());
    assert_eq!(result.entries, ["b"]);

    // ordering_validated: b references c, so c must come first.
    let misordered = serde_json::json!({
        "entries": [
            { "entity_id": "b", "action": "implement" },
            { "entity_id": "c", "action": "implement" },
        ]
    });
    let ordered = crate::view_support::plan_check(&graph, &["behavior"], &misordered);
    assert_eq!(
        gap_texts(&ordered, PlanGapKind::Ordering),
        ["'b' depends on 'c' (via depends_on), but 'c' appears later in the plan"]
    );
    let fixed = serde_json::json!({
        "entries": [
            { "entity_id": "c", "action": "implement" },
            { "entity_id": "b", "action": "implement" },
        ]
    });
    let fine = crate::view_support::plan_check(&graph, &["behavior"], &fixed);
    assert!(
        gap_texts(&fine, PlanGapKind::Ordering).is_empty(),
        "{:?}",
        fine.gaps
    );

    // structured_report_produced: every gap is a record of its source, target,
    // kind and context.
    let report = serde_json::to_value(&result.gaps).unwrap();
    let records = report.as_array().expect("gaps serialize as an array");
    assert_eq!(records.len(), 2, "{report}");
    for record in records {
        let mut keys: Vec<&str> = record
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(keys, ["context", "kind", "source", "target"], "{record}");
    }
    assert_eq!(records[0]["kind"], "unresolved_entity", "{report}");
    assert_eq!(records[1]["kind"], "missing_plan_entry", "{report}");
}

// B:validate_agent_plan — verify unit "the plan check reads no recorded test report"
#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "the plan check reads no recorded test report"
)]
#[ignore = "fixed by T1"]
fn the_plan_check_reads_no_recorded_report() {
    let project = Project::new(
        "behavior a \"A\" {\n  verify unit \"a works\"\n}\n\nbehavior b \"B\" {\n  verify unit \"b works\"\n}\n",
        registries(&["behavior"], &[]),
    );
    let plan =
        serde_json::json!({"plan_id": "p", "entries": [{"entity_id": "a", "action": "implement"}]});
    let unreported = specforge_ops::plan::check(&project.view(), &plan).unwrap();
    assert_eq!(
        gap_texts(&unreported, PlanGapKind::MissingPlanEntry),
        ["testable entity 'b' (behavior) is not covered by the plan"]
    );
    for report in [
        "{",
        r#"{"results":{"a":{"tests":[{"name":"t","status":"pass","verify":"a works"}]}}}"#,
    ] {
        std::fs::write(project.dir.path().join("specforge-report.json"), report).unwrap();
        assert_eq!(
            specforge_ops::plan::check(&project.view(), &plan).unwrap(),
            unreported,
            "{report}"
        );
    }
}
