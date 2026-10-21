//! An argument that names an entity, or counts hops from one, is refused by the operation (ADR 0101 D1,
//! ADR 0015 Q5): an entity the graph lacks is `view::not_found` about the request field that named it.

use crate::view_support::{Assembled, Viewed, registries};
use specforge_ops::OpErrorKind;
use specforge_ops::authoring::fix::{self, FixField, FixQuery};
use specforge_ops::read::coverage::{CoverageQuery, coverage};
use specforge_ops::read::explore::{ExplorationRequest, explore};
use specforge_ops::read::navigate::Navigator;
use specforge_ops::read::query::{SearchField, SearchRequest, search};
use specforge_ops::read::review::{ReviewRequest, review};
use specforge_ops::view::{DEPTH_WITHOUT_ENTITY, EntityField};
use specforge_test::prelude::*;

/// A project with one behavior, `login`.
fn project() -> Assembled {
    Assembled::over(registries(&["behavior", "feature"], &[]))
        .source("behavior login \"Log in\" {\n}\n")
}

#[specforge_test(
    behavior = "compute_the_coverage_view",
    verify = "an entity_id the graph lacks is E003 naming the closest entity"
)]
fn coverage_refuses_an_entity_the_graph_lacks() {
    let project = project();
    let query = CoverageQuery {
        entity_id: Some("logn"),
        ..Default::default()
    };
    let refusal = coverage(&project.view(), &query).unwrap_err();
    assert_eq!(refusal.subject, Some(EntityField::EntityId));
    assert_eq!(refusal.error.kind, OpErrorKind::EntityNotFound);
    assert_eq!(refusal.error.code, "E003");
    assert_eq!(
        refusal.error.suggestion.as_deref(),
        Some("did you mean 'login'?")
    );
}

#[specforge_test(
    behavior = "compute_the_coverage_view",
    verify = "an unknown kind matches nothing and is an I020 notice; kind narrows an entity_id's row as status does"
)]
fn coverage_kind_is_a_filter_like_every_kind_filter() {
    let project = project();
    let view = project.view();
    let unknown = coverage(
        &view,
        &CoverageQuery {
            kind: Some("behaviour"),
            ..Default::default()
        },
    )
    .unwrap();
    let codes: Vec<_> = unknown
        .notices()
        .iter()
        .map(|n| n.code.to_string())
        .collect();
    assert_eq!(codes, ["I020"]);
    assert!(unknown.into_value_and_notices().0.rows.is_empty());

    let ids = |kind| {
        let query = CoverageQuery {
            entity_id: Some("login"),
            kind: Some(kind),
            ..Default::default()
        };
        let read = coverage(&view, &query).unwrap();
        assert!(read.notices().is_empty());
        read.into_value_and_notices()
            .0
            .rows
            .into_iter()
            .map(|row| row.entity_id)
            .collect::<Vec<_>>()
    };
    assert!(ids("feature").is_empty());
    assert_eq!(ids("behavior"), ["login"]);
}

// The invariant's own verify line: a missing entity is one refusal on every read view.
#[test]
fn fixes_refuse_an_entity_the_graph_lacks() {
    let project = project();
    let query = FixQuery {
        entity: Some("logn"),
        ..Default::default()
    };
    let refusal = fix::fixes(&Navigator::new(project.view()), &[], &query).unwrap_err();
    assert_eq!(refusal.subject, Some(FixField::EntityId));
    assert_eq!(refusal.error.code, "E003");
    assert_eq!(
        refusal.error.suggestion.as_deref(),
        Some("did you mean 'login'?")
    );
}

#[specforge_test(
    behavior = "select_entities_over_the_view",
    verify = "a references entity the graph lacks is E003 naming the closest entity"
)]
fn search_refuses_a_referenced_entity_the_graph_lacks() {
    let project = project();
    let request = SearchRequest {
        referencing: Some("logn"),
        ..Default::default()
    };
    let refusal = search(&project.view(), &request).unwrap_err();
    assert_eq!(refusal.subject, Some(SearchField::Referencing));
    assert_eq!(refusal.error.code, "E003");
    assert_eq!(
        refusal.error.suggestion.as_deref(),
        Some("did you mean 'login'?")
    );
}

fn assert_depth_refused<T: std::fmt::Debug>(read: Result<T, specforge_ops::Refusal<EntityField>>) {
    let refusal = read.unwrap_err();
    assert_eq!(refusal.error.kind, OpErrorKind::InvalidInput);
    assert_eq!(refusal.error.code, DEPTH_WITHOUT_ENTITY);
    assert_eq!(refusal.subject, Some(EntityField::EntityId));
}

#[specforge_test(
    behavior = "explore_the_graph",
    verify = "a depth without entity_id is refused as invalid input about entity_id"
)]
fn explore_refuses_a_depth_without_an_entity() {
    let project = project();
    assert_depth_refused(explore(
        &project.view(),
        &ExplorationRequest {
            depth: Some(1),
            ..Default::default()
        },
    ));
}

#[specforge_test(
    behavior = "review_coverage_gaps",
    verify = "a depth without entity_id is refused as invalid input about entity_id"
)]
fn review_refuses_a_depth_without_an_entity() {
    let project = project();
    assert_depth_refused(review(
        &project.view(),
        &ReviewRequest {
            depth: Some(1),
            ..Default::default()
        },
    ));
    let with_entity = review(
        &project.view(),
        &ReviewRequest {
            entity_id: Some("login"),
            depth: None,
        },
    )
    .unwrap();
    assert_eq!(with_entity.depth, Some(1));
}
