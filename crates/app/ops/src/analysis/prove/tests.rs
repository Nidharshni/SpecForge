use super::*;
use specforge_common::Sym;
use specforge_graph::{FieldMap, Graph, Node};
use specforge_parser::{EntityId, EntityKind, FieldValue};
use specforge_registry::{FieldRegistry, FieldRegistryEntry, KindRegistry};
use specforge_test_macros::test as specforge_test;

fn span(file: &str) -> SourceSpan {
    SourceSpan {
        file: Sym::new(file),
        start_line: 1,
        start_col: 1,
        end_line: 1,
        end_col: 2,
    }
}

fn expr_field(text: &str) -> FieldValue {
    FieldValue::Expression(vec![parse_expression(text).expect("parse claim")])
}

fn node(id: &str, kind: &str, fields: FieldMap) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: None,
        fields,
        source_span: span("spec/main.spec"),
        methods: Vec::new(),
        obligations: Vec::new(),
    }
}

fn constraint_node(id: &str, bound: &str) -> Node {
    let mut fields = FieldMap::new();
    fields.push(Sym::new("metric"), FieldValue::String(bound.to_string()));
    node(id, "constraint", fields)
}

fn claim_node(id: &str, expression: &str) -> Node {
    let mut fields = FieldMap::new();
    fields.push(Sym::new("expression"), expr_field(expression));
    node(id, "invariant", fields)
}

/// The builtins' proof roles: a constraint's metric and an axiom's
/// expression are bounds, an invariant's expression a claim.
fn roles() -> FieldRegistry {
    let mut registry = FieldRegistry::default();
    for (kind, field, role) in [
        ("constraint", "metric", ProofRole::Bound),
        ("axiom", "expression", ProofRole::Bound),
        ("invariant", "expression", ProofRole::Claim),
    ] {
        registry.register(
            FieldRegistryEntry::new(
                kind,
                "@test/roles",
                specforge_protocol_types::FieldDescriptor {
                    name: field.to_string(),
                    field_type: "string".to_string(),
                    normative: true,
                    proof_role: Some(role.as_str().to_string()),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
    }
    registry
}

fn prove(graph: &Graph) -> ProveReport {
    let kind_registry = KindRegistry::default();
    let field_registry = roles();
    let empty_proved = std::collections::HashSet::new();
    let ctx = AnalysisContext {
        graph,
        kind_registry: &kind_registry,
        field_registry: &field_registry,
        entities: &specforge_registry::EntitySnapshot::default(),
        test_results: None,
        proved_claims: Some(&empty_proved),
    };
    run(&ctx, &crate::solver::real_z3())
}

#[specforge_test(
    behavior = "prove_declared_bounds_and_claims",
    verify = "contradictory declared bounds are E046 citing every bound of the unsat core"
)]
fn contradictory_bounds_flagged_with_core() {
    let mut g = Graph::new();
    g.add_node(constraint_node("c1", "latency < 100ms"));
    g.add_node(constraint_node("c2", "latency > 500ms"));
    let report = prove(&g);
    assert!(report.summary["unsatisfiable"].as_bool().unwrap());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == "E046" && f.message.contains("latency < 100ms"))
    );
}

#[specforge_test(
    behavior = "prove_declared_bounds_and_claims",
    verify = "a claim the bounds entail reaches the extension passes as a proved claim"
)]
fn satisfiable_bounds_prove_entailed_claim() {
    let mut g = Graph::new();
    g.add_node(constraint_node("budget", "latency < 100ms"));
    g.add_node(claim_node("inv", "latency < 200ms"));
    let report = prove(&g);
    assert!(report.summary["satisfiable"].as_bool().unwrap());
    assert_eq!(report.summary["claims"].as_u64(), Some(1));
    assert_eq!(report.summary["claims_proved"].as_u64(), Some(1));
    assert_eq!(report.summary["claims_unproved"].as_u64(), Some(0));
    assert_eq!(report.proved_claim_ids, vec!["inv".to_string()]);
    assert!(report.findings.iter().all(|f| f.code != "W139"));
}

#[specforge_test(
    behavior = "prove_declared_bounds_and_claims",
    verify = "a claim the bounds do not entail is W139 with a counterexample in the declared units"
)]
fn unentailed_claim_yields_counterexample() {
    let mut g = Graph::new();
    g.add_node(constraint_node("budget", "latency < 100ms"));
    g.add_node(claim_node("inv", "latency > 500ms"));
    let report = prove(&g);
    assert_eq!(report.summary["claims_unproved"].as_u64(), Some(1));
    let w139 = report
        .findings
        .iter()
        .find(|f| f.code == "W139")
        .expect("W139 expected");
    assert!(
        w139.message.contains("counterexample") && w139.message.contains("latency ="),
        "counterexample must name a violating assignment: {}",
        w139.message
    );
    // the model must satisfy the bound and violate the claim
    assert!(
        w139.message.contains("latency >")
            || w139.message.contains("latency = 5")
            || w139.message.contains("latency = 1")
            || w139.message.contains("latency = 2")
            || w139.message.contains("latency = 3")
            || w139.message.contains("latency = 4")
            || w139.message.contains("latency = 6")
            || w139.message.contains("latency = 7")
            || w139.message.contains("latency = 8")
            || w139.message.contains("latency = 9"),
        "unexpected model: {}",
        w139.message
    );
}

#[test]
fn claim_with_no_bounds_requires_tautology() {
    let mut g = Graph::new();
    g.add_node(claim_node("inv", "latency <= latency"));
    let report = prove(&g);
    assert_eq!(report.summary["claims_proved"].as_u64(), Some(1));

    let mut g2 = Graph::new();
    g2.add_node(claim_node("inv", "latency < 100ms"));
    let report2 = prove(&g2);
    assert_eq!(report2.summary["claims_unproved"].as_u64(), Some(1));
}

#[test]
fn prose_metric_lines_are_skipped_and_counted() {
    let mut g = Graph::new();
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("metric"),
        FieldValue::String(
            "latency < 100ms\nwith up to 500 .spec files the parser stays fast".to_string(),
        ),
    );
    g.add_node(node("c1", "constraint", fields));
    let report = prove(&g);
    assert_eq!(report.summary["axioms"].as_u64(), Some(1));
    assert_eq!(report.summary["skipped_prose_lines"].as_u64(), Some(1));
    assert!(report.summary["satisfiable"].as_bool().unwrap());
}

#[test]
fn cross_unit_consistency_no_false_positive() {
    // 5s = 5000ms > 200ms: consistent, must NOT be flagged
    let mut g = Graph::new();
    g.add_node(constraint_node("lo", "timeout > 200ms"));
    g.add_node(constraint_node("hi", "timeout < 5s"));
    let report = prove(&g);
    assert!(
        report.summary["satisfiable"].as_bool().unwrap(),
        "cross-unit consistent bounds must be satisfiable"
    );
    assert!(report.findings.iter().all(|f| f.code != "E046"));
}

#[test]
fn cross_unit_contradiction_detected() {
    // 1s = 1000ms > 100ms: a real contradiction raw comparison misses
    let mut g = Graph::new();
    g.add_node(constraint_node("budget", "latency < 100ms"));
    g.add_node(constraint_node("floor", "latency > 1s"));
    let report = prove(&g);
    assert!(report.summary["unsatisfiable"].as_bool().unwrap());
    assert!(report.findings.iter().any(|f| f.code == "E046"));
}

#[test]
fn data_units_normalize_to_bytes() {
    // 2KiB = 2048B < 1MB: consistent
    let mut g = Graph::new();
    g.add_node(constraint_node("floor", "cache_size > 2KiB"));
    g.add_node(constraint_node("budget", "cache_size < 1MB"));
    let report = prove(&g);
    assert!(report.summary["satisfiable"].as_bool().unwrap());

    // 3GB = 3e9 B > 1MB: contradiction
    let mut g2 = Graph::new();
    g2.add_node(constraint_node("budget", "cache_size < 1MB"));
    g2.add_node(constraint_node("floor", "cache_size > 3GB"));
    let report2 = prove(&g2);
    assert!(report2.summary["unsatisfiable"].as_bool().unwrap());
}

#[specforge_test(
    behavior = "prove_declared_bounds_and_claims",
    verify = "a claim the bounds do not entail is W139 with a counterexample in the declared units"
)]
fn counterexample_renders_declared_units() {
    let mut g = Graph::new();
    g.add_node(constraint_node("budget", "latency < 100ms"));
    g.add_node(claim_node("inv", "latency > 500ms"));
    let report = prove(&g);
    let w139 = report
        .findings
        .iter()
        .find(|f| f.code == "W139")
        .expect("W139 expected");
    assert!(
        w139.message.contains("latency = ") && w139.message.contains("ms"),
        "counterexample must render in the declared unit: {}",
        w139.message
    );
    // the model value must lie in the consistent band (0..=100 ms),
    // proving the conversion happened (raw would be >= 500)
    let value: f64 = w139
        .message
        .split("latency = ")
        .nth(1)
        .and_then(|rest| rest.split("ms").next())
        .and_then(|v| v.trim().parse().ok())
        .expect("parse counterexample value");
    assert!(
        (0.0..=100.0).contains(&value),
        "model must be rendered in ms, got {value}"
    );
}

#[test]
fn expr_form_field_is_consumed() {
    let mut g = Graph::new();
    let mut fields = FieldMap::new();
    fields.push(Sym::new("metric"), expr_field("latency < 100ms"));
    g.add_node(node("budget", "constraint", fields));
    g.add_node(claim_node("inv", "latency < 150ms"));
    let report = prove(&g);
    assert_eq!(report.summary["claims_proved"].as_u64(), Some(1));
}

#[test]
fn a_field_without_a_proof_role_is_not_read() {
    let mut g = Graph::new();
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("metric"),
        FieldValue::String("latency < 100ms".to_string()),
    );
    // Contradicts the metric, but no extension declares it a bound.
    fields.push(
        Sym::new("threshold"),
        FieldValue::String("latency > 500ms".to_string()),
    );
    g.add_node(node("budget", "constraint", fields));
    // An `expression` on a kind no extension declares it for is no claim.
    let mut fields = FieldMap::new();
    fields.push(Sym::new("expression"), expr_field("latency > 1s"));
    g.add_node(node("login", "behavior", fields));
    let report = prove(&g);
    assert!(report.findings.iter().all(|f| f.code != "E046"));
    assert_eq!(report.summary["axioms"].as_u64(), Some(1));
    assert_eq!(report.summary["claims"].as_u64(), Some(0));
    assert_eq!(report.summary["entities_with_bounds"].as_u64(), Some(1));
}

#[test]
fn an_axiom_style_bound_joins_the_bounds() {
    let mut g = Graph::new();
    let mut fields = FieldMap::new();
    fields.push(Sym::new("expression"), expr_field("latency < 100ms"));
    g.add_node(node("fast_network", "axiom", fields));
    g.add_node(claim_node("inv", "latency < 200ms"));
    let report = prove(&g);
    assert_eq!(report.summary["entities_with_bounds"].as_u64(), Some(1));
    assert_eq!(report.summary["claims"].as_u64(), Some(1));
    assert_eq!(report.proved_claim_ids, vec!["inv".to_string()]);

    // An assumption contradicting a constraint's bound is E046.
    g.add_node(constraint_node("floor", "latency > 500ms"));
    let report = prove(&g);
    let e046 = report.findings.iter().find(|f| f.code == "E046").unwrap();
    assert!(
        e046.message.starts_with("contradictory bounds: "),
        "{}",
        e046.message
    );
    assert!(
        e046.message.contains("line 1 of 'metric'"),
        "{}",
        e046.message
    );
}

// ── solver seam: scripted adapter, no z3 binary needed ─────────────────

use crate::testing::ScriptedSolver;

fn prove_with(graph: &Graph, solver: &dyn Solver) -> ProveReport {
    let kind_registry = KindRegistry::default();
    let field_registry = roles();
    let empty_proved = std::collections::HashSet::new();
    let ctx = AnalysisContext {
        graph,
        kind_registry: &kind_registry,
        field_registry: &field_registry,
        entities: &specforge_registry::EntitySnapshot::default(),
        test_results: None,
        proved_claims: Some(&empty_proved),
    };
    run(&ctx, solver)
}

#[specforge_test(
    behavior = "prove_declared_bounds_and_claims",
    verify = "a solver that times out or fails mid-run is W098 and the checks it did not answer are skipped"
)]
fn timed_out_solver_yields_w098() {
    let mut g = Graph::new();
    g.add_node(constraint_node("c1", "latency < 100ms"));
    let report = prove_with(
        &g,
        &ScriptedSolver::answering([Err(SolveFailure::TimedOut {
            after: std::time::Duration::from_secs(30),
        })]),
    );
    let w098 = report
        .findings
        .iter()
        .find(|f| f.code == "W098")
        .expect("W098 expected");
    assert_eq!(
        w098.message,
        "the SMT solver timed out after 30s; some formal checks were skipped"
    );
    assert_eq!(
        w098.suggestion.as_deref(),
        Some(
            "simplify the declared bounds or claims, or raise `specforge analyze --prove-timeout`"
        )
    );
}

#[specforge_test(
    behavior = "prove_declared_bounds_and_claims",
    verify = "a solver that does not run is one W098, the finding doctor states, and nothing is solved"
)]
fn missing_solver_yields_w098_without_solving() {
    let mut g = Graph::new();
    g.add_node(constraint_node("c1", "latency < 100ms"));
    let solver = ScriptedSolver::not_found();
    let report = prove_with(&g, &solver);
    assert_eq!(report.findings, vec![crate::solver::Z3::not_on_path()]);
    assert!(report.summary["solver"].is_null());
    assert_eq!(report.summary["solver_available"], false);
    assert!(solver.scripts().is_empty());
}

#[specforge_test(
    behavior = "prove_declared_bounds_and_claims",
    verify = "a solver that times out or fails mid-run is W098 and the checks it did not answer are skipped"
)]
fn solver_failing_mid_run_yields_w098() {
    let mut g = Graph::new();
    g.add_node(constraint_node("c1", "latency < 100ms"));
    g.add_node(claim_node("inv", "latency < 200ms"));
    let report = prove_with(
        &g,
        &ScriptedSolver::answering([Ok("sat\n".to_string()), Err(SolveFailure::Failed)]),
    );
    assert!(report.findings.iter().any(|f| f.code == "W098"));
    assert_eq!(report.summary["solver_runtime_failure"], true);
    assert!(report.proved_claim_ids.is_empty());
}

#[specforge_test(
    behavior = "prove_declared_bounds_and_claims",
    verify = "contradictory declared bounds are E046 citing every bound of the unsat core"
)]
fn unsat_bounds_yield_e046_with_scripted_core() {
    let mut g = Graph::new();
    g.add_node(constraint_node("c1", "latency < 100ms"));
    g.add_node(constraint_node("c2", "latency > 500ms"));
    let report = prove_with(
        &g,
        &ScriptedSolver::answering([Ok("unsat\n(c0 c1)\n".to_string())]),
    );
    assert!(report.findings.iter().any(|f| f.code == "E046"
        && f.message.contains("latency < 100ms")
        && f.message.contains("latency > 500ms")));
    assert_eq!(report.summary["unsatisfiable"], true);
}
