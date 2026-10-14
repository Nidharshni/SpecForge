use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_ops::read::plan::{AgentPlan, PlanGapKind};
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
        "plan_id": "p",
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
        "plan_id": "p",
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
        "plan_id": "p",
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
        specforge_ops::read::plan::PlanGapKind::MissingPlanEntry
    );
    assert_eq!(result.gaps[0].target, "c");
    // A plan covering both behaviors warns about nothing.
    let full = serde_json::json!({
        "plan_id": "p",
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
        "plan_id": "p",
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
        "plan_id": "p",
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
        "plan_id": "p",
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
        "plan_id": "p",
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

    assert!(!result.ok, "an unresolved entry fails the plan");
    assert!(fine.ok, "{:?}", fine.gaps);

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
        assert_eq!(
            keys,
            [
                "gap_context",
                "missing_link_type",
                "source_entity",
                "target_entity"
            ],
            "{record}"
        );
    }
    assert_eq!(
        records[0]["missing_link_type"], "unresolved_entity",
        "{report}"
    );
    assert_eq!(
        records[1]["missing_link_type"], "missing_plan_entry",
        "{report}"
    );
}

// B:validate_agent_plan — verify unit "the plan check reads no recorded test report"
#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "the plan check reads no recorded test report"
)]
fn the_plan_check_reads_no_recorded_report() {
    let project = Project::new(
        "behavior a \"A\" {\n  verify unit \"a works\"\n}\n\nbehavior b \"B\" {\n  verify unit \"b works\"\n}\n",
        registries(&["behavior"], &[]),
    );
    let plan = AgentPlan::read(
        &serde_json::json!({"plan_id": "p", "entries": [{"entity_id": "a", "action": "implement"}]}),
    )
    .unwrap();
    let unreported = specforge_ops::read::plan::check(&project.view(), &plan);
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
            specforge_ops::read::plan::check(&project.view(), &plan),
            unreported,
            "{report}"
        );
    }
}

// B:validate_agent_plan — verify unit "a plan that is not an AgentPlan is refused, naming every place it departs from the type"
#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "a plan that is not an AgentPlan is refused, naming every place it departs from the type"
)]
fn an_agent_plan_is_read_against_its_schema() {
    use serde_json::json;
    let refused = |value: serde_json::Value| AgentPlan::read(&value).unwrap_err().message("plan");
    assert_eq!(
        refused(json!({"entries": [{"entity_id": "complete_task"}]})),
        "plan must be an AgentPlan: plan: missing required plan_id; plan.entries[0]: missing required action"
    );
    assert_eq!(
        refused(json!({})),
        "plan must be an AgentPlan: plan: missing required plan_id; plan: missing required entries"
    );
    assert_eq!(
        refused(json!([1, 2])),
        "plan must be an AgentPlan: plan: expected object, got array"
    );
    assert_eq!(
        refused(
            json!({"plan_id": "p", "entries": [{"entity_id": "Task", "action": "implement", "dependencies": ["x"]}], "bogus": 1})
        ),
        "plan must be an AgentPlan: plan.entries[0].dependencies: undeclared key; plan.bogus: undeclared key"
    );
    assert_eq!(
        refused(json!({"entries": [{"entity_id": "complete_task", "action": 7, "weird": true}]})),
        "plan must be an AgentPlan: plan: missing required plan_id; plan.entries[0].action: expected string, got integer; plan.entries[0].weird: undeclared key"
    );
    assert_eq!(
        refused(json!(42)),
        "plan must be an AgentPlan: plan: expected object, got integer"
    );
    assert_eq!(
        refused(json!({"plan_id": "p", "entries": [["a", "x"]]})),
        "plan must be an AgentPlan: plan.entries[0]: expected object, got array"
    );
    assert!(
        AgentPlan::parse("{not json")
            .unwrap_err()
            .message("plan")
            .starts_with("plan is not valid JSON: ")
    );
    let plan =
        AgentPlan::parse(r#"{"plan_id":"p","entries":[{"entity_id":"a","action":"x"}]}"#).unwrap();
    assert_eq!(
        plan,
        AgentPlan {
            plan_id: "p".into(),
            entries: vec![specforge_ops::read::plan::AgentPlanEntry {
                entity_id: "a".into(),
                action: "x".into()
            }],
        }
    );
}

// B:validate_agent_plan — verify unit "a plan passes when every entry names an entity and none comes before what it depends on; a missing entry is a warning"
#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "a plan passes when every entry names an entity and none comes before what it depends on; a missing entry is a warning"
)]
fn a_missing_entry_warns_and_an_unresolved_or_misordered_entry_fails() {
    let graph = build_graph();
    let check = |ids: &[&str]| {
        let plan = serde_json::json!({
            "plan_id": "p",
            "entries": ids
                .iter()
                .map(|id| serde_json::json!({"entity_id": id, "action": "implement"}))
                .collect::<Vec<_>>(),
        });
        crate::view_support::plan_check(&graph, &["behavior"], &plan)
    };
    // c is missing: a warning.
    assert!(check(&["b"]).ok);
    // An entry that names no entity fails the plan.
    assert!(!check(&["nonexistent", "b", "c"]).ok);
    // b before c, which it depends on, fails it.
    assert!(!check(&["b", "c"]).ok);
    let fine = check(&["c", "b"]);
    assert!(fine.ok && fine.gaps.is_empty(), "{:?}", fine.gaps);
}

// B:validate_agent_plan — verify unit "output is structured JSON"
#[specforge_test(behavior = "validate_agent_plan", verify = "output is structured JSON")]
fn the_plan_outcome_is_one_document() {
    let graph = build_graph();
    let plan = serde_json::json!({"plan_id": "p", "entries": [
        {"entity_id": "b", "action": "implement"},
        {"entity_id": "nonexistent", "action": "implement"},
    ]});
    let outcome = crate::view_support::plan_check(&graph, &["behavior"], &plan);
    let value = serde_json::to_value(&outcome).unwrap();
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(keys, ["affected_entities", "gaps", "ok"]);
    let schema = specforge_common::shape::build::<specforge_ops::read::plan::PlanOutcome>();
    assert_eq!(
        specforge_common::shape::violations(&schema, &value),
        Vec::<String>::new()
    );
}

// B:validate_agent_plan — verify unit "output is structured JSON"
#[specforge_test(behavior = "validate_agent_plan", verify = "output is structured JSON")]
fn the_plan_outcome_reads_as_human_text() {
    let graph = build_graph();
    let plan = serde_json::json!({"plan_id": "p", "entries": [
        {"entity_id": "b", "action": "implement"},
        {"entity_id": "ghost", "action": "implement"},
        {"entity_id": "c", "action": "implement"},
    ]});
    let outcome = crate::view_support::plan_check(&graph, &["behavior"], &plan);
    assert_eq!(
        outcome.to_human(),
        "affected: b, c\n\
         gaps (2):\n  \
         - unresolved_entity: E003: unresolved entity 'ghost' in plan — not found in graph\n  \
         - ordering: 'b' depends on 'c' (via depends_on), but 'c' appears later in the plan\n\
         plan check: failed\n"
    );
    let ok = crate::view_support::plan_check(
        &graph,
        &["behavior"],
        &serde_json::json!({"plan_id": "p", "entries": [
            {"entity_id": "c", "action": "implement"},
            {"entity_id": "b", "action": "implement"},
        ]}),
    );
    assert_eq!(
        ok.to_human(),
        "affected: c, b\ngaps: none\nplan check: passed\n"
    );
    let none = crate::view_support::plan_check(
        &graph,
        &["behavior"],
        &serde_json::json!({"plan_id": "p", "entries": []}),
    );
    assert!(none.to_human().starts_with("affected: none\ngaps (2):"));
}
