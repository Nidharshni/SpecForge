//! The nine list commands: the shared filter, sort and page contract, then each list's own filters and entries.

use super::host::*;
use serde_json::{json, Value};
use specforge_test::prelude::*;

// ── lists ─────────────────────────────────────────────────────────────────

/// The nine list commands and the key each payload's entries are under.
const LISTS: &[&str] = &[
    "features",
    "journeys",
    "deliverables",
    "milestones",
    "modules",
    "terms",
    "personas",
    "channels",
    "releases",
];

/// A few entities of every kind, with the fields the lists filter on.
fn catalog() -> G {
    G::default()
        .node(
            "f1",
            "feature",
            json!({"status": "done", "priority": "high", "tags": ["core", "cli"], "problem": "p"}),
        )
        .node(
            "f2",
            "feature",
            json!({"status": "done", "priority": "low", "tags": ["web"]}),
        )
        .node("f3", "feature", json!({"priority": "high"}))
        .node(
            "f4",
            "feature",
            json!({"status": "deferred", "priority": "critical"}),
        )
        .node(
            "j1",
            "journey",
            json!({"persona": "dev", "priority": "high"}),
        )
        .node("j2", "journey", json!({"persona": "ops"}))
        .n("j3", "journey")
        .node(
            "d1",
            "deliverable",
            json!({"artifact_type": "cli", "status": "shipped"}),
        )
        .node("d2", "deliverable", json!({"artifact_type": "web_app"}))
        .node(
            "ms1",
            "milestone",
            json!({"status": "completed", "target_date": "2026-01-01"}),
        )
        .n("ms2", "milestone")
        .node("mod1", "module", json!({"family": "core"}))
        .node("mod2", "module", json!({"family": "experimental"}))
        .node(
            "t1",
            "term",
            json!({"definition": "one", "aliases": ["uno", "eins"]}),
        )
        .node("t2", "term", json!({"definition": "two"}))
        .node(
            "dev",
            "persona",
            json!({"technical_level": "expert", "status": "active"}),
        )
        .node("ops", "persona", json!({"technical_level": "beginner"}))
        .node(
            "cli",
            "channel",
            json!({"interaction_model": "batch", "status": "active"}),
        )
        .node("web", "channel", json!({"interaction_model": "streaming"}))
        .node(
            "r1",
            "release",
            json!({"version": "1.0.0", "status": "released"}),
        )
        .n("r2", "release")
        .edge("j1", "dev", "persona")
        .edge("j2", "ops", "persona")
        .edge("j1", "cli", "channels")
        .edge("j1", "web", "channels")
        .edge("j2", "cli", "channels")
        .edge("j1", "f1", "features")
        .edge("j1", "f2", "features")
        .edge("j3", "f3", "features")
        .edge("d1", "j1", "journeys")
        .edge("d1", "mod1", "modules")
        .edge("d1", "mod2", "modules")
        .edge("d2", "j2", "journeys")
        .edge("ms1", "f1", "features")
        .edge("ms1", "f2", "features")
        .edge("mod1", "f1", "features")
        .edge("mod1", "mod2", "depends_on")
        .edge("r1", "d1", "deliverables")
}

/// The ids `list` answers with `args` over `g`, in order.
fn ids_of(list: &str, args: Value, g: &G) -> Vec<String> {
    let payload = json_of(list, args, g);
    payload[list]
        .as_array()
        .unwrap_or_else(|| panic!("{list}: no {list} array: {payload}"))
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect()
}

/// The entry `id` of `list` over [`catalog`].
fn entry_of(list: &str, id: &str) -> Value {
    json_of(list, json!({}), &catalog())[list]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("{list}: no {id}"))
        .clone()
}

/// What `list` answers when it refuses `args`: its exit code and error.
fn refused(list: &str, args: Value) -> (i64, Value) {
    let out = run_in(&runtime(), list, args, &catalog(), "json");
    let exit = out.exit;
    (exit, out.error())
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "filter by status returns only matching entities"
)]
fn a_list_filtered_by_status_has_only_that_status() {
    let g = catalog();
    assert_eq!(
        ids_of("features", json!({"status": "done"}), &g),
        ["f1", "f2"]
    );
    // A feature without a status is proposed.
    assert_eq!(
        ids_of("features", json!({"status": "proposed"}), &g),
        ["f3"]
    );
    assert_eq!(
        ids_of("milestones", json!({"status": "completed"}), &g),
        ["ms1"]
    );
    // A persona or channel without a status is active.
    assert_eq!(
        ids_of("personas", json!({"status": "active"}), &g),
        ["dev", "ops"]
    );
    assert_eq!(
        ids_of("channels", json!({"status": "active"}), &g),
        ["cli", "web"]
    );
    assert!(ids_of("personas", json!({"status": "deprecated"}), &g).is_empty());
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "filter by priority returns only matching entities"
)]
fn a_list_filtered_by_priority_has_only_that_priority() {
    let g = catalog();
    assert_eq!(
        ids_of("features", json!({"priority": "high"}), &g),
        ["f1", "f3"]
    );
    assert_eq!(ids_of("journeys", json!({"priority": "high"}), &g), ["j1"]);
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "combined status+priority filter uses AND logic"
)]
fn list_filters_combine_with_and() {
    let g = catalog();
    let args = json!({"status": "done", "priority": "high"});
    assert_eq!(ids_of("features", args, &g), ["f1"]);
    let args = json!({"status": "done", "priority": "critical"});
    assert!(ids_of("features", args, &g).is_empty());
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "invalid status filter returns INVALID_INPUT"
)]
fn a_status_outside_the_kinds_enum_is_invalid_input() {
    // `draft` is a deliverable's status, not a feature's.
    let (exit, error) = refused("features", json!({"status": "draft"}));
    assert_eq!(exit, 2);
    assert_eq!(error["code"], "INVALID_INPUT");
    assert_eq!(
        error["message"],
        "status must be one of proposed, accepted, in_progress, done, deferred, deprecated, got 'draft'"
    );
    // The same value is one a deliverable's status takes.
    assert_eq!(
        ids_of("deliverables", json!({"status": "draft"}), &catalog()),
        ["d2"]
    );
}

#[specforge_test(
    behavior = "surface_error_handling",
    verify = "invalid filter value returns INVALID_INPUT code"
)]
fn every_closed_filter_refuses_a_value_outside_its_enum() {
    for (list, arg) in [
        ("features", "status"),
        ("features", "priority"),
        ("journeys", "priority"),
        ("deliverables", "status"),
        ("deliverables", "artifact_type"),
        ("milestones", "status"),
        ("milestones", "priority"),
        ("personas", "status"),
        ("personas", "technical_level"),
        ("channels", "status"),
        ("channels", "interaction_model"),
        ("releases", "status"),
    ] {
        let (exit, error) = refused(list, json!({ arg: "nonesuch" }));
        assert_eq!(exit, 2, "{list} --{arg}");
        assert_eq!(error["code"], "INVALID_INPUT", "{list} --{arg}");
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .starts_with(&format!("{arg} must be one of ")),
            "{list} --{arg}: {error}"
        );
    }
    // An open filter matches what it is given: a non-standard family is a
    // family (I062 says so, as an info).
    assert_eq!(
        ids_of("modules", json!({"family": "experimental"}), &catalog()),
        ["mod2"]
    );
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "sort by priority with tie-break by ID is deterministic"
)]
fn a_list_sorted_by_priority_ties_by_id() {
    let g = catalog();
    let asc = json!({"sort_by": "priority"});
    // Priority's order, most important first; ties by id.
    assert_eq!(
        ids_of("features", asc.clone(), &g),
        ["f4", "f1", "f3", "f2"]
    );
    assert_eq!(ids_of("features", asc, &g), ["f4", "f1", "f3", "f2"]);
    let desc = json!({"sort_by": "priority", "sort_order": "desc"});
    assert_eq!(ids_of("features", desc, &g), ["f2", "f1", "f3", "f4"]);
    // A text field sorts by its text.
    let by_version = json!({"sort_by": "version", "sort_order": "desc"});
    assert_eq!(ids_of("releases", by_version, &g), ["r1", "r2"]);
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "invalid sort_by field returns INVALID_INPUT"
)]
fn a_sort_field_the_kind_lacks_is_invalid_input() {
    let (exit, error) = refused("terms", json!({"sort_by": "priority"}));
    assert_eq!(exit, 2);
    assert_eq!(error["code"], "INVALID_INPUT");
    assert_eq!(error["message"], "sort_by: a term has no field 'priority'");
    let (exit, error) = refused("features", json!({"sort_order": "sideways"}));
    assert_eq!((exit, error["code"].clone()), (2, json!("INVALID_INPUT")));
}

/// A graph of `n` features, `f00` to `f<n-1>`.
fn features(n: usize) -> G {
    (0..n).fold(G::default(), |g, i| g.n(&format!("f{i:02}"), "feature"))
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "limit=0 is clamped to 1"
)]
fn a_limit_of_zero_is_one() {
    let page = json_of("features", json!({"limit": 0}), &features(3));
    assert_eq!(page["limit"], 1);
    assert_eq!(page["features"].as_array().unwrap().len(), 1);
    assert_eq!(page["has_more"], true);
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "limit=5000 is clamped to 1000"
)]
fn a_limit_past_a_thousand_is_a_thousand() {
    let page = json_of("features", json!({"limit": 5000}), &features(1001));
    assert_eq!(page["limit"], 1000);
    assert_eq!(page["features"].as_array().unwrap().len(), 1000);
    assert_eq!(
        (page["total"].clone(), page["has_more"].clone()),
        (json!(1001), json!(true))
    );
    // Unset, a page is 100.
    let page = json_of("features", json!({}), &features(101));
    assert_eq!(page["limit"], 100);
    assert_eq!(page["features"].as_array().unwrap().len(), 100);
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "offset beyond total returns empty list"
)]
fn an_offset_past_the_end_is_an_empty_page() {
    let page = json_of("features", json!({"offset": 10}), &features(3));
    assert_eq!(
        page,
        json!({"features": [], "total": 3, "offset": 10, "limit": 100, "has_more": false})
    );
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "empty graph returns total=0 and has_more=false"
)]
fn every_list_over_an_empty_graph_is_empty() {
    let runtime = runtime();
    for list in LISTS {
        let page = run_in(&runtime, list, json!({}), &G::default(), "json").json();
        assert_eq!(
            page,
            json!({ *list: [], "total": 0, "offset": 0, "limit": 100, "has_more": false }),
            "{list}"
        );
    }
}

/// Every list's pages over [`catalog`] for a range of offsets and limits.
fn every_page() -> Vec<(&'static str, usize, usize, Value)> {
    let runtime = runtime();
    let g = catalog();
    let mut pages = Vec::new();
    for list in LISTS {
        for offset in 0..5 {
            for limit in 1..5 {
                let args = json!({"offset": offset, "limit": limit});
                let page = run_in(&runtime, list, args, &g, "json").json();
                pages.push((*list, offset, limit, page));
            }
        }
    }
    pages
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "for all list commands: entities.length <= limit"
)]
fn no_page_is_longer_than_its_limit() {
    for (list, _, limit, page) in every_page() {
        assert!(
            page[list].as_array().unwrap().len() <= limit,
            "{list}: {page}"
        );
        assert_eq!(page["limit"], limit);
    }
}

#[specforge_test(
    behavior = "surface_list_command_contract",
    verify = "for all list commands: has_more == (offset + entities.length < total)"
)]
fn a_page_has_more_iff_entries_remain_after_it() {
    for (list, offset, _, page) in every_page() {
        let shown = page[list].as_array().unwrap().len();
        let total = page["total"].as_u64().unwrap() as usize;
        assert_eq!(page["has_more"], offset + shown < total, "{list}: {page}");
        assert_eq!(page["offset"], offset);
    }
}

#[specforge_test(
    behavior = "product_pagination_sort_stability",
    verify = "page 0 + page 1 concatenation equals unpaginated result"
)]
fn two_pages_are_the_whole_list() {
    let g = features(7);
    let args = |offset: usize| json!({"offset": offset, "limit": 4, "sort_by": "title"});
    let mut paged = ids_of("features", args(0), &g);
    paged.extend(ids_of("features", args(4), &g));
    assert_eq!(paged, ids_of("features", json!({"sort_by": "title"}), &g));
}

#[specforge_test(
    behavior = "product_pagination_sort_stability",
    verify = "entities with same priority sorted alphabetically by ID"
)]
fn entities_of_one_priority_are_by_id() {
    let g = ["c", "a", "b"].iter().fold(G::default(), |g, id| {
        g.node(id, "feature", json!({"priority": "medium"}))
    });
    for order in ["asc", "desc"] {
        let args = json!({"sort_by": "priority", "sort_order": order});
        assert_eq!(ids_of("features", args, &g), ["a", "b", "c"], "{order}");
    }
}

#[specforge_test(
    behavior = "product_pagination_sort_stability",
    verify = "union of all pages equals full result set with no duplicates"
)]
fn the_pages_of_a_list_are_its_entries_once_each() {
    let g = catalog();
    for list in LISTS {
        let all = ids_of(list, json!({}), &g);
        for limit in 1..4 {
            let mut seen: Vec<String> = Vec::new();
            let mut offset = 0;
            loop {
                let page = json_of(list, json!({"offset": offset, "limit": limit}), &g);
                let ids: Vec<String> = page[*list]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| e["id"].as_str().unwrap().to_string())
                    .collect();
                offset += ids.len();
                seen.extend(ids);
                if page["has_more"] == false {
                    break;
                }
            }
            assert_eq!(seen, all, "{list} by {limit}");
        }
    }
}

mod each_list;

/// Pinned by plan 01: a lifecycle kind's list counts an entity without a
/// status as the kind's absent status, and `bulk-status` still reports it
/// as `(none)`; the declared default will say the same thing once.
#[test]
fn lists_count_an_absent_status_as_its_default() {
    let g = G::default()
        .n("f", "feature")
        .n("d", "deliverable")
        .n("m", "milestone")
        .n("p", "persona")
        .n("c", "channel")
        .n("r", "release");
    for (list, absent, id) in [
        ("features", "proposed", "f"),
        ("deliverables", "draft", "d"),
        ("milestones", "planned", "m"),
        ("personas", "active", "p"),
        ("channels", "active", "c"),
        ("releases", "planned", "r"),
    ] {
        assert_eq!(ids_of(list, json!({"status": absent}), &g), [id], "{list}");
        // It sorts with them.
        assert_eq!(
            ids_of(list, json!({"sort_by": "status"}), &g),
            [id],
            "{list}"
        );
    }
    let report = json_of("bulk_status", json!({}), &g);
    for kind in report["kinds"].as_array().unwrap() {
        assert_eq!(
            kind["by_status"],
            json!([{"status": "(none)", "count": 1}]),
            "{kind}"
        );
    }
}
