//! The dependency commands: feature ordering, the critical path, module depth and coupling, deliverable dependents.

use super::host::*;
use serde_json::json;
use specforge_test::prelude::*;

// ── dependency graphs ─────────────────────────────────────────────────────

/// Features over `depends_on`, by level:
/// - 0: base (low), solo (medium), loner (no priority)
/// - 1, on base: m_crit (critical), m_high (high), m_none (no priority),
///   m_med (medium), m_low (low)
/// - 2: top, on m_high and m_none
fn layered() -> G {
    let mut g = G::default()
        .node("base", "feature", json!({"priority": "low"}))
        .node("solo", "feature", json!({"priority": "medium"}))
        .n("loner", "feature")
        .node("m_crit", "feature", json!({"priority": "critical"}))
        .node("m_high", "feature", json!({"priority": "high"}))
        .n("m_none", "feature")
        .node("m_med", "feature", json!({"priority": "medium"}))
        .node("m_low", "feature", json!({"priority": "low"}))
        .n("top", "feature")
        .edge("top", "m_high", "depends_on")
        .edge("top", "m_none", "depends_on");
    for mid in ["m_crit", "m_high", "m_none", "m_med", "m_low"] {
        g = g.edge(mid, "base", "depends_on");
    }
    g
}

/// The order `layered` sorts to.
const LAYERED: [&str; 9] = [
    "loner", "solo", "base", "m_crit", "m_high", "m_med", "m_none", "m_low", "top",
];

/// [`layered`] plus x <-> y and z on x.
fn layered_with_a_cycle() -> G {
    layered()
        .n("x", "feature")
        .n("y", "feature")
        .n("z", "feature")
        .edge("x", "y", "depends_on")
        .edge("y", "x", "depends_on")
        .edge("z", "x", "depends_on")
}

fn reversed(mut g: G) -> G {
    g.edges.reverse();
    g.nodes.reverse();
    g
}

#[specforge_test(
    behavior = "surface_feature_ordering",
    verify = "feature-ordering returns FeatureOrderingPayload JSON"
)]
fn feature_ordering_answers_its_payload() {
    let fo = json_of("feature_ordering", json!({}), &layered());
    assert_eq!(
        fo,
        json!({"sorted_features": LAYERED, "has_cycles": false, "cycle_members": []})
    );
    let human = human_of("feature_ordering", json!({}), &layered_with_a_cycle());
    assert!(human.starts_with("  1. loner\n  2. solo\n"), "{human}");
    assert!(human.contains(". x  (cycle)\n"), "{human}");
    assert!(human.contains(". z\n"), "{human}");
    assert!(human.ends_with("Dependency cycle: x, y\n"), "{human}");
}

#[specforge_test(
    behavior = "surface_feature_ordering",
    verify = "cycles present in output does not cause exit code 1"
)]
fn a_feature_cycle_is_reported_with_exit_zero() {
    let out = run_in(
        &runtime(),
        "feature_ordering",
        json!({}),
        &layered_with_a_cycle(),
        "json",
    );
    assert_eq!(out.exit, 0, "{}", out.stderr);
    assert_eq!(out.json()["has_cycles"], true);
}

#[specforge_test(
    behavior = "surface_feature_ordering",
    verify = "empty feature graph returns empty sorted list"
)]
fn feature_ordering_of_no_features_is_empty() {
    let fo = json_of("feature_ordering", json!({}), &G::default());
    assert_eq!(fo["sorted_features"], json!([]));
    assert_eq!(
        human_of("feature_ordering", json!({}), &G::default()),
        "No features.\n"
    );
}

#[specforge_test(
    behavior = "pe_query_feature_ordering",
    verify = "acyclic feature graph returns topological sort"
)]
fn an_acyclic_feature_graph_sorts_dependencies_first() {
    let fo = json_of("feature_ordering", json!({}), &layered());
    let order: Vec<&str> = fo["sorted_features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    assert_eq!(order, LAYERED);
    let at = |id: &str| order.iter().position(|f| *f == id).unwrap();
    for mid in ["m_crit", "m_high", "m_none", "m_med", "m_low"] {
        assert!(at("base") < at(mid), "{mid}");
    }
    assert!(at("m_high") < at("top") && at("m_none") < at("top"));
}

#[specforge_test(
    behavior = "pe_query_feature_ordering",
    verify = "features at same level sorted by priority descending"
)]
fn a_levels_features_are_most_important_first() {
    let fo = json_of("feature_ordering", json!({}), &layered());
    assert_eq!(
        fo["sorted_features"].as_array().unwrap()[3..8],
        json!(["m_crit", "m_high", "m_med", "m_none", "m_low"])
            .as_array()
            .unwrap()[..]
    );
}

#[specforge_test(
    behavior = "pe_query_feature_ordering",
    verify = "features without priority default to medium"
)]
fn a_feature_without_a_priority_ranks_as_medium() {
    // m_none sits with m_med, by id; loner (none) sorts before base (low)
    // and beside solo (medium), by id.
    let fo = json_of("feature_ordering", json!({}), &layered());
    let order = fo["sorted_features"].as_array().unwrap();
    assert_eq!(
        order[..3],
        json!(["loner", "solo", "base"]).as_array().unwrap()[..]
    );
    assert_eq!(
        order[5..7],
        json!(["m_med", "m_none"]).as_array().unwrap()[..]
    );
}

#[specforge_test(
    behavior = "pe_query_feature_ordering",
    verify = "cyclic feature graph returns has_cycles=true with cycle members"
)]
fn a_feature_cycle_names_its_members_once() {
    // z depends on the cycle without being on it: it is listed last, not
    // as a member.
    let g = layered_with_a_cycle().edge("x", "y", "depends_on");
    let fo = json_of("feature_ordering", json!({}), &g);
    assert_eq!(fo["has_cycles"], true);
    assert_eq!(fo["cycle_members"], json!(["x", "y"]));
    let order = fo["sorted_features"].as_array().unwrap();
    assert_eq!(order.len(), 12, "every feature once: {fo}");
    assert_eq!(order[9..], json!(["x", "y", "z"]).as_array().unwrap()[..]);
}

#[specforge_test(
    behavior = "pe_query_feature_ordering",
    verify = "features with no dependencies return stable ordering"
)]
fn independent_features_of_one_priority_sort_by_id() {
    let g = G::default()
        .n("delta", "feature")
        .n("alpha", "feature")
        .n("charlie", "feature")
        .n("bravo", "feature");
    let fo = json_of("feature_ordering", json!({}), &g);
    assert_eq!(
        fo["sorted_features"],
        json!(["alpha", "bravo", "charlie", "delta"])
    );
}

#[specforge_test(
    behavior = "pe_query_feature_ordering",
    verify = "empty feature graph returns empty sorted list and has_cycles=false"
)]
fn an_empty_feature_graph_orders_nothing_and_has_no_cycle() {
    // Other kinds' depends_on are not features'.
    let g = G::default()
        .n("a", "module")
        .n("b", "module")
        .edge("a", "b", "depends_on")
        .edge("b", "a", "depends_on");
    let fo = json_of("feature_ordering", json!({}), &g);
    assert_eq!(
        fo,
        json!({"sorted_features": [], "has_cycles": false, "cycle_members": []})
    );
}

#[specforge_test(
    behavior = "pe_query_feature_ordering",
    verify = "feature ordering is deterministic across repeated queries"
)]
fn feature_ordering_is_the_same_every_time_and_in_any_order() {
    let first = json_of("feature_ordering", json!({}), &layered_with_a_cycle());
    assert_eq!(
        json_of("feature_ordering", json!({}), &layered_with_a_cycle()),
        first
    );
    assert_eq!(
        json_of(
            "feature_ordering",
            json!({}),
            &reversed(layered_with_a_cycle())
        ),
        first
    );
}

#[specforge_test(
    behavior = "product_query_correctness",
    verify = "feature ordering produces valid topological sort"
)]
fn every_feature_comes_after_the_features_it_depends_on() {
    for g in [layered(), plan(), shipping()] {
        let fo = json_of("feature_ordering", json!({}), &g);
        let order: Vec<&str> = fo["sorted_features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_str().unwrap())
            .collect();
        let at = |id: &str| order.iter().position(|f| *f == id);
        for edge in g.edges.iter().filter(|e| e["label"] == "depends_on") {
            let (from, to) = (
                edge["source"].as_str().unwrap(),
                edge["target"].as_str().unwrap(),
            );
            if let (Some(dependent), Some(dependency)) = (at(from), at(to)) {
                assert!(dependency < dependent, "{to} before {from}: {order:?}");
            }
        }
    }
}

mod critical_path;
mod deliverable_dependents;
mod modules;
