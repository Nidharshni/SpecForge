use crate::queries::*;
use crate::tests::G;

#[test]
fn a_rollup_counts_a_missing_status_as_its_kinds_default() {
    // A milestone without a status is planned, not completed; a
    // deliverable without one is draft, not shipped.
    let g = G::default()
        .n("d1", "deliverable")
        .node("d2", "deliverable", &[("status", "shipped")])
        .n("ms1", "milestone")
        .node("ms2", "milestone", &[("status", "completed")])
        .n("r1", "release")
        .edge("d1", "ms1", "milestones")
        .edge("d1", "ms2", "milestones")
        .edge("r1", "d1", "deliverables")
        .edge("r1", "d2", "deliverables")
        .build();
    let dc = deliverable_completion(&g, "d1", false).unwrap();
    assert_eq!((dc.completed_count, dc.milestone_count), (1, 2));
    assert!(dc.milestone_details.is_none());
    let rc = release_completion(&g, "r1").unwrap();
    assert_eq!(
        (rc.shipped, rc.total, rc.completion_ratio),
        (1, 2, Some(0.5))
    );
    assert!(release_completion(&g, "d1").is_none());
}

#[test]
fn a_deliverables_priority_is_its_highest_declared_one() {
    let g = G::default()
        .n("d1", "deliverable")
        .node("ms1", "milestone", &[("priority", "medium")])
        .node("j1", "journey", &[("priority", "high")])
        .n("j2", "journey")
        .edge("d1", "ms1", "milestones")
        .edge("d1", "j1", "journeys")
        .edge("d1", "j2", "journeys")
        .build();
    let dp = deliverable_priority(&g, "d1").unwrap();
    assert_eq!((dp.priority.as_deref(), dp.source_count), (Some("high"), 2));
}

#[test]
fn owners_are_ranked_by_how_much_they_own() {
    let g = G::default()
        .node("f1", "feature", &[("owner", "bo")])
        .node("f2", "feature", &[("owner", " al ")])
        .node("ms1", "milestone", &[("owner", "al")])
        .node("d1", "deliverable", &[("owner", "")])
        .node("mod1", "module", &[("owner", "al")])
        .build();
    let w = owner_workload(&g);
    let owners: Vec<(&str, usize)> = w
        .owners
        .iter()
        .map(|o| (o.owner.as_str(), o.entity_count))
        .collect();
    assert_eq!(owners, [("al", 2), ("bo", 1)]);
    assert_eq!(w.owners[0].entity_ids, ["f2", "ms1"]);
    assert_eq!((w.unowned_count, w.total_entities), (1, 4));
}
