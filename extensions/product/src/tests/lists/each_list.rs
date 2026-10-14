use super::{catalog, entry_of, ids_of};
use crate::tests::host::*;
use serde_json::json;
use specforge_test::prelude::*;

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
