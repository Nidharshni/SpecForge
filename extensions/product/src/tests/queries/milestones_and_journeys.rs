use crate::queries::*;
use crate::tests::G;
use specforge_extension_sdk::prelude::GraphNode;

#[test]
fn milestone_completion_counts_done_features_in_declared_order() {
    let mut g = G::default()
        .node("f2", "feature", &[("status", "done")])
        .node("f1", "feature", &[("status", "draft")])
        .edge("ms1", "f1", "features")
        .edge("ms1", "f2", "features");
    g.nodes.push(GraphNode {
        id: "ms1".into(),
        kind: "milestone".into(),
        title: None,
        fields: [
            ("status".to_string(), serde_json::json!("active")),
            ("features".to_string(), serde_json::json!(["f2", "f1"])),
        ]
        .into_iter()
        .collect(),
    });
    let mc = milestone_completion(&g.build(), "ms1").unwrap();
    assert_eq!(mc.total_features, 2);
    assert_eq!(mc.done_count, 1);
    assert_eq!(mc.done_features, ["f2"]);
    assert!((mc.completion_ratio - 0.5).abs() < 1e-9);
    assert_eq!(mc.status.as_deref(), Some("active"));
    let ids: Vec<&str> = mc.features.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["f2", "f1"]);
}

#[test]
fn milestone_completion_of_a_milestone_only() {
    let g = G::default()
        .n("f1", "feature")
        .n("ms1", "milestone")
        .build();
    assert!(milestone_completion(&g, "nonexistent").is_none());
    assert!(milestone_completion(&g, "f1").is_none());
    let empty = milestone_completion(&g, "ms1").unwrap();
    assert_eq!(empty.total_features, 0);
    assert_eq!(empty.completion_ratio, 0.0);
}

#[test]
fn journey_coverage_counts_done_features_not_module_ownership() {
    let g = G::default()
        .node("j1", "journey", &[("persona", "dev")])
        .n("f1", "feature")
        .node("f2", "feature", &[("status", "done")])
        .n("mod1", "module")
        .edge("j1", "f1", "features")
        .edge("j1", "f2", "features")
        .edge("mod1", "f1", "features")
        .build();
    let jc = journey_coverage(&g, "j1").unwrap();
    assert_eq!(jc.total_features, 2);
    assert_eq!(jc.covered_count, 1);
    assert_eq!(jc.uncovered_features, ["f1"]);
    assert_eq!(jc.persona.as_deref(), Some("dev"));
    assert!(journey_coverage(&g, "f1").is_none());
    assert!(journey_coverage(&g, "nonexistent").is_none());
}

#[test]
fn milestone_completion_reports_proven_features_beside_done_ones() {
    use specforge_extension_sdk::prelude::{CommandEvidence, EntityEvidence};
    let g = G::default()
        .node("f1", "feature", &[("status", "done")])
        .node("f2", "feature", &[("status", "done")])
        .n("b1", "behavior")
        .n("b2", "behavior")
        .node("ms", "milestone", &[("status", "completed")])
        .edge("ms", "f1", "features")
        .edge("ms", "f2", "features")
        .edge("b1", "f1", "features")
        .edge("b2", "f2", "features")
        .build();
    let scored = |b1_proven: usize| CommandEvidence::Recorded {
        entities: [
            (
                "b1".to_string(),
                EntityEvidence {
                    obligations: 1,
                    proven: b1_proven,
                    failing: 0,
                },
            ),
            (
                "b2".to_string(),
                EntityEvidence {
                    obligations: 1,
                    proven: 0,
                    failing: 0,
                },
            ),
        ]
        .into_iter()
        .collect(),
    };
    let r = milestone_completion(&g, "ms")
        .unwrap()
        .with_evidence(&g, &scored(1));
    assert_eq!(r.done_count, 2, "the declared count is unchanged");
    assert_eq!(r.proven_count, Some(1));
    assert_eq!(r.proven_features.as_deref(), Some(&["f1".to_string()][..]));
    assert_eq!(r.proven_ratio, Some(0.5));
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["evidence"]["state"], "recorded");
    assert_eq!(json["feature_evidence"][1]["proven_obligations"], 0);

    let none = milestone_completion(&g, "ms")
        .unwrap()
        .with_evidence(&g, &CommandEvidence::None);
    let json = serde_json::to_value(&none).unwrap();
    assert_eq!(json["evidence"]["state"], "none");
    assert!(json.get("proven_count").is_none());
    let plain = serde_json::to_value(milestone_completion(&g, "ms").unwrap()).unwrap();
    assert!(plain.get("evidence").is_none(), "not asked, not reported");
}
