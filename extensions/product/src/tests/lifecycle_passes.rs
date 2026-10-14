use crate::evidence::FeatureEvidence;
use crate::lifecycle::{pass_delivery_evidence, pass_lifecycle};
use specforge_extension_sdk::prelude::{
    EntityEvidence, PassEdge, PassEntity, PassInput, TestResult, TestResults, TestStatus,
};

fn entity(id: &str, kind: &str, fields: &[(&str, &str)]) -> PassEntity {
    PassEntity {
        id: id.into(),
        kind: kind.into(),
        fields: fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..PassEntity::default()
    }
}

fn behavior(id: &str, verifies: &[&str]) -> PassEntity {
    PassEntity {
        testable: true,
        verify_kinds: verifies.iter().map(|_| "unit".to_string()).collect(),
        verify_texts: verifies.iter().map(|v| v.to_string()).collect(),
        ..entity(id, "behavior", &[])
    }
}

fn edge(source: &str, target: &str, label: &str) -> PassEdge {
    PassEdge {
        source: source.into(),
        target: target.into(),
        label: label.into(),
    }
}

fn codes(input: &PassInput) -> Vec<(String, String)> {
    pass_lifecycle(input)
        .into_iter()
        .map(|d| (d.code, d.entity.unwrap_or_default()))
        .collect()
}

#[test]
fn w154_names_each_unfinished_feature_of_a_completed_milestone() {
    let input = PassInput {
        entities: vec![
            entity("m_done", "milestone", &[("status", "completed")]),
            entity("m_open", "milestone", &[("status", "in_progress")]),
            entity("f_done", "feature", &[("status", "done")]),
            entity("f_gone", "feature", &[("status", "deprecated")]),
            entity("f_open", "feature", &[("status", "in_progress")]),
            entity("f_bare", "feature", &[]),
        ],
        edges: vec![
            edge("m_done", "f_done", "features"),
            edge("m_done", "f_gone", "features"),
            edge("m_done", "f_open", "features"),
            edge("m_done", "f_bare", "features"),
            edge("m_open", "f_open", "features"),
        ],
        ..PassInput::default()
    };
    let found = pass_lifecycle(&input);
    let w154: Vec<&str> = found
        .iter()
        .filter(|d| d.code == "W154")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        w154,
        [
            "milestone 'm_done' is completed but its feature 'f_open' is in_progress",
            "milestone 'm_done' is completed but its feature 'f_bare' is proposed",
        ]
    );
}

#[test]
fn i063_i064_i065_report_status_contradictions_across_entities() {
    let input = PassInput {
        entities: vec![
            entity("f_done", "feature", &[("status", "done")]),
            entity("f_dep", "feature", &[("status", "accepted")]),
            entity("f_ok", "feature", &[("status", "done")]),
            entity("m_early", "milestone", &[("target_date", "2026-01-01")]),
            entity("m_late", "milestone", &[("target_date", "2026-03-01")]),
            entity("m_undated", "milestone", &[]),
            entity("d_shipped", "deliverable", &[("status", "shipped")]),
            entity("d_draft", "deliverable", &[]),
            entity("m_completed", "milestone", &[("status", "completed")]),
        ],
        edges: vec![
            edge("f_done", "f_dep", "depends_on"),
            edge("f_done", "f_ok", "depends_on"),
            edge("m_early", "m_late", "depends_on"),
            edge("m_late", "m_early", "depends_on"),
            edge("m_early", "m_undated", "depends_on"),
            edge("d_shipped", "m_late", "milestones"),
            edge("d_shipped", "m_completed", "milestones"),
            edge("d_draft", "m_late", "milestones"),
        ],
        ..PassInput::default()
    };
    assert_eq!(
        codes(&input),
        [
            ("I063".to_string(), "f_done".to_string()),
            ("I064".to_string(), "m_early".to_string()),
            ("I065".to_string(), "d_shipped".to_string()),
        ]
    );
}

fn evidence_input(test_results: Option<TestResults>) -> PassInput {
    PassInput {
        entities: vec![
            entity("f_proven", "feature", &[("status", "done")]),
            entity("f_half", "feature", &[("status", "done")]),
            entity("f_alone", "feature", &[("status", "done")]),
            entity("f_ahead", "feature", &[("status", "in_progress")]),
            behavior("b1", &["a"]),
            behavior("b2", &["a", "b"]),
            behavior("b3", &["a"]),
        ],
        edges: vec![
            edge("b1", "f_proven", "features"),
            edge("b1", "f_half", "features"),
            edge("b2", "f_half", "features"),
            edge("b3", "f_ahead", "features"),
            // A journey naming a feature does not implement it.
            edge("j1", "f_alone", "features"),
        ],
        test_results,
        ..PassInput::default()
    }
}

fn passing(results: &[(&str, &str)]) -> TestResults {
    let mut recorded = TestResults {
        runner: Some("fixture".into()),
        ..TestResults::default()
    };
    for (id, verify) in results {
        recorded
            .results
            .entry(id.to_string())
            .or_default()
            .tests
            .push(TestResult {
                name: format!("{id}_{verify}"),
                status: TestStatus::Passed,
                verify: Some(verify.to_string()),
                duration_ms: None,
            });
    }
    recorded
}

#[test]
fn i071_names_done_features_the_recorded_tests_do_not_prove() {
    let input = evidence_input(Some(passing(&[("b1", "a"), ("b2", "a"), ("b3", "a")])));
    let out = pass_delivery_evidence(&input);
    let found: Vec<(&str, &str)> = out
        .diagnostics
        .iter()
        .map(|d| (d.entity.as_deref().unwrap(), d.message.as_str()))
        .collect();
    assert_eq!(
            found,
            [
                (
                    "f_alone",
                    "feature 'f_alone' is done but no behavior implements it, so no recorded test can prove it"
                ),
                (
                    "f_half",
                    "feature 'f_half' is done but the recorded tests prove 1 of the 2 behaviors implementing it (2/3 obligations)"
                ),
            ]
        );
    assert!(out.diagnostics.iter().all(|d| d.code == "I071"));
    assert_eq!(out.summary["done"], 3);
    assert_eq!(out.summary["proven"], 2);
    assert_eq!(out.summary["done_unproven"], 2);
    assert_eq!(out.summary["proven_not_done"], 1);
}

#[test]
fn without_recorded_tests_delivery_evidence_reports_nothing() {
    let out = pass_delivery_evidence(&evidence_input(None));
    assert!(out.diagnostics.is_empty());
    assert_eq!(out.summary["evidence"], "none");
}

#[test]
fn a_feature_is_proven_when_every_implementer_is() {
    let scores = |id: &str| match id {
        "ok" => Some(EntityEvidence {
            obligations: 2,
            proven: 2,
            failing: 0,
        }),
        "failing" => Some(EntityEvidence {
            obligations: 1,
            proven: 1,
            failing: 1,
        }),
        _ => None,
    };
    let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(FeatureEvidence::of("f", &ids(&["ok"]), scores).proven);
    assert!(!FeatureEvidence::of("f", &ids(&["ok", "failing"]), scores).proven);
    assert!(!FeatureEvidence::of("f", &ids(&["ok", "unscored"]), scores).proven);
    assert!(
        !FeatureEvidence::of("f", &[], scores).proven,
        "no implementer, no proof"
    );
}
