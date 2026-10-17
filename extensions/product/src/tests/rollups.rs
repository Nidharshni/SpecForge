//! Status and progress rollups: deliverable and release completion, deliverable priority, unscheduled features, owner workload.

use super::host::*;
use serde_json::{json, Value};
use specforge_test::prelude::*;

// ── status and progress rollups ───────────────────────────────────────────

/// Deliverables tracked by milestones, a release over them, features some
/// milestone schedules and owners:
/// - d1: ms1 (completed, critical), ms2 (completed); shipped, owner bo
/// - d2: ms2, ms3 (in_progress, medium); j1 (high); shipped
/// - d3: nothing; no status (draft)
/// - d4: ms4 (no status); j2
/// - d5: j1 (high) and five constituents without a priority
/// - r1: d1, d2, d3; owner bo. r2: nothing
/// - f1 (ms1), f2 (ms3) scheduled; f3 (only a journey's), f4 not
fn progress() -> G {
    G::default()
        .node(
            "d1",
            "deliverable",
            json!({"status": "shipped", "owner": "bo"}),
        )
        .node("d2", "deliverable", json!({"status": "shipped"}))
        .n("d3", "deliverable")
        .n("d4", "deliverable")
        .n("d5", "deliverable")
        .node(
            "ms1",
            "milestone",
            json!({"status": "completed", "priority": "critical", "owner": "al"}),
        )
        .node("ms2", "milestone", json!({"status": "completed"}))
        .node(
            "ms3",
            "milestone",
            json!({"status": "in_progress", "priority": "medium"}),
        )
        .n("ms4", "milestone")
        .node("j1", "journey", json!({"priority": "high"}))
        .n("j2", "journey")
        .n("j3", "journey")
        .n("j4", "journey")
        .node("r1", "release", json!({"owner": "bo"}))
        .n("r2", "release")
        .node("f1", "feature", json!({"status": "done", "owner": "al"}))
        .node("f2", "feature", json!({"owner": "al"}))
        .node(
            "f3",
            "feature",
            json!({"status": "accepted", "owner": "cy"}),
        )
        .n("f4", "feature")
        .edge("d1", "ms1", "milestones")
        .edge("d1", "ms2", "milestones")
        .edge("d2", "ms2", "milestones")
        .edge("d2", "ms3", "milestones")
        .edge("d2", "j1", "journeys")
        .edge("d4", "ms4", "milestones")
        .edge("d4", "j2", "journeys")
        .edge("d5", "j1", "journeys")
        .edge("d5", "j2", "journeys")
        .edge("d5", "j3", "journeys")
        .edge("d5", "j4", "journeys")
        .edge("d5", "ms2", "milestones")
        .edge("d5", "ms4", "milestones")
        .edge("r1", "d1", "deliverables")
        .edge("r1", "d2", "deliverables")
        .edge("r1", "d3", "deliverables")
        .edge("ms1", "f1", "features")
        .edge("ms3", "f2", "features")
        .edge("j1", "f3", "features")
}

/// [`progress`] with its references declared in the opposite order.
fn progress_reversed() -> G {
    let mut g = progress();
    g.edges.reverse();
    g.nodes.reverse();
    g
}

#[specforge_test(
    behavior = "surface_deliverable_completion",
    verify = "deliverable-completion returns DeliverableCompletionPayload JSON"
)]
fn deliverable_completion_answers_its_payload() {
    let dc = json_of(
        "deliverable_completion",
        json!({"deliverable": "d2"}),
        &progress(),
    );
    assert_eq!(
        dc,
        json!({"deliverable_id": "d2", "milestone_count": 2, "completed_count": 1,
            "completion_ratio": 0.5})
    );
    // asking for details adds each milestone's own completion.
    let dc = json_of(
        "deliverable_completion",
        json!({"deliverable": "d2", "details": true}),
        &progress(),
    );
    assert_eq!(
        dc["milestone_details"],
        json!([
            {"milestone_id": "ms2", "total_features": 0, "done_count": 0,
                "completion_ratio": 0.0, "done_features": []},
            {"milestone_id": "ms3", "total_features": 1, "done_count": 0,
                "completion_ratio": 0.0, "done_features": []},
        ])
    );
    let human = human_of(
        "deliverable_completion",
        json!({"deliverable": "d2", "details": true}),
        &progress(),
    );
    assert!(
        human.contains("Completion: 50% (1/2 milestones completed)"),
        "{human}"
    );
    assert!(human.contains("milestone  status"), "{human}");
}

#[specforge_test(
    behavior = "surface_deliverable_completion",
    verify = "missing deliverable ID returns error with suggestion"
)]
fn deliverable_completion_of_a_mistyped_deliverable_suggests_the_nearest() {
    let error = not_found_suggesting(
        "deliverable_completion",
        json!({"deliverable": "dd1"}),
        &progress(),
    );
    assert_eq!(error["suggestion"], "d1");
}

#[specforge_test(
    behavior = "pe_query_deliverable_completion",
    verify = "deliverable with all completed milestones returns ratio 1.0"
)]
fn a_deliverable_whose_milestones_are_all_completed_is_complete() {
    let dc = json_of(
        "deliverable_completion",
        json!({"deliverable": "d1"}),
        &progress(),
    );
    assert_eq!(
        (
            dc["completed_count"].clone(),
            dc["completion_ratio"].clone()
        ),
        (json!(2), json!(1.0))
    );
}

#[specforge_test(
    behavior = "pe_query_deliverable_completion",
    verify = "deliverable with no completed milestones returns ratio 0.0"
)]
fn a_deliverable_without_a_completed_milestone_is_at_zero() {
    // ms4 has no status: it is planned, not completed.
    let dc = json_of(
        "deliverable_completion",
        json!({"deliverable": "d4"}),
        &progress(),
    );
    assert_eq!(
        (dc["milestone_count"].clone(), dc["completed_count"].clone()),
        (json!(1), json!(0))
    );
    assert_eq!(dc["completion_ratio"], 0.0);
}

#[specforge_test(
    behavior = "pe_query_deliverable_completion",
    verify = "deliverable with zero milestones returns ratio 0.0"
)]
fn a_deliverable_without_milestones_is_at_zero_without_dividing_by_zero() {
    let dc = json_of(
        "deliverable_completion",
        json!({"deliverable": "d3"}),
        &progress(),
    );
    assert_eq!(
        dc,
        json!({"deliverable_id": "d3", "milestone_count": 0, "completed_count": 0,
            "completion_ratio": 0.0})
    );
}

#[specforge_test(
    behavior = "pe_query_deliverable_completion",
    verify = "deliverable with mix of completed and non-completed milestones returns partial ratio"
)]
fn a_deliverable_half_completed_is_at_one_half() {
    let dc = json_of(
        "deliverable_completion",
        json!({"deliverable": "d2"}),
        &progress(),
    );
    assert_eq!(dc["completion_ratio"], 0.5);
}

#[specforge_test(
    behavior = "pe_query_deliverable_completion",
    verify = "deliverable completion is deterministic across repeated queries"
)]
fn deliverable_completion_is_the_same_every_time_and_in_any_order() {
    let args = json!({"deliverable": "d2", "details": true});
    let first = json_of("deliverable_completion", args.clone(), &progress());
    assert_eq!(
        json_of("deliverable_completion", args.clone(), &progress()),
        first
    );
    assert_eq!(
        json_of("deliverable_completion", args, &progress_reversed()),
        first
    );
}

#[specforge_test(
    behavior = "surface_release_completion",
    verify = "release-completion returns correct shipped/total ratio"
)]
fn release_completion_answers_shipped_over_total() {
    let rc = json_of("release_completion", json!({"release": "r1"}), &progress());
    assert_eq!(
        rc,
        json!({"release_id": "r1", "total": 3, "shipped": 2, "completion_ratio": 2.0 / 3.0})
    );
    let human = human_of("release_completion", json!({"release": "r1"}), &progress());
    assert!(
        human.contains("Completion: 67% (2/3 deliverables shipped)"),
        "{human}"
    );
    let error = not_found_suggesting("release_completion", json!({"release": "r9"}), &progress());
    assert_eq!(error["suggestion"], "r1");
}

#[specforge_test(
    behavior = "pe_query_release_completion",
    verify = "release with 2/3 shipped returns ratio 0.667"
)]
fn a_release_with_two_of_three_shipped_is_at_two_thirds() {
    // d3 has no status: it is a draft, not shipped.
    let rc = json_of("release_completion", json!({"release": "r1"}), &progress());
    let ratio = rc["completion_ratio"].as_f64().unwrap();
    assert!((ratio - 0.667).abs() < 0.001, "{ratio}");
}

#[specforge_test(
    behavior = "pe_query_release_completion",
    verify = "release with no deliverables returns null ratio"
)]
fn a_release_without_deliverables_has_no_ratio() {
    let rc = json_of("release_completion", json!({"release": "r2"}), &progress());
    assert_eq!(
        rc,
        json!({"release_id": "r2", "total": 0, "shipped": 0, "completion_ratio": null})
    );
}

#[specforge_test(
    behavior = "surface_deliverable_priority",
    verify = "deliverable-priority returns DeliverablePriorityPayload JSON"
)]
fn deliverable_priority_answers_its_payload() {
    let dp = json_of(
        "deliverable_priority",
        json!({"deliverable": "d1"}),
        &progress(),
    );
    assert_eq!(
        dp,
        json!({"deliverable_id": "d1", "priority": "critical", "source_count": 1})
    );
    // With nothing to derive it from, the priority is null.
    let dp = json_of(
        "deliverable_priority",
        json!({"deliverable": "d3"}),
        &progress(),
    );
    assert_eq!(dp["priority"], Value::Null);
}

#[specforge_test(
    behavior = "surface_deliverable_priority",
    verify = "missing deliverable ID returns error with suggestion"
)]
fn deliverable_priority_of_a_mistyped_deliverable_suggests_the_nearest() {
    let error = not_found_suggesting(
        "deliverable_priority",
        json!({"deliverable": "d22"}),
        &progress(),
    );
    assert_eq!(error["suggestion"], "d2");
}

#[specforge_test(
    behavior = "pe_query_deliverable_priority",
    verify = "deliverable with critical milestone returns critical priority"
)]
fn a_critical_milestone_makes_its_deliverable_critical() {
    let dp = json_of(
        "deliverable_priority",
        json!({"deliverable": "d1"}),
        &progress(),
    );
    assert_eq!(dp["priority"], "critical");
}

#[specforge_test(
    behavior = "pe_query_deliverable_priority",
    verify = "deliverable with high journey and medium milestone returns high priority"
)]
fn a_high_journey_outranks_a_medium_milestone() {
    let dp = json_of(
        "deliverable_priority",
        json!({"deliverable": "d2"}),
        &progress(),
    );
    assert_eq!(
        (dp["priority"].clone(), dp["source_count"].clone()),
        (json!("high"), json!(2))
    );
}

#[specforge_test(
    behavior = "pe_query_deliverable_priority",
    verify = "deliverable with no milestones and no journeys returns null priority"
)]
fn a_deliverable_with_no_constituents_has_no_priority() {
    let dp = json_of(
        "deliverable_priority",
        json!({"deliverable": "d3"}),
        &progress(),
    );
    assert_eq!(
        dp,
        json!({"deliverable_id": "d3", "priority": null, "source_count": 0})
    );
}

#[specforge_test(
    behavior = "pe_query_deliverable_priority",
    verify = "deliverable where all constituents have null priority returns null priority"
)]
fn a_deliverable_whose_constituents_declare_no_priority_has_none() {
    // d4's ms4 and j2 declare none; none is not medium.
    let dp = json_of(
        "deliverable_priority",
        json!({"deliverable": "d4"}),
        &progress(),
    );
    assert_eq!(
        (dp["priority"].clone(), dp["source_count"].clone()),
        (Value::Null, json!(0))
    );
}

#[specforge_test(
    behavior = "pe_query_deliverable_priority",
    verify = "deliverable with one prioritized and five unprioritized returns the one priority"
)]
fn one_declared_priority_among_six_constituents_is_the_priority() {
    let dp = json_of(
        "deliverable_priority",
        json!({"deliverable": "d5"}),
        &progress(),
    );
    assert_eq!(dp["priority"], "high");
}

#[specforge_test(
    behavior = "pe_query_deliverable_priority",
    verify = "source_count counts only entities with explicit priority"
)]
fn the_source_count_is_the_constituents_with_a_priority() {
    // d5 has six constituents; only j1 declares a priority.
    let dp = json_of(
        "deliverable_priority",
        json!({"deliverable": "d5"}),
        &progress(),
    );
    assert_eq!(dp["source_count"], 1);
}

#[specforge_test(
    behavior = "pe_query_deliverable_priority",
    verify = "deliverable priority is deterministic across repeated queries"
)]
fn deliverable_priority_is_the_same_every_time_and_in_any_order() {
    for d in ["d1", "d2", "d5"] {
        let args = json!({ "deliverable": d });
        let first = json_of("deliverable_priority", args.clone(), &progress());
        assert_eq!(
            json_of("deliverable_priority", args.clone(), &progress()),
            first
        );
        assert_eq!(
            json_of("deliverable_priority", args, &progress_reversed()),
            first
        );
    }
}

#[specforge_test(
    behavior = "surface_unscheduled_features",
    verify = "product:unscheduled-features returns UnscheduledFeaturesPayload"
)]
fn unscheduled_features_answers_its_payload() {
    let uf = json_of("unscheduled_features", json!({}), &progress());
    assert_eq!(
        uf,
        json!({"features": ["f3", "f4"], "count": 2, "total_features": 4, "scheduled_count": 2})
    );
    let human = human_of("unscheduled_features", json!({}), &progress());
    assert_eq!(
        human,
        "id  status\nf3  accepted\nf4  -\n2 of 4 features unscheduled\n"
    );
}

#[specforge_test(
    behavior = "surface_unscheduled_features",
    verify = "no unscheduled features returns empty list"
)]
fn a_plan_with_every_feature_scheduled_has_none_unscheduled() {
    let g = progress()
        .edge("ms2", "f3", "features")
        .edge("ms2", "f4", "features");
    let uf = json_of("unscheduled_features", json!({}), &g);
    assert_eq!(
        (uf["features"].clone(), uf["count"].clone()),
        (json!([]), json!(0))
    );
}

#[specforge_test(
    behavior = "pe_query_unscheduled_features",
    verify = "feature in no milestone appears in unscheduled list"
)]
fn a_feature_in_no_milestone_is_unscheduled() {
    // f3 is a journey's feature, but no milestone's.
    let uf = json_of("unscheduled_features", json!({}), &progress());
    assert!(uf["features"].as_array().unwrap().contains(&json!("f3")));
    assert!(uf["features"].as_array().unwrap().contains(&json!("f4")));
}

#[specforge_test(
    behavior = "pe_query_unscheduled_features",
    verify = "feature in one milestone is excluded from unscheduled list"
)]
fn a_feature_in_a_milestone_is_scheduled() {
    let uf = json_of("unscheduled_features", json!({}), &progress());
    assert!(!uf["features"].as_array().unwrap().contains(&json!("f1")));
    assert!(!uf["features"].as_array().unwrap().contains(&json!("f2")));
}

#[specforge_test(
    behavior = "pe_query_unscheduled_features",
    verify = "empty graph returns empty list"
)]
fn an_empty_graph_has_nothing_unscheduled() {
    let uf = json_of("unscheduled_features", json!({}), &G::default());
    assert_eq!(
        uf,
        json!({"features": [], "count": 0, "total_features": 0, "scheduled_count": 0})
    );
}

#[specforge_test(
    behavior = "pe_query_unscheduled_features",
    verify = "count + scheduled_count == total_features"
)]
fn unscheduled_and_scheduled_add_up_to_every_feature() {
    for g in [progress(), plan(), shipping(), G::default()] {
        let uf = json_of("unscheduled_features", json!({}), &g);
        let count = uf["count"].as_u64().unwrap();
        assert_eq!(count, uf["features"].as_array().unwrap().len() as u64);
        assert_eq!(
            count + uf["scheduled_count"].as_u64().unwrap(),
            uf["total_features"].as_u64().unwrap()
        );
    }
}

#[specforge_test(
    behavior = "product_new_query_correctness",
    verify = "unscheduled features have zero MilestoneDeliversFeature edges"
)]
fn an_unscheduled_feature_is_one_no_milestone_lists() {
    // A journey or module listing a feature does not schedule it; a
    // milestone does.
    let g = progress()
        .n("mod1", "module")
        .edge("mod1", "f4", "features");
    let uf = json_of("unscheduled_features", json!({}), &g);
    assert_eq!(uf["features"], json!(["f3", "f4"]));
    let g = g.edge("ms4", "f4", "features");
    let uf = json_of("unscheduled_features", json!({}), &g);
    assert_eq!(uf["features"], json!(["f3"]));
}

#[specforge_test(
    behavior = "surface_owner_workload",
    verify = "owner-workload returns grouped ownership statistics"
)]
fn owner_workload_groups_entities_by_owner() {
    let ow = json_of("owner_workload", json!({}), &progress());
    assert_eq!(
        ow,
        json!({
            "owners": [
                {"owner": "al", "entity_ids": ["f1", "f2", "ms1"], "entity_count": 3,
                    "by_kind": {"features": 2, "milestones": 1, "deliverables": 0, "releases": 0}},
                {"owner": "bo", "entity_ids": ["d1", "r1"], "entity_count": 2,
                    "by_kind": {"features": 0, "milestones": 0, "deliverables": 1, "releases": 1}},
                {"owner": "cy", "entity_ids": ["f3"], "entity_count": 1,
                    "by_kind": {"features": 1, "milestones": 0, "deliverables": 0, "releases": 0}},
            ],
            "unowned_count": 9, "total_entities": 15,
            "total": 3, "offset": 0, "limit": 100, "has_more": false,
        })
    );
    // One page of owners, per the shared offset/limit contract.
    let page = json_of(
        "owner_workload",
        json!({"offset": 1, "limit": 1}),
        &progress(),
    );
    assert_eq!(page["owners"][0]["owner"], "bo");
    assert_eq!(
        (page["total"].clone(), page["has_more"].clone()),
        (json!(3), json!(true))
    );
    assert_eq!(page["unowned_count"], 9);
    let human = human_of("owner_workload", json!({}), &progress());
    assert!(
        human
            .starts_with("owner  entities  features  milestones  deliverables  releases\nal     3"),
        "{human}"
    );
}

#[specforge_test(
    behavior = "surface_owner_workload",
    verify = "owner-workload reports unowned entities"
)]
fn owner_workload_counts_the_unowned() {
    let ow = json_of("owner_workload", json!({}), &progress());
    assert_eq!(ow["unowned_count"], 9);
    let human = human_of("owner_workload", json!({}), &progress());
    assert!(human.contains("Unowned: 9 of 15 entities"), "{human}");
}

#[specforge_test(
    behavior = "pe_query_owner_workload",
    verify = "single owner across multiple kinds returns correct breakdown"
)]
fn one_owner_across_kinds_is_broken_down_by_kind() {
    let g = G::default()
        .node("f1", "feature", json!({"owner": "al"}))
        .node("ms1", "milestone", json!({"owner": "al"}))
        .node("d1", "deliverable", json!({"owner": "al"}))
        .node("r1", "release", json!({"owner": "al"}))
        .node("r2", "release", json!({"owner": "al"}));
    let ow = json_of("owner_workload", json!({}), &g);
    assert_eq!(ow["owners"].as_array().unwrap().len(), 1);
    assert_eq!(
        ow["owners"][0]["by_kind"],
        json!({"features": 1, "milestones": 1, "deliverables": 1, "releases": 2})
    );
    assert_eq!(ow["owners"][0]["entity_count"], 5);
}

#[specforge_test(
    behavior = "pe_query_owner_workload",
    verify = "entities without owner contribute to unowned_count"
)]
fn an_entity_without_an_owner_is_unowned() {
    // A module has no owner field: it is not counted at all.
    let g = G::default()
        .node("f1", "feature", json!({"owner": "al"}))
        .n("f2", "feature")
        .node("ms1", "milestone", json!({"owner": ""}))
        .n("mod1", "module");
    let ow = json_of("owner_workload", json!({}), &g);
    assert_eq!(
        (ow["unowned_count"].clone(), ow["total_entities"].clone()),
        (json!(2), json!(3))
    );
    let owned: u64 = ow["owners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["entity_count"].as_u64().unwrap())
        .sum();
    assert_eq!(owned + 2, 3);
}

#[specforge_test(
    behavior = "pe_query_owner_workload",
    verify = "empty graph returns zero totals"
)]
fn an_empty_graph_has_no_workload() {
    let ow = json_of("owner_workload", json!({}), &G::default());
    assert_eq!(
        ow,
        json!({"owners": [], "unowned_count": 0, "total_entities": 0,
            "total": 0, "offset": 0, "limit": 100, "has_more": false})
    );
}
