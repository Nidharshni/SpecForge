use super::*;
use serde_json::{Value, json};
use specforge_test_macros::test as specforge_test;

/// A case's input, in the `coverage` pass's input shape (the SDK's
/// `PassInput`), so the extension's native tests read the same file.
#[derive(Deserialize)]
struct PassInput {
    entities: Vec<PassEntity>,
    #[serde(default)]
    test_results: Option<PassTestResults>,
    #[serde(default)]
    proved_claims: Option<BTreeSet<String>>,
}

#[derive(Deserialize)]
struct PassEntity {
    id: String,
    kind: String,
    #[serde(default)]
    fields: BTreeMap<String, String>,
    #[serde(default)]
    incoming_edge_count: usize,
    #[serde(default)]
    testable: bool,
    #[serde(default)]
    owes_none: bool,
    #[serde(default)]
    verify_kinds: Vec<String>,
    #[serde(default)]
    verify_texts: Vec<String>,
}

#[derive(Deserialize)]
struct PassTestResults {
    #[serde(default)]
    runner: Option<String>,
    #[serde(default)]
    results: BTreeMap<String, PassEntityResults>,
}

#[derive(Deserialize)]
struct PassEntityResults {
    #[serde(default)]
    tests: Vec<RecordedTest>,
}

/// `@specforge/testing`'s grading: invariants, an error at high risk.
fn testing_grading() -> RiskGrading {
    RiskGrading {
        kind: "invariant".to_string(),
        error_at: "high".to_string(),
    }
}

fn assess_input(input: Value) -> (Vec<Entity>, Assessment) {
    assess_graded(input, Some(&testing_grading()))
}

/// As the testing pass does: the risk field is read for the graded kind only.
fn assess_graded(input: Value, grading: Option<&RiskGrading>) -> (Vec<Entity>, Assessment) {
    let input: PassInput = serde_json::from_value(input).unwrap();
    let entities: Vec<Entity> = input
        .entities
        .into_iter()
        .map(|e| Entity {
            risk: grading
                .filter(|g| g.kind == e.kind)
                .and_then(|_| e.fields.get("risk").cloned()),
            referenced: e.incoming_edge_count > 0,
            id: e.id,
            kind: e.kind,
            testable: e.testable,
            owes_none: e.owes_none,
            verify_kinds: e.verify_kinds,
            verify_texts: e.verify_texts,
        })
        .collect();
    let results = input.test_results.map(|r| TestResults {
        runner: r.runner,
        entities: r.results.into_iter().map(|(id, e)| (id, e.tests)).collect(),
    });
    let assessment = assess(
        &entities,
        results.as_ref(),
        input.proved_claims.as_ref(),
        grading,
    );
    (entities, assessment)
}

fn codes(assessment: &Assessment) -> Vec<&str> {
    assessment.findings.iter().map(|f| f.code).collect()
}

fn create_user(tests: Value) -> Value {
    json!({
        "entities": [{
            "id": "create_user", "kind": "behavior", "testable": true,
            "verify_kinds": ["unit", "unit"],
            "verify_texts": ["rejects a duplicate email", "stores a hashed password"]
        }],
        "test_results": {"runner": "cargo-test", "results": {"create_user": {"tests": tests}}}
    })
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "recorded test results prove entities and failing tests are A014"
)]
fn an_obligation_is_proven_by_a_passing_test_that_names_it() {
    let (_, out) = assess_input(create_user(json!([
        {"name": "dup", "status": "pass", "verify": "rejects a duplicate email"},
        {"name": "hash", "status": "pass", "verify": "stores a hashed password"}
    ])));
    assert!(out.findings.is_empty(), "{:?}", codes(&out));
    assert_eq!(out.summary.discharge_funnel.entities_proven, 1);
    assert_eq!(out.summary.test_results.unwrap().obligations_proven, 2);
    assert!(out.verdicts["create_user"].is_proven());
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "recorded test results prove entities and failing tests are A014"
)]
fn unnamed_and_failing_tests_leave_obligations_unproven() {
    let (_, out) = assess_input(create_user(json!([
        {"name": "any", "status": "pass"},
        {"name": "hash", "status": "fail", "verify": "stores a hashed password"}
    ])));
    assert_eq!(codes(&out), vec!["A015", "A014"]);
    assert!(out.findings[0].message.contains(
        "2 obligation(s) no passing test proves: \"rejects a duplicate email\", \"stores a hashed password\""
    ));
    assert_eq!(out.findings[1].severity, Severity::Error);
    assert_eq!(out.summary.discharge_funnel.entities_proven, 0);
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "an obligation no passing test names is A015 and a test naming an undeclared obligation is A016"
)]
fn a_test_naming_an_undeclared_obligation_is_reported() {
    let (_, out) = assess_input(create_user(json!([
        {"name": "dup", "status": "pass", "verify": "rejects a duplicate email"},
        {"name": "hash", "status": "pass", "verify": "stores a hashed pasword"}
    ])));
    assert_eq!(codes(&out), vec!["A015", "A016"]);
    assert!(
        out.findings[1]
            .message
            .contains("\"stores a hashed pasword\"")
    );
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "a proved formal claim discharges verify property obligations"
)]
fn an_entailed_formal_claim_discharges_property_obligations() {
    let (_, out) = assess_input(json!({
        "entities": [{
            "id": "unique_ids", "kind": "invariant", "testable": true, "incoming_edge_count": 1,
            "verify_kinds": ["property", "unit"],
            "verify_texts": ["ids never collide", "a second insert fails"]
        }],
        "test_results": {"results": {}},
        "proved_claims": ["unique_ids"]
    }));
    assert_eq!(codes(&out), vec!["A015"]);
    assert!(out.findings[0].message.contains("1 obligation(s)"));
    assert!(
        out.findings[0]
            .message
            .contains("\"a second insert fails\"")
    );
    assert_eq!(out.summary.discharge_funnel.formally_discharged, 1);
}

#[test]
fn without_recorded_results_obligations_are_not_scored() {
    let mut input = create_user(json!([]));
    input.as_object_mut().unwrap().remove("test_results");
    let (_, out) = assess_input(input);
    assert!(out.findings.is_empty(), "{:?}", codes(&out));
    assert!(out.summary.test_results.is_none());
}

#[test]
fn the_gate_percentage_is_proven_over_testable_and_vacuous_without_testables() {
    let mut summary = Summary::default();
    assert_eq!(summary.proof_pct(), 100.0);
    summary.testable_total = 4;
    summary.testable_proven = 1;
    assert_eq!(summary.proof_pct(), 25.0);
}

#[specforge_test(
    behavior = "te_coverage_gate",
    verify = "a proven entity whose kind is not testable does not raise the gate"
)]
fn only_testable_entities_count_toward_the_gate() {
    let (_, out) = assess_input(json!({
        "entities": [
            {"id": "login", "kind": "behavior", "testable": true,
             "verify_kinds": ["unit"], "verify_texts": ["a known user logs in"]},
            {"id": "logout", "kind": "behavior", "testable": true,
             "verify_kinds": ["unit"], "verify_texts": ["the session ends"]},
            {"id": "no_lost_login", "kind": "property", "testable": false,
             "verify_kinds": ["unit"], "verify_texts": ["no login is lost"]}
        ],
        "test_results": {"results": {
            "login": {"tests": [{"name": "t", "status": "pass", "verify": "a known user logs in"}]},
            "no_lost_login": {"tests": [{"name": "u", "status": "pass", "verify": "no login is lost"}]}
        }}
    }));
    // Both proven entities are in the funnel; only the testable one is
    // in the gate's numerator.
    assert_eq!(out.summary.discharge_funnel.entities_proven, 2);
    assert_eq!(out.summary.testable_proven, 1);
    assert_eq!(out.summary.testable_total, 2);
    assert_eq!(out.summary.proof_pct(), 50.0);
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "an entity with no obligations is never proven, even by passing tests"
)]
fn passing_tests_do_not_prove_an_entity_with_no_obligations() {
    let (_, out) = assess_input(json!({
        "entities": [{"id": "logout", "kind": "behavior", "testable": true}],
        "test_results": {"results": {
            "logout": {"tests": [{"name": "logout works", "status": "pass"}]}
        }}
    }));
    let verdict = &out.verdicts["logout"];
    assert!(!verdict.is_proven(), "{verdict:?}");
    assert_eq!(verdict.status(), Status::Uncovered);
    assert_eq!(out.summary.discharge_funnel.entities_proven, 0);
    assert_eq!(out.summary.testable_proven, 0);
    assert_eq!(codes(&out), vec!["A001"]);
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "an entity whose obligations are all formally discharged is proven without tests"
)]
fn a_formally_discharged_entity_is_proven_without_tests() {
    let (_, out) = assess_input(json!({
        "entities": [{
            "id": "unique_ids", "kind": "invariant", "testable": true, "incoming_edge_count": 1,
            "verify_kinds": ["property"], "verify_texts": ["ids never collide"]
        }],
        "test_results": {"results": {}},
        "proved_claims": ["unique_ids"]
    }));
    let verdict = &out.verdicts["unique_ids"];
    assert_eq!(verdict.tests, 0);
    assert!(verdict.is_proven(), "{verdict:?}");
    assert_eq!(verdict.status(), Status::Covered);
    assert_eq!(out.summary.discharge_funnel.entities_proven, 1);
    assert_eq!(out.summary.testable_proven, 1);
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "entities W004 exempts are not A001 and leave the testable count"
)]
fn exempt_entities_leave_a001_and_the_testable_count() {
    let (_, out) = assess_input(json!({
        "entities": [
            // A union type and an abstract behavior: exempt, nothing declared.
            {"id": "Status", "kind": "type", "testable": true, "owes_none": true},
            {"id": "base_login", "kind": "behavior", "testable": true, "owes_none": true},
            // A governance kind no rule requires obligations of.
            {"id": "lost_task", "kind": "failure_mode", "testable": true, "owes_none": true},
            // ... unless it declares some: then it counts like any other.
            {"id": "latency", "kind": "constraint", "testable": true, "owes_none": true,
             "verify_kinds": ["load"], "verify_texts": ["p99 under 50ms"]},
            {"id": "logout", "kind": "behavior", "testable": true}
        ]
    }));
    assert_eq!(out.summary.testable_exempt, 3);
    assert_eq!(out.summary.testable_total, 2);
    assert_eq!(out.summary.testable_verified, 1);
    let a001: Vec<&str> = out
        .findings
        .iter()
        .filter(|f| f.code == "A001")
        .map(|f| f.message.as_str())
        .collect();
    assert_eq!(a001, ["behavior 'logout' declares no verify obligations"]);
}

/// `tests/fixtures/coverage-cases.json`: the rule's golden vectors. The
/// testing extension's native tests hold its Wasm pass to the same file.
#[test]
fn the_rule_matches_the_shared_golden_vectors() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../tests/fixtures/coverage-cases.json")).unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let expect = &case["expect"];
        let (entities, out) = assess_input(case["input"].clone());

        let summary = serde_json::to_value(&out.summary).unwrap();
        assert_eq!(summary, expect["summary"], "summary of {name:?}");
        let back: Summary = serde_json::from_value(summary).unwrap();
        assert_eq!(back, out.summary, "summary of {name:?} round-trips");

        let findings: Vec<Value> = out
            .findings
            .iter()
            .map(|f| {
                json!({
                    "code": f.code,
                    "entity": entities[f.entity].id,
                    "severity": f.severity,
                    "message": f.message,
                    "suggestion": f.suggestion,
                })
            })
            .collect();
        assert_eq!(
            Value::from(findings),
            expect["findings"],
            "findings of {name:?}"
        );

        let verdicts: serde_json::Map<String, Value> = out
            .verdicts
            .iter()
            .map(|(id, v)| {
                let mut view = serde_json::to_value(v).unwrap();
                view["is_proven"] = json!(v.is_proven());
                view["status"] = serde_json::to_value(v.status()).unwrap();
                (id.clone(), view)
            })
            .collect();
        assert_eq!(
            Value::from(verdicts),
            expect["verdicts"],
            "verdicts of {name:?}"
        );
    }
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "without a risk grading no kind is risk-tallied and nothing is A002"
)]
fn without_grading_no_kind_is_risk_tallied_and_nothing_is_a002() {
    let input = json!({
        "entities": [
            {"id": "no_overdraft", "kind": "invariant", "fields": {"risk": "high"}},
            {"id": "ids_unique", "kind": "invariant", "fields": {"risk": "low"},
             "incoming_edge_count": 1}
        ]
    });
    let (_, graded) = assess_input(input.clone());
    assert_eq!(codes(&graded), ["A002", "A002"]);
    assert_eq!(graded.summary.invariants.len(), 2);

    let (entities, ungraded) = assess_graded(input, None);
    assert!(entities.iter().all(|e| e.risk.is_none()));
    assert!(ungraded.findings.is_empty(), "{:?}", ungraded.findings);
    assert!(ungraded.summary.invariants.is_empty());
    assert_eq!(ungraded.summary.invariant_enforced, 0);
    assert_eq!(ungraded.summary.invariant_unreferenced, 0);

    // A grading names any kind; the messages name it.
    let grading = RiskGrading {
        kind: "hazard".to_string(),
        error_at: "severe".to_string(),
    };
    let (_, hazards) = assess_graded(
        json!({"entities": [{"id": "fire", "kind": "hazard", "fields": {"risk": "severe"}}]}),
        Some(&grading),
    );
    assert_eq!(
        hazards.findings[0].message,
        "hazard 'fire' declares no verify obligations"
    );
    assert_eq!(hazards.findings[0].severity, Severity::Error);
    assert_eq!(
        hazards.findings[0].suggestion.as_deref(),
        Some("severe-risk hazard: add at least one `verify property` obligation")
    );
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "an entity counts toward coverage when its kind is testable and it owes or declares obligations, and is exempt when testable and neither"
)]
fn counting_is_testable_and_owing_or_declaring() {
    for testable in [false, true] {
        for owes_obligations in [false, true] {
            for declared in 0..=2 {
                let counting = Counting {
                    testable,
                    owes_obligations,
                    declared,
                };
                assert_eq!(
                    counting.counts(),
                    testable && (owes_obligations || declared > 0),
                    "{counting:?}"
                );
                assert_eq!(
                    counting.exempt(),
                    testable && !counting.counts(),
                    "{counting:?}"
                );
            }
        }
    }
    let entity = Entity {
        testable: true,
        owes_none: true,
        verify_texts: vec!["a".into(), "b".into()],
        verify_kinds: vec!["unit".into(), "unit".into()],
        ..Entity::default()
    };
    assert_eq!(
        entity.counting(),
        Counting {
            testable: true,
            owes_obligations: false,
            declared: 2,
        }
    );
    assert!(entity.counts_toward_coverage());
}

#[specforge_test(
    behavior = "te_coverage_pass",
    verify = "the rule reads a pass entity as the host decided it, with no risk unless the grading caller sets one"
)]
fn a_pass_entity_is_read_as_the_host_decided_it() {
    let mut wire = specforge_protocol_types::PassEntity {
        id: "a".into(),
        kind: "invariant".into(),
        testable: true,
        owes_none: true,
        verify_kinds: vec!["unit".into(), "property".into()],
        verify_texts: vec!["one".into(), "two".into()],
        incoming_edge_count: 0,
        ..specforge_protocol_types::PassEntity::default()
    };
    wire.fields.insert("risk".into(), "high".into());
    let entity = Entity::from(&wire);
    assert_eq!(entity.id, "a");
    assert_eq!(entity.kind, "invariant");
    assert!(entity.testable && entity.owes_none);
    assert_eq!(entity.verify_kinds, wire.verify_kinds);
    assert_eq!(entity.verify_texts, wire.verify_texts);
    assert_eq!(entity.risk, None);
    assert!(!entity.referenced);
    wire.incoming_edge_count = 2;
    assert!(Entity::from(&wire).referenced);
}
