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

// ── each list ─────────────────────────────────────────────────────────────

#[specforge_test(
    behavior = "surface_list_features",
    verify = "list features returns paginated FeatureListResult"
)]
fn the_features_list_is_a_feature_list_result() {
    let page = json_of("features", json!({"limit": 2}), &catalog());
    assert_eq!(
        page,
        json!({"features": [
            {"id": "f1", "title": "f1", "status": "done", "priority": "high", "problem": "p",
                "tags": ["core", "cli"]},
            {"id": "f2", "title": "f2", "status": "done", "priority": "low", "tags": ["web"]},
        ], "total": 4, "offset": 0, "limit": 2, "has_more": true})
    );
}

#[specforge_test(
    behavior = "surface_list_features",
    verify = "priority filter reduces result set"
)]
fn the_features_list_filters_by_priority() {
    assert_eq!(
        ids_of("features", json!({"priority": "critical"}), &catalog()),
        ["f4"]
    );
}

#[specforge_test(
    behavior = "surface_list_features",
    verify = "tags filter intersects correctly"
)]
fn the_features_list_keeps_features_sharing_a_tag() {
    let g = catalog();
    assert_eq!(ids_of("features", json!({"tags": "cli"}), &g), ["f1"]);
    assert_eq!(
        ids_of("features", json!({"tags": "web, core"}), &g),
        ["f1", "f2"]
    );
    assert!(ids_of("features", json!({"tags": "mobile"}), &g).is_empty());
    // An empty tag list filters nothing.
    assert_eq!(ids_of("features", json!({"tags": ""}), &g).len(), 4);
}

#[specforge_test(
    behavior = "surface_list_features",
    verify = "empty project returns total=0 and empty list"
)]
fn the_features_list_of_no_features_is_empty() {
    let page = json_of("features", json!({}), &G::default().n("j1", "journey"));
    assert_eq!(
        (page["total"].clone(), page["features"].clone()),
        (json!(0), json!([]))
    );
}

#[specforge_test(
    behavior = "surface_list_features",
    verify = "human format is a table with a header row"
)]
fn the_features_list_is_a_table_for_people() {
    let out = human_of("features", json!({"limit": 3}), &catalog());
    assert_eq!(
        out,
        "id  title  status  priority\n\
         f1  f1     done    high\n\
         f2  f2     done    low\n\
         f3  f3     -       high\n\
         3 of 4 features; --offset 3 for more\n"
    );
}

#[specforge_test(
    behavior = "surface_list_journeys",
    verify = "list journeys returns paginated JourneyListResult"
)]
fn the_journeys_list_is_a_journey_list_result() {
    let page = json_of("journeys", json!({}), &catalog());
    assert_eq!(page["total"], 3);
    assert_eq!(
        page["journeys"][0],
        json!({"id": "j1", "title": "j1", "persona": "dev", "channel_count": 2,
            "feature_count": 2, "priority": "high"})
    );
}

#[specforge_test(
    behavior = "surface_list_journeys",
    verify = "persona filter reduces result set to matching journeys"
)]
fn the_journeys_list_filters_by_persona() {
    let g = catalog();
    assert_eq!(ids_of("journeys", json!({"persona": "ops"}), &g), ["j2"]);
    assert!(ids_of("journeys", json!({"persona": "nobody"}), &g).is_empty());
}

#[specforge_test(
    behavior = "surface_list_journeys",
    verify = "channel_count and feature_count are accurate per entry"
)]
fn a_journey_entry_counts_its_channels_and_features() {
    let j2 = entry_of("journeys", "j2");
    assert_eq!(
        (j2["channel_count"].clone(), j2["feature_count"].clone()),
        (json!(1), json!(0))
    );
    let j3 = entry_of("journeys", "j3");
    assert_eq!(
        (j3["channel_count"].clone(), j3["feature_count"].clone()),
        (json!(0), json!(1))
    );
}

#[specforge_test(
    behavior = "surface_list_deliverables",
    verify = "list deliverables returns paginated DeliverableListResult"
)]
fn the_deliverables_list_is_a_deliverable_list_result() {
    let page = json_of("deliverables", json!({}), &catalog());
    assert_eq!(
        page,
        json!({"deliverables": [
            {"id": "d1", "title": "d1", "artifact_type": "cli", "status": "shipped",
                "journey_count": 1, "module_count": 2},
            {"id": "d2", "title": "d2", "artifact_type": "web_app", "journey_count": 1,
                "module_count": 0},
        ], "total": 2, "offset": 0, "limit": 100, "has_more": false})
    );
}

#[specforge_test(
    behavior = "surface_list_deliverables",
    verify = "artifact-type filter reduces result set"
)]
fn the_deliverables_list_filters_by_artifact_type() {
    let g = catalog();
    assert_eq!(
        ids_of("deliverables", json!({"artifact_type": "web_app"}), &g),
        ["d2"]
    );
    // A deliverable without a status is a draft.
    assert_eq!(
        ids_of("deliverables", json!({"status": "draft"}), &g),
        ["d2"]
    );
}

#[specforge_test(
    behavior = "surface_list_deliverables",
    verify = "journey_count and module_count are accurate per entry"
)]
fn a_deliverable_entry_counts_its_journeys_and_modules() {
    let d1 = entry_of("deliverables", "d1");
    assert_eq!(
        (d1["journey_count"].clone(), d1["module_count"].clone()),
        (json!(1), json!(2))
    );
}

#[specforge_test(
    behavior = "surface_list_milestones",
    verify = "list milestones returns paginated MilestoneListResult"
)]
fn the_milestones_list_is_a_milestone_list_result() {
    let page = json_of("milestones", json!({}), &catalog());
    assert_eq!(
        page["milestones"],
        json!([
            {"id": "ms1", "title": "ms1", "status": "completed", "target_date": "2026-01-01",
                "feature_count": 2},
            {"id": "ms2", "title": "ms2", "feature_count": 0},
        ])
    );
    assert_eq!(
        (page["total"].clone(), page["has_more"].clone()),
        (json!(2), json!(false))
    );
}

#[specforge_test(
    behavior = "surface_list_milestones",
    verify = "status filter reduces result set"
)]
fn the_milestones_list_filters_by_status() {
    let g = catalog();
    // A milestone without a status is planned.
    assert_eq!(
        ids_of("milestones", json!({"status": "planned"}), &g),
        ["ms2"]
    );
    assert!(ids_of("milestones", json!({"status": "blocked"}), &g).is_empty());
}

#[specforge_test(
    behavior = "surface_list_milestones",
    verify = "feature_count is accurate per entry"
)]
fn a_milestone_entry_counts_its_features() {
    assert_eq!(entry_of("milestones", "ms1")["feature_count"], 2);
    assert_eq!(entry_of("milestones", "ms2")["feature_count"], 0);
}

#[specforge_test(
    behavior = "surface_list_modules",
    verify = "list modules returns paginated ModuleListResult"
)]
fn the_modules_list_is_a_module_list_result() {
    let page = json_of("modules", json!({}), &catalog());
    assert_eq!(
        page,
        json!({"modules": [
            {"id": "mod1", "title": "mod1", "family": "core", "feature_count": 1,
                "depends_on": ["mod2"]},
            {"id": "mod2", "title": "mod2", "family": "experimental", "feature_count": 0,
                "depends_on": []},
        ], "total": 2, "offset": 0, "limit": 100, "has_more": false})
    );
}

#[specforge_test(
    behavior = "surface_list_modules",
    verify = "family filter reduces result set"
)]
fn the_modules_list_filters_by_family() {
    assert_eq!(
        ids_of("modules", json!({"family": "core"}), &catalog()),
        ["mod1"]
    );
}

#[specforge_test(
    behavior = "surface_list_modules",
    verify = "feature_count and depends_on are accurate per entry"
)]
fn a_module_entry_counts_its_features_and_lists_its_dependencies() {
    let mod1 = entry_of("modules", "mod1");
    assert_eq!(
        (mod1["feature_count"].clone(), mod1["depends_on"].clone()),
        (json!(1), json!(["mod2"]))
    );
}

#[specforge_test(
    behavior = "surface_list_terms",
    verify = "list terms returns paginated TermListResult"
)]
fn the_terms_list_is_a_term_list_result() {
    let page = json_of("terms", json!({}), &catalog());
    assert_eq!(
        page,
        json!({"terms": [
            {"id": "t1", "title": "t1", "definition": "one", "alias_count": 2},
            {"id": "t2", "title": "t2", "definition": "two", "alias_count": 0},
        ], "total": 2, "offset": 0, "limit": 100, "has_more": false})
    );
}

#[specforge_test(
    behavior = "surface_list_terms",
    verify = "alias_count is accurate per entry"
)]
fn a_term_entry_counts_its_aliases() {
    assert_eq!(entry_of("terms", "t1")["alias_count"], 2);
    assert_eq!(entry_of("terms", "t2")["alias_count"], 0);
}

#[specforge_test(
    behavior = "surface_list_personas",
    verify = "list personas returns paginated PersonaListResult"
)]
fn the_personas_list_is_a_persona_list_result() {
    let page = json_of("personas", json!({}), &catalog());
    assert_eq!(
        page["personas"],
        json!([
            {"id": "dev", "title": "dev", "technical_level": "expert", "status": "active",
                "journey_count": 1},
            {"id": "ops", "title": "ops", "technical_level": "beginner", "journey_count": 1},
        ])
    );
    assert_eq!(page["total"], 2);
}

#[specforge_test(
    behavior = "surface_list_personas",
    verify = "technical-level filter reduces result set"
)]
fn the_personas_list_filters_by_technical_level() {
    assert_eq!(
        ids_of(
            "personas",
            json!({"technical_level": "beginner"}),
            &catalog()
        ),
        ["ops"]
    );
}

#[specforge_test(
    behavior = "surface_list_personas",
    verify = "journey_count is accurate per entry"
)]
fn a_persona_entry_counts_the_journeys_targeting_it() {
    let g = catalog().n("j4", "journey").edge("j4", "dev", "persona");
    let page = json_of("personas", json!({}), &g);
    assert_eq!(page["personas"][0]["journey_count"], 2);
    assert_eq!(page["personas"][1]["journey_count"], 1);
}

#[specforge_test(
    behavior = "surface_list_channels",
    verify = "list channels returns paginated ChannelListResult"
)]
fn the_channels_list_is_a_channel_list_result() {
    let page = json_of("channels", json!({}), &catalog());
    assert_eq!(
        page["channels"],
        json!([
            {"id": "cli", "title": "cli", "interaction_model": "batch", "status": "active",
                "journey_count": 2},
            {"id": "web", "title": "web", "interaction_model": "streaming", "journey_count": 1},
        ])
    );
    assert_eq!(page["total"], 2);
}

#[specforge_test(
    behavior = "surface_list_channels",
    verify = "interaction-model filter reduces result set"
)]
fn the_channels_list_filters_by_interaction_model() {
    assert_eq!(
        ids_of(
            "channels",
            json!({"interaction_model": "streaming"}),
            &catalog()
        ),
        ["web"]
    );
}

#[specforge_test(
    behavior = "surface_list_channels",
    verify = "journey_count is accurate per entry"
)]
fn a_channel_entry_counts_the_journeys_using_it() {
    assert_eq!(entry_of("channels", "cli")["journey_count"], 2);
    assert_eq!(entry_of("channels", "web")["journey_count"], 1);
}

#[specforge_test(
    behavior = "surface_list_releases",
    verify = "list-releases returns all releases with default pagination"
)]
fn the_releases_list_has_every_release_on_one_default_page() {
    let page = json_of("releases", json!({}), &catalog());
    assert_eq!(
        page,
        json!({"releases": [
            {"id": "r1", "title": "r1", "version": "1.0.0", "status": "released",
                "deliverable_count": 1},
            {"id": "r2", "title": "r2", "deliverable_count": 0},
        ], "total": 2, "offset": 0, "limit": 100, "has_more": false})
    );
}

#[specforge_test(
    behavior = "surface_list_releases",
    verify = "list-releases --status=released filters correctly"
)]
fn the_releases_list_filters_by_status() {
    let g = catalog();
    assert_eq!(
        ids_of("releases", json!({"status": "released"}), &g),
        ["r1"]
    );
    // A release without a status is planned.
    assert_eq!(ids_of("releases", json!({"status": "planned"}), &g), ["r2"]);
}

#[specforge_test(
    behavior = "surface_list_releases",
    verify = "list-releases --format=json returns valid JSON"
)]
fn the_releases_list_is_json_when_asked() {
    let out = run_in(&runtime(), "releases", json!({}), &catalog(), "json");
    assert!(out.json()["releases"].is_array());
    let human = run_in(&runtime(), "releases", json!({}), &catalog(), "human");
    assert!(
        human.stdout.starts_with("id  title  version  status"),
        "{}",
        human.stdout
    );
}

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
