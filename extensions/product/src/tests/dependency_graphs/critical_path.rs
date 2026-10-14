use super::reversed;
use crate::tests::host::*;
use serde_json::{json, Value};
use specforge_test::prelude::*;

/// Milestones over `depends_on` (each on the one before):
/// - m0 (completed) <- m1 (in_progress, 2026-11-01) <- m2 (blocked,
///   2026-12-01) <- m3 (no date)
/// - n1 (2026-10-15) <- n2 (2026-10-30)
fn schedule() -> G {
    G::default()
        .node(
            "m0",
            "milestone",
            json!({"status": "completed", "target_date": "2026-10-01"}),
        )
        .node(
            "m1",
            "milestone",
            json!({"status": "in_progress", "target_date": "2026-11-01"}),
        )
        .node(
            "m2",
            "milestone",
            json!({"status": "blocked", "target_date": "2026-12-01"}),
        )
        .n("m3", "milestone")
        .node("n1", "milestone", json!({"target_date": "2026-10-15"}))
        .node("n2", "milestone", json!({"target_date": "2026-10-30"}))
        .edge("m1", "m0", "depends_on")
        .edge("m2", "m1", "depends_on")
        .edge("m3", "m2", "depends_on")
        .edge("n2", "n1", "depends_on")
}

fn path_ids(cp: &Value) -> Vec<String> {
    cp["critical_path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["entity_id"].as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "surface_critical_path",
    verify = "product:critical-path returns CriticalPathPayload"
)]
fn critical_path_answers_its_payload() {
    let cp = json_of("critical_path", json!({}), &schedule());
    assert_eq!(
        cp,
        json!({
            "critical_path": [
                {"entity_id": "m1", "entity_kind": "milestone", "target_date": "2026-11-01",
                    "status": "in_progress", "slack_days": 0},
                {"entity_id": "m2", "entity_kind": "milestone", "target_date": "2026-12-01",
                    "status": "blocked", "slack_days": 0},
                {"entity_id": "m3", "entity_kind": "milestone", "target_date": null,
                    "status": null, "slack_days": null},
            ],
            "path_length": 3,
            "earliest_completion": "2026-11-01",
            "latest_completion": null,
            "bottleneck_ids": ["m1", "m2"],
        })
    );
    let human = human_of("critical_path", json!({}), &schedule());
    assert_eq!(
        human,
        "milestone  target_date  status       slack\n\
         m1         2026-11-01   in_progress  0\n\
         m2         2026-12-01   blocked      0\n\
         m3         -            -            -\n\
         Bottlenecks: m1, m2\n"
    );
}

#[specforge_test(
    behavior = "surface_critical_path",
    verify = "empty graph returns empty path"
)]
fn critical_path_of_no_milestones_is_empty() {
    let cp = json_of("critical_path", json!({}), &G::default());
    assert_eq!(
        cp,
        json!({"critical_path": [], "path_length": 0, "earliest_completion": null,
            "latest_completion": null, "bottleneck_ids": []})
    );
}

#[specforge_test(
    behavior = "surface_critical_path",
    verify = "cycles return empty path with diagnostic message"
)]
fn a_milestone_cycle_gives_an_empty_path_and_says_why() {
    let g = schedule().edge("m1", "m3", "depends_on");
    let out = run_in(&runtime(), "critical_path", json!({}), &g, "json");
    assert_eq!(out.exit, 0);
    let cp = out.json();
    assert_eq!(cp["critical_path"], json!([]));
    assert_eq!(
        cp["message"],
        "milestones depend on each other in a cycle (m1, m2, m3): no critical path"
    );
    assert_eq!(
        human_of("critical_path", json!({}), &g),
        "No critical path: milestones depend on each other in a cycle (m1, m2, m3): no critical path\n"
    );
}

#[specforge_test(
    behavior = "pe_query_critical_path",
    verify = "linear chain of 3 milestones returns all 3 as critical path"
)]
fn a_chain_of_three_open_milestones_is_the_path() {
    let g = G::default()
        .n("a", "milestone")
        .n("b", "milestone")
        .n("c", "milestone")
        .edge("c", "b", "depends_on")
        .edge("b", "a", "depends_on");
    let cp = json_of("critical_path", json!({}), &g);
    assert_eq!(path_ids(&cp), ["a", "b", "c"]);
    assert_eq!(cp["path_length"], 3);
}

#[specforge_test(
    behavior = "pe_query_critical_path",
    verify = "parallel chains return the longer one"
)]
fn of_two_chains_the_longer_is_the_path() {
    // m1-m2-m3 (three open) beats n1-n2.
    let cp = json_of("critical_path", json!({}), &schedule());
    assert_eq!(path_ids(&cp), ["m1", "m2", "m3"]);
}

#[specforge_test(
    behavior = "pe_query_critical_path",
    verify = "completed milestones are excluded from critical path"
)]
fn a_completed_milestone_is_off_the_path() {
    // m0 is completed: the chain stops at m1.
    let cp = json_of("critical_path", json!({}), &schedule());
    assert!(!path_ids(&cp).contains(&"m0".to_string()));
    // Completing m1 and m2 leaves the n chain longest.
    let mut g = schedule();
    for node in &mut g.nodes {
        if matches!(node["id"].as_str(), Some("m1" | "m2")) {
            node["fields"]["status"] = json!("completed");
        }
    }
    let cp = json_of("critical_path", json!({}), &g);
    assert_eq!(path_ids(&cp), ["n1", "n2"]);
    assert_eq!(
        (
            cp["earliest_completion"].clone(),
            cp["latest_completion"].clone()
        ),
        (json!("2026-10-15"), json!("2026-10-30"))
    );
}

#[specforge_test(
    behavior = "pe_query_critical_path",
    verify = "milestones without target_date still appear on path"
)]
fn a_milestone_without_a_date_is_on_the_path_without_dates() {
    let cp = json_of("critical_path", json!({}), &schedule());
    let m3 = &cp["critical_path"][2];
    assert_eq!(m3["entity_id"], "m3");
    assert_eq!(
        (m3["target_date"].clone(), m3["slack_days"].clone()),
        (Value::Null, Value::Null)
    );
}

#[specforge_test(
    behavior = "pe_query_critical_path",
    verify = "graph with cycles returns empty path"
)]
fn a_cycle_among_milestones_leaves_no_path() {
    // Even a cycle off the longest chain: E015 fires, so no path is given.
    let g = schedule().edge("n1", "n2", "depends_on");
    let cp = json_of("critical_path", json!({}), &g);
    assert_eq!(
        (cp["critical_path"].clone(), cp["path_length"].clone()),
        (json!([]), json!(0))
    );
    assert!(cp["message"].as_str().unwrap().contains("(n1, n2)"), "{cp}");
}

#[specforge_test(
    behavior = "product_new_query_correctness",
    verify = "critical path is the longest incomplete chain"
)]
fn no_chain_of_open_milestones_is_longer_than_the_critical_path() {
    // A diamond and a tail: e <- {f, g} <- h <- i; j alone; equal branches
    // are taken by id.
    let g = G::default()
        .n("e", "milestone")
        .n("f", "milestone")
        .n("g", "milestone")
        .n("h", "milestone")
        .n("i", "milestone")
        .n("j", "milestone")
        .edge("f", "e", "depends_on")
        .edge("g", "e", "depends_on")
        .edge("h", "f", "depends_on")
        .edge("h", "g", "depends_on")
        .edge("i", "h", "depends_on");
    let cp = json_of("critical_path", json!({}), &g);
    assert_eq!(path_ids(&cp), ["e", "f", "h", "i"]);
    assert_eq!(
        json_of("critical_path", json!({}), &reversed(g)),
        cp,
        "declaration order does not change it"
    );
}
