//! The exploration (plan 04, ADR 0015 "Prompt read views"): where to start
//! reading a project's graph.

use specforge_ops::read::explore::{Exploration, ExplorationRequest, explore};
use specforge_registry::RegistryBuild;
use specforge_test::prelude::*;

use crate::view_support::{Assembled, R1_SOURCE, Viewed, registries};

/// `R1_SOURCE` with `feature` and `behavior` declared, so the project knows
/// both kinds.
fn r1() -> Assembled {
    Assembled::over(registries(&["behavior"], &["feature"])).source(R1_SOURCE)
}

/// The exploration `request` asks of `project`.
fn explored(project: &Assembled, request: ExplorationRequest) -> Exploration {
    explore(&project.view(), &request).unwrap()
}

/// A project of `node` entities, each referencing the ones its edges name:
/// `edges` is `(from, [to, ...])`; every entity in `alone` references nothing.
fn nodes(edges: &[(&str, &[&str])], alone: &[&str]) -> Assembled {
    let mut source = String::new();
    for (from, to) in edges {
        source.push_str(&format!(
            "node {from} \"{from}\" {{\n  to [{}]\n}}\n",
            to.join(", ")
        ));
    }
    for id in alone {
        source.push_str(&format!("node {id} \"{id}\" {{\n}}\n"));
    }
    Assembled::over(RegistryBuild::default()).source(&source)
}

#[specforge_test(
    behavior = "explore_the_graph",
    verify = "the exploration selects the entities entity_id reaches within depth, of kind when given, and every list is about that selection"
)]
fn every_list_is_about_the_selection() {
    let project = r1();

    let features = explored(
        &project,
        ExplorationRequest {
            kind: Some("feature"),
            ..Default::default()
        },
    );
    assert_eq!(features.selected, ["alone", "hub", "selfish"]);
    assert_eq!(features.starting_points, ["hub"]);
    assert_eq!(features.most_connected, ["hub"]);
    assert_eq!(features.unconnected, ["alone", "selfish"]);

    let around_linked = explored(
        &project,
        ExplorationRequest {
            entity_id: Some("linked"),
            depth: Some(1),
            ..Default::default()
        },
    );
    assert_eq!(around_linked.selected, ["hub", "linked"]);
    assert_eq!(around_linked.unconnected, Vec::<String>::new());
    assert_eq!(around_linked.starting_points, ["linked", "hub"]);

    let whole = explored(&project, ExplorationRequest::default());
    assert_eq!(whole.selected.len(), 6);
    assert_eq!(
        whole.unconnected,
        ["alone", "dangling", "lonely", "selfish"]
    );
    assert_eq!(whole.most_connected, ["hub", "linked"]);
}

#[specforge_test(
    behavior = "explore_the_graph",
    verify = "starting points are the selected connected entities that lead most, ties by id, at most five"
)]
fn starting_points_lead_most() {
    let project = nodes(
        &[
            ("a", &["b", "c"]),
            ("d", &["c"]),
            ("e", &["f"]),
            ("g", &["h"]),
            ("i", &["j"]),
        ],
        &["b", "c", "f", "h", "j", "k"],
    );
    let exploration = explored(&project, ExplorationRequest::default());
    // Leads 2, 1, 1, 1, 1 (then the sinks at -1 and below): the cap cuts.
    assert_eq!(exploration.starting_points, ["a", "d", "e", "g", "i"]);
    assert!(!exploration.starting_points.contains(&"k".to_string()));
    assert_eq!(exploration.unconnected, ["k"]);
}

#[specforge_test(
    behavior = "explore_the_graph",
    verify = "the most connected are the selected connected entities with the most edges to other entities, ties by id, at most ten"
)]
fn most_connected_count_edges_to_others() {
    let mut source = String::new();
    for n in 1..12 {
        source.push_str(&format!(
            "node n{n:02} \"n\" {{\n  to [n{:02}]\n}}\n",
            n + 1
        ));
    }
    source.push_str("node n12 \"n12\" {\n}\nnode selfish \"selfish\" {\n  to [selfish]\n}\n");
    let project = Assembled::over(RegistryBuild::default()).source(&source);

    let exploration = explored(&project, ExplorationRequest::default());
    // n02..n11 have two edges; n01 and n12 one: the cap cuts them. An entity
    // referencing only itself has none.
    let expected: Vec<String> = (2..=11).map(|n| format!("n{n:02}")).collect();
    assert_eq!(exploration.most_connected, expected);
    assert!(!exploration.most_connected.contains(&"selfish".to_string()));
}

#[specforge_test(
    behavior = "explore_the_graph",
    verify = "unconnected lists the selected entities no edge links to another entity"
)]
fn unconnected_lists_the_selection_s_unconnected() {
    let project = r1();
    let behaviors = explored(
        &project,
        ExplorationRequest {
            kind: Some("behavior"),
            ..Default::default()
        },
    );
    assert_eq!(behaviors.selected, ["dangling", "linked", "lonely"]);
    assert_eq!(behaviors.unconnected, ["dangling", "lonely"]);
}

#[specforge_test(
    behavior = "explore_the_graph",
    verify = "relationship paths run from entity_id to each selected entity it reaches, nearest first, with their edge labels"
)]
fn paths_run_from_the_entity() {
    let project = Assembled::over(RegistryBuild::default()).source("node a \"a\" {\n  to [b]\n}\nnode b \"b\" {\n  next [c]\n}\nnode c \"c\" {\n  last [d]\n}\nnode d \"d\" {\n}\n");
    let exploration = explored(
        &project,
        ExplorationRequest {
            entity_id: Some("a"),
            ..Default::default()
        },
    );
    let paths: Vec<(&str, Vec<&str>)> = exploration
        .paths
        .iter()
        .map(|p| (p.to.as_str(), p.labels.iter().map(String::as_str).collect()))
        .collect();
    assert_eq!(
        paths,
        [
            ("b", vec!["to"]),
            ("c", vec!["to", "next"]),
            ("d", vec!["to", "next", "last"]),
        ]
    );
    let json = serde_json::to_value(exploration.document()).unwrap();
    assert_eq!(
        json["relationship_paths"][1],
        serde_json::json!({"from_entity": "a", "to_entity": "c", "edge_types": ["to", "next"], "path_length": 2})
    );
}

#[specforge_test(
    behavior = "explore_the_graph",
    verify = "an unknown kind selects nothing and is an I020 notice naming the closest kind"
)]
fn an_unknown_kind_is_a_notice() {
    let project = r1();
    let exploration = explored(
        &project,
        ExplorationRequest {
            kind: Some("featur"),
            ..Default::default()
        },
    );
    assert!(exploration.selected.is_empty());
    assert_eq!(exploration.notices.len(), 1);
    assert_eq!(exploration.notices[0].code, "I020");
    assert_eq!(
        exploration.notices[0].suggestion.as_deref(),
        Some("did you mean 'feature'?")
    );
    assert_eq!(
        serde_json::to_value(exploration.document()).unwrap()["notices"][0]["code"],
        "I020"
    );
}
