//! The review (plan 04, ADR 0015 "Prompt read views"): the coverage gaps of
//! an entity's neighbourhood, or of the whole project.

use specforge_ops::read::explore::{ExplorationRequest, explore};
use specforge_ops::read::review::{ReviewFinding, ReviewGap, ReviewRequest, review};
use specforge_test::prelude::*;

use crate::view_support::{Assembled, Viewed, registries};

/// `alpha <- beta -> delta`, and `gamma_orphan`, an invariant with no edge
/// and no obligation. Behaviors and invariants owe obligations; `beta` is a
/// feature, whose kind is unknown here, so it never counts toward coverage.
fn project() -> Assembled {
    Assembled::over(registries(&["behavior", "invariant"], &[])).source(
        "behavior alpha \"Alpha\" {\n  verify unit \"test alpha\"\n}\n\n\
         feature beta \"Beta\" {\n  behaviors [alpha, delta]\n}\n\n\
         behavior delta \"Delta\" {\n  verify unit \"test delta\"\n}\n\n\
         invariant gamma_orphan \"Gamma\" {\n}\n",
    )
}

fn reviewed(project: &Assembled, entity_id: Option<&str>, depth: usize) -> Vec<String> {
    review(&project.view(), &ReviewRequest { entity_id, depth })
        .unwrap()
        .rows
        .into_iter()
        .map(|row| row.entity_id)
        .collect()
}

fn finding(entity_id: &str, gap: ReviewGap) -> ReviewFinding {
    ReviewFinding {
        entity_id: entity_id.to_string(),
        gap,
    }
}

#[specforge_test(
    behavior = "review_coverage_gaps",
    verify = "the review lists the coverage view's rows of the entities within depth hops of entity_id, or of every entity that counts toward coverage"
)]
fn the_review_lists_the_neighbourhood_s_rows() {
    let project = project();
    assert_eq!(reviewed(&project, Some("alpha"), 1), ["alpha"]);
    assert_eq!(reviewed(&project, Some("alpha"), 2), ["alpha", "delta"]);
    assert_eq!(reviewed(&project, Some("delta"), 0), ["delta"]);
    // The feature is not testable: left out of the whole project's rows.
    assert_eq!(
        reviewed(&project, None, 1),
        ["alpha", "delta", "gamma_orphan"]
    );
}

#[specforge_test(
    behavior = "review_coverage_gaps",
    verify = "an entity that declares no obligation is a warning finding"
)]
fn an_entity_without_obligations_is_a_warning() {
    let project = project();
    let outcome = review(&project.view(), &ReviewRequest::default()).unwrap();
    let missing: Vec<&ReviewFinding> = outcome
        .findings
        .iter()
        .filter(|f| f.gap == ReviewGap::NoObligations)
        .collect();
    assert_eq!(
        missing,
        [&finding("gamma_orphan", ReviewGap::NoObligations)]
    );
    assert_eq!(ReviewGap::NoObligations.severity(), "warning");
    assert_eq!(
        missing[0].message(),
        "Entity 'gamma_orphan' has no verify declarations"
    );
}

#[specforge_test(
    behavior = "review_coverage_gaps",
    verify = "an unconnected entity is an info finding"
)]
fn an_unconnected_entity_is_an_info() {
    let project = project();
    let outcome = review(&project.view(), &ReviewRequest::default()).unwrap();
    let unconnected: Vec<&ReviewFinding> = outcome
        .findings
        .iter()
        .filter(|f| f.gap == ReviewGap::Unconnected)
        .collect();
    assert_eq!(
        unconnected,
        [&finding("gamma_orphan", ReviewGap::Unconnected)]
    );
    assert_eq!(ReviewGap::Unconnected.severity(), "info");
    assert_eq!(
        unconnected[0].message(),
        "Entity 'gamma_orphan' is unconnected: no edge links it to another entity"
    );
    let json = serde_json::to_value(outcome.document()).unwrap();
    assert_eq!(json["entity_id"], "*");
    assert_eq!(json["findings"][1]["severity"], "info");
}

#[specforge_test(
    behavior = "review_coverage_gaps",
    verify = "the review's depth defaults to one hop"
)]
fn review_defaults_to_one_hop() {
    assert_eq!(ReviewRequest::default().depth, 1);
    let project = project();
    let default = review(
        &project.view(),
        &ReviewRequest {
            entity_id: Some("alpha"),
            ..Default::default()
        },
    )
    .unwrap();
    let ids: Vec<&str> = default.rows.iter().map(|r| r.entity_id.as_str()).collect();
    assert_eq!(ids, reviewed(&project, Some("alpha"), 1));
}

#[specforge_test(
    behavior = "review_coverage_gaps",
    verify = "a project with nothing that counts toward coverage has no rows and no findings"
)]
fn a_review_of_nothing_testable_is_empty() {
    // Only a feature with no verify and no edges.
    let project =
        Assembled::over(registries(&["behavior"], &[])).source("feature beta \"Beta\" {\n}\n");
    let outcome = review(&project.view(), &ReviewRequest::default()).unwrap();
    assert!(outcome.rows.is_empty() && outcome.findings.is_empty());
}

#[specforge_test(
    behavior = "review_coverage_gaps",
    verify = "a recorded report that cannot be read is the review's E045 failure"
)]
fn an_unreadable_report_is_e045() {
    let project = project();
    std::fs::write(project.root().join("specforge-report.json"), "{not json").unwrap();
    let error = review(&project.view(), &ReviewRequest::default()).unwrap_err();
    assert_eq!(error.error.code, "E045");
}

#[specforge_test(
    behavior = "explore_the_graph",
    verify = "the exploration and the review reach the same entities at the same depth"
)]
fn the_exploration_and_the_review_share_one_neighbourhood() {
    let chain: String = [
        ("a", Some("b")),
        ("b", Some("c")),
        ("c", Some("d")),
        ("d", None),
    ]
    .iter()
    .map(|(id, next)| {
        let next = next.map_or_else(String::new, |next| format!("  next [{next}]\n"));
        format!("behavior {id} \"{id}\" {{\n  verify unit \"test {id}\"\n{next}}}\n")
    })
    .collect();
    let project = Assembled::over(registries(&["behavior"], &[])).source(&chain);
    for depth in 0..=3 {
        let explored = explore(
            &project.view(),
            &ExplorationRequest {
                entity_id: Some("a"),
                depth: Some(depth),
                ..Default::default()
            },
        )
        .unwrap()
        .into_value_and_notices()
        .0;
        assert_eq!(
            explored.selected,
            reviewed(&project, Some("a"), depth),
            "depth {depth}"
        );
    }
}
