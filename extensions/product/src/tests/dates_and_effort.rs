//! The date commands (timeline, velocity) and weighted completion.

use super::host::*;
use serde_json::{json, Value};
use specforge_protocol_types::CommandEvidence;
use specforge_test::prelude::*;

// ── dates and effort ──────────────────────────────────────────────────────

/// Milestones around the date the tests pass as today (2026-10-03):
/// done (2026-08-01, completed), late (2026-09-01, in progress, high),
/// next (2026-11-01, critical), a and b (both 2026-12-15) and undated.
fn timeline() -> G {
    G::default()
        .node(
            "done",
            "milestone",
            json!({"target_date": "2026-08-01", "status": "completed"}),
        )
        .node(
            "late",
            "milestone",
            json!({"target_date": "2026-09-01", "status": "in_progress", "priority": "high"}),
        )
        .node(
            "next",
            "milestone",
            json!({"target_date": "2026-11-01", "priority": "critical"}),
        )
        .node("b", "milestone", json!({"target_date": "2026-12-15"}))
        .node("a", "milestone", json!({"target_date": "2026-12-15"}))
        .n("undated", "milestone")
}

fn timeline_order(t: &Value) -> Vec<(String, bool)> {
    t["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["milestone_id"].as_str().unwrap().to_string(),
                m["is_overdue"].as_bool().unwrap(),
            )
        })
        .collect()
}

fn ids_flagged(order: &[(&str, bool)]) -> Vec<(String, bool)> {
    order.iter().map(|(id, o)| (id.to_string(), *o)).collect()
}

#[specforge_test(
    behavior = "surface_milestone_timeline",
    verify = "milestone-timeline returns MilestoneTimelinePayload JSON"
)]
fn milestone_timeline_answers_its_payload() {
    let t = json_of("milestone_timeline", json!({}), &timeline());
    assert_eq!(
        t,
        json!({
            "milestones": [
                {"milestone_id": "done", "target_date": "2026-08-01", "status": "completed",
                    "is_overdue": false, "priority": null},
                {"milestone_id": "late", "target_date": "2026-09-01", "status": "in_progress",
                    "is_overdue": true, "priority": "high"},
                {"milestone_id": "next", "target_date": "2026-11-01", "status": null,
                    "is_overdue": false, "priority": "critical"},
                {"milestone_id": "a", "target_date": "2026-12-15", "status": null,
                    "is_overdue": false, "priority": null},
                {"milestone_id": "b", "target_date": "2026-12-15", "status": null,
                    "is_overdue": false, "priority": null},
                {"milestone_id": "undated", "target_date": null, "status": null,
                    "is_overdue": false, "priority": null},
            ],
            "overdue_count": 1,
        })
    );
}

#[specforge_test(
    behavior = "surface_milestone_timeline",
    verify = "as-of flag overrides current date for overdue calculation"
)]
fn as_of_overrides_the_hosts_today() {
    let t = json_of(
        "milestone_timeline",
        json!({"as_of": "2026-12-01"}),
        &timeline(),
    );
    assert_eq!(t["overdue_count"], 2);
    let early = json_of(
        "milestone_timeline",
        json!({"as_of": "2026-01-01"}),
        &timeline(),
    );
    assert_eq!(early["overdue_count"], 0);
    // A date that is not one is refused, from --as-of or from the host.
    for args in [
        json!({"as_of": "2026-02-30"}),
        json!({"as_of": "tomorrow"}),
        json!({"as_of": 3}),
    ] {
        let out = run_in(
            &runtime(),
            "milestone_timeline",
            args.clone(),
            &timeline(),
            "json",
        );
        assert_eq!(out.exit, 2, "{args}");
        assert_eq!(out.error()["code"], "INVALID_INPUT", "{args}");
    }
    // The host passed no date: no "as of" to default to.
    let out = run_input(
        &runtime(),
        "milestone_timeline",
        json!({}),
        &timeline(),
        "json",
        "",
        CommandEvidence::None,
    );
    assert_eq!(out.exit, 2, "{}", out.stderr);
    assert!(
        out.stderr.contains("give a date to compare against"),
        "{}",
        out.stderr
    );
}

#[specforge_test(
    behavior = "surface_milestone_timeline",
    verify = "human format marks overdue milestones"
)]
fn the_human_timeline_marks_the_overdue() {
    let human = human_of("milestone_timeline", json!({}), &timeline());
    assert_eq!(
        human,
        "milestone  target_date  status       priority  overdue\n\
         done       2026-08-01   completed    -\n\
         late       2026-09-01   in_progress  high      OVERDUE\n\
         next       2026-11-01   -            critical\n\
         a          2026-12-15   -            -\n\
         b          2026-12-15   -            -\n\
         undated    -            -            -\n\
         1 overdue as of 2026-10-03\n"
    );
}

#[specforge_test(
    behavior = "pe_query_milestone_timeline",
    verify = "milestones sorted by target_date ascending"
)]
fn the_timeline_is_earliest_first() {
    let t = json_of("milestone_timeline", json!({}), &timeline());
    let dates: Vec<&str> = t["milestones"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["target_date"].as_str())
        .collect();
    let mut sorted = dates.clone();
    sorted.sort_unstable();
    assert_eq!(dates, sorted);
}

#[specforge_test(
    behavior = "pe_query_milestone_timeline",
    verify = "undated milestones appear after dated milestones"
)]
fn undated_milestones_come_last() {
    let g =
        timeline()
            .n("aaa", "milestone")
            .node("bad", "milestone", json!({"target_date": "soon"}));
    let t = json_of("milestone_timeline", json!({}), &g);
    let order: Vec<String> = timeline_order(&t).into_iter().map(|(id, _)| id).collect();
    assert_eq!(&order[5..], ["aaa", "bad", "undated"]);
}

#[specforge_test(
    behavior = "pe_query_milestone_timeline",
    verify = "overdue milestone with status=in_progress is flagged in query result"
)]
fn an_in_progress_milestone_past_its_date_is_overdue() {
    let t = json_of("milestone_timeline", json!({}), &timeline());
    assert_eq!(timeline_order(&t)[1], ("late".to_string(), true));
}

#[specforge_test(
    behavior = "pe_query_milestone_timeline",
    verify = "completed milestone past target_date is not flagged overdue"
)]
fn a_completed_milestone_is_never_overdue() {
    let t = json_of(
        "milestone_timeline",
        json!({"as_of": "2030-01-01"}),
        &timeline(),
    );
    assert_eq!(timeline_order(&t)[0], ("done".to_string(), false));
    assert_eq!(t["overdue_count"], 4);
}

#[specforge_test(
    behavior = "pe_query_milestone_timeline",
    verify = "empty milestone set returns empty timeline"
)]
fn no_milestones_is_an_empty_timeline() {
    assert_eq!(
        json_of("milestone_timeline", json!({}), &G::default()),
        json!({"milestones": [], "overdue_count": 0})
    );
}

#[specforge_test(
    behavior = "pe_query_milestone_timeline",
    verify = "same as_of_date produces identical timeline across repeated queries"
)]
fn one_as_of_date_is_one_timeline() {
    let args = json!({"as_of": "2026-11-15"});
    let first = json_of("milestone_timeline", args.clone(), &timeline());
    assert_eq!(
        json_of("milestone_timeline", args.clone(), &timeline()),
        first
    );
    assert_eq!(
        timeline_order(&first),
        ids_flagged(&[
            ("done", false),
            ("late", true),
            ("next", true),
            ("a", false),
            ("b", false),
            ("undated", false)
        ])
    );
}

#[specforge_test(
    behavior = "pe_query_milestone_timeline",
    verify = "milestone timeline is deterministic across repeated queries"
)]
fn the_timeline_is_the_same_every_time_and_in_any_order() {
    let first = json_of("milestone_timeline", json!({}), &timeline());
    let mut g = timeline();
    g.nodes.reverse();
    assert_eq!(json_of("milestone_timeline", json!({}), &g), first);
    assert_eq!(json_of("milestone_timeline", json!({}), &timeline()), first);
}

/// A milestone started 30 days before the tests' today with three of its
/// five features done and one in progress, and one with no dates.
fn velocity() -> G {
    G::default()
        .node(
            "ms",
            "milestone",
            json!({"start_date": "2026-09-03", "target_date": "2026-12-01"}),
        )
        .node("nodate", "milestone", json!({}))
        .node("due", "milestone", json!({"target_date": "2026-09-23"}))
        .feature("f1", "done")
        .feature("f2", "done")
        .feature("f3", "done")
        .feature("f4", "in_progress")
        .n("f5", "feature")
        .edge("ms", "f1", "features")
        .edge("ms", "f2", "features")
        .edge("ms", "f3", "features")
        .edge("ms", "f4", "features")
        .edge("ms", "f5", "features")
        .edge("nodate", "f1", "features")
        .edge("nodate", "f4", "features")
        .edge("due", "f4", "features")
        .edge("due", "f5", "features")
}

#[specforge_test(
    behavior = "surface_milestone_velocity",
    verify = "milestone-velocity returns MilestoneVelocityPayload JSON"
)]
fn milestone_velocity_answers_its_payload() {
    let v = json_of(
        "milestone_velocity",
        json!({"milestone": "ms"}),
        &velocity(),
    );
    assert_eq!(
        v,
        json!({"milestone_id": "ms", "total_features": 5, "done_features": 3,
            "in_progress_features": 1, "remaining_features": 1, "completion_ratio": 0.6,
            "days_elapsed": 30, "days_remaining": 20, "features_per_day": 0.1})
    );
    let human = human_of(
        "milestone_velocity",
        json!({"milestone": "ms"}),
        &velocity(),
    );
    assert_eq!(
        human,
        "Milestone: ms (as of 2026-10-03)\n\
         Features: 5 total, 3 done, 1 in progress, 1 remaining\n\
         Completion: 60%\n\
         Days elapsed: 30\n\
         Features per day: 0.10\n\
         Days remaining: 20\n"
    );
    let later = json_of(
        "milestone_velocity",
        json!({"milestone": "ms", "as_of": "2026-10-13"}),
        &velocity(),
    );
    assert_eq!(
        (
            later["days_elapsed"].clone(),
            later["features_per_day"].clone()
        ),
        (json!(40), json!(0.075))
    );
}

#[specforge_test(
    behavior = "surface_milestone_velocity",
    verify = "missing milestone ID returns error with suggestion"
)]
fn milestone_velocity_of_a_mistyped_milestone_suggests_the_nearest() {
    let error = not_found_suggesting(
        "milestone_velocity",
        json!({"milestone": "mss"}),
        &velocity(),
    );
    assert_eq!(error["suggestion"], "ms");
}

#[specforge_test(
    behavior = "pe_query_milestone_velocity",
    verify = "milestone with 3 done and 2 remaining returns correct counts"
)]
fn three_done_of_five_are_counted_by_status() {
    let v = json_of(
        "milestone_velocity",
        json!({"milestone": "ms"}),
        &velocity(),
    );
    let counts = |k: &str| v[k].as_u64().unwrap();
    assert_eq!(
        (
            counts("done_features"),
            counts("total_features") - counts("done_features")
        ),
        (3, 2)
    );
    assert_eq!(
        counts("total_features"),
        counts("done_features") + counts("in_progress_features") + counts("remaining_features")
    );
}

#[specforge_test(
    behavior = "pe_query_milestone_velocity",
    verify = "milestone with no done features returns null velocity"
)]
fn a_milestone_with_nothing_done_has_no_velocity() {
    // due's date is the start: 10 days elapsed, nothing done.
    let v = json_of(
        "milestone_velocity",
        json!({"milestone": "due"}),
        &velocity(),
    );
    assert_eq!(
        (
            v["days_elapsed"].clone(),
            v["features_per_day"].clone(),
            v["days_remaining"].clone()
        ),
        (json!(10), json!(null), json!(null))
    );
}

#[specforge_test(
    behavior = "pe_query_milestone_velocity",
    verify = "milestone without target_date returns null days_elapsed"
)]
fn a_milestone_without_dates_has_no_days() {
    let v = json_of(
        "milestone_velocity",
        json!({"milestone": "nodate"}),
        &velocity(),
    );
    assert_eq!(
        (
            v["days_elapsed"].clone(),
            v["days_remaining"].clone(),
            v["features_per_day"].clone()
        ),
        (json!(null), json!(null), json!(null))
    );
    assert_eq!(v["completion_ratio"], 0.5);
}

#[specforge_test(
    behavior = "pe_query_milestone_velocity",
    verify = "velocity calculation is mathematically correct"
)]
fn velocity_is_done_over_elapsed_and_paces_what_is_left() {
    for (as_of, elapsed) in [
        ("2026-09-04", 1),
        ("2026-09-10", 7),
        ("2026-10-03", 30),
        ("2027-09-03", 365),
    ] {
        let v = json_of(
            "milestone_velocity",
            json!({"milestone": "ms", "as_of": as_of}),
            &velocity(),
        );
        let pace = 3.0 / elapsed as f64;
        assert_eq!(v["days_elapsed"], elapsed, "{as_of}");
        assert_eq!(v["features_per_day"].as_f64().unwrap(), pace, "{as_of}");
        // 2 left at 3 per elapsed days: 2 * elapsed / 3 days, rounded up.
        assert_eq!(v["days_remaining"], (2 * elapsed + 2) / 3, "{as_of}");
    }
    // Before the start no day has elapsed.
    let v = json_of(
        "milestone_velocity",
        json!({"milestone": "ms", "as_of": "2026-01-01"}),
        &velocity(),
    );
    assert_eq!(
        (v["days_elapsed"].clone(), v["features_per_day"].clone()),
        (json!(0), json!(null))
    );
}

/// A milestone with an xs feature done and an xl one pending, one with
/// a feature of each effort and one without, and an empty one.
fn efforts() -> G {
    G::default()
        .node("x1", "feature", json!({"effort": "xs", "status": "done"}))
        .node("x2", "feature", json!({"effort": "xl"}))
        .node("e1", "feature", json!({"effort": "xs", "status": "done"}))
        .node("e2", "feature", json!({"effort": "s"}))
        .node("e3", "feature", json!({"effort": "m", "status": "done"}))
        .node("e4", "feature", json!({"effort": "l"}))
        .node("e5", "feature", json!({"effort": "xl", "status": "done"}))
        .node("e6", "feature", json!({"status": "done"}))
        .n("ms_pair", "milestone")
        .n("ms_all", "milestone")
        .n("ms_empty", "milestone")
        .edge("ms_pair", "x1", "features")
        .edge("ms_pair", "x2", "features")
        .edge("ms_all", "e1", "features")
        .edge("ms_all", "e2", "features")
        .edge("ms_all", "e3", "features")
        .edge("ms_all", "e4", "features")
        .edge("ms_all", "e5", "features")
        .edge("ms_all", "e6", "features")
}

#[specforge_test(
    behavior = "surface_weighted_milestone_completion",
    verify = "weighted-milestone-completion returns effort breakdown"
)]
fn weighted_completion_answers_its_breakdown() {
    let w = json_of(
        "weighted_milestone_completion",
        json!({"milestone": "ms_all"}),
        &efforts(),
    );
    assert_eq!(
        w,
        json!({"milestone_id": "ms_all", "total_effort": 22, "done_effort": 15,
        "completion_ratio": 15.0 / 22.0, "effort_breakdown": [
            {"effort_level": "xs", "total": 1, "done": 1},
            {"effort_level": "s", "total": 1, "done": 0},
            {"effort_level": "m", "total": 2, "done": 2},
            {"effort_level": "l", "total": 1, "done": 0},
            {"effort_level": "xl", "total": 1, "done": 1},
        ]})
    );
    let human = human_of(
        "weighted_milestone_completion",
        json!({"milestone": "ms_pair"}),
        &efforts(),
    );
    assert_eq!(
        human,
        "Milestone: ms_pair\n\
         Weighted completion: 11% (1/9 effort points done)\n\
         effort  weight  features  done\n\
         xs      1       1         1\n\
         xl      8       1         0\n"
    );
}

#[specforge_test(
    behavior = "surface_weighted_milestone_completion",
    verify = "weighted-milestone-completion with unknown ID returns ENTITY_NOT_FOUND"
)]
fn weighted_completion_of_no_milestone_is_not_found() {
    let error = not_found_suggesting(
        "weighted_milestone_completion",
        json!({"milestone": "ms_al"}),
        &efforts(),
    );
    assert_eq!(error["suggestion"], "ms_all");
}

#[specforge_test(
    behavior = "pe_query_weighted_milestone_completion",
    verify = "milestone with xs(done) + xl(pending) returns ratio 1/9"
)]
fn xs_done_and_xl_pending_is_one_ninth() {
    let w = json_of(
        "weighted_milestone_completion",
        json!({"milestone": "ms_pair"}),
        &efforts(),
    );
    assert_eq!(
        (w["done_effort"].clone(), w["total_effort"].clone()),
        (json!(1), json!(9))
    );
    assert_eq!(w["completion_ratio"], 1.0 / 9.0);
}

#[specforge_test(
    behavior = "pe_query_weighted_milestone_completion",
    verify = "feature without effort defaults to m=3 weight"
)]
fn a_feature_without_effort_weighs_three() {
    let g = G::default()
        .feature("f", "done")
        .node("g", "feature", json!({"effort": "xs"}))
        .n("ms", "milestone")
        .edge("ms", "f", "features")
        .edge("ms", "g", "features");
    let w = json_of(
        "weighted_milestone_completion",
        json!({"milestone": "ms"}),
        &g,
    );
    assert_eq!(
        (w["done_effort"].clone(), w["total_effort"].clone()),
        (json!(3), json!(4))
    );
}

#[specforge_test(
    behavior = "pe_query_weighted_milestone_completion",
    verify = "empty milestone returns null completion_ratio"
)]
fn an_empty_milestone_has_no_weighted_ratio() {
    assert_eq!(
        json_of(
            "weighted_milestone_completion",
            json!({"milestone": "ms_empty"}),
            &efforts()
        ),
        json!({"milestone_id": "ms_empty", "total_effort": 0, "done_effort": 0,
            "completion_ratio": null, "effort_breakdown": []})
    );
}

#[specforge_test(
    behavior = "product_effort_weight_correctness",
    verify = "each effort level maps to its Fibonacci weight"
)]
fn each_effort_level_weighs_its_fibonacci_number() {
    for (effort, weight) in [("xs", 1), ("s", 2), ("m", 3), ("l", 5), ("xl", 8)] {
        let g = G::default()
            .node("f", "feature", json!({"effort": effort}))
            .n("ms", "milestone")
            .edge("ms", "f", "features");
        let w = json_of(
            "weighted_milestone_completion",
            json!({"milestone": "ms"}),
            &g,
        );
        assert_eq!(w["total_effort"], weight, "{effort}");
    }
}

#[specforge_test(
    behavior = "product_effort_weight_correctness",
    verify = "missing effort defaults to m weight"
)]
fn a_missing_or_unknown_effort_weighs_as_m() {
    for fields in [json!({}), json!({"effort": "huge"})] {
        let g = G::default()
            .node("f", "feature", fields.clone())
            .n("ms", "milestone")
            .edge("ms", "f", "features");
        let w = json_of(
            "weighted_milestone_completion",
            json!({"milestone": "ms"}),
            &g,
        );
        assert_eq!(w["total_effort"], 3, "{fields}");
        assert_eq!(
            w["effort_breakdown"],
            json!([{"effort_level": "m", "total": 1, "done": 0}])
        );
    }
}
