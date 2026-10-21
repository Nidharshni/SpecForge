//! A read's selection (ADR 0177): the entity a request names, the hops counted from it and the
//! kinds it keeps, checked once by the project view.

use crate::view_support::{Assembled, Viewed, registries};
use specforge_ops::OpErrorKind;
use specforge_ops::read::explore::{ExplorationRequest, explore};
use specforge_ops::read::export::{ExportField, Request, export, export_recorded};
use specforge_ops::read::review::{ReviewRequest, review};
use specforge_ops::view::{DEPTH_WITHOUT_ENTITY, EntityField};

/// A project with one behavior, `login`.
fn project() -> Assembled {
    Assembled::over(registries(&["behavior", "feature"], &[]))
        .source("behavior login \"Log in\" {\n}\n")
}

fn codes(notices: &[specforge_common::Diagnostic]) -> Vec<String> {
    notices.iter().map(|n| n.code.to_string()).collect()
}

fn assert_i020_for_behaviour(notices: &[specforge_common::Diagnostic]) {
    assert_eq!(codes(notices), ["I020"], "{notices:?}");
    assert_eq!(notices[0].message, "unknown entity kind 'behaviour'");
    assert_eq!(
        notices[0].suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
}

#[test]
#[ignore = "flipped by T5"]
fn export_reports_an_unknown_kind() {
    let project = project();
    let view = project.view();
    for scope in [None, Some("login")] {
        let request = Request {
            kinds: vec!["behaviour"],
            scope,
            ..Request::default()
        };
        let exported = export(&view, &request).unwrap();
        assert_i020_for_behaviour(exported.notices());
        let recorded = export_recorded(&view, &request);
        assert_i020_for_behaviour(recorded.notices());
    }
}

#[test]
#[ignore = "flipped by T6"]
fn a_depth_without_its_entity_is_one_refusal() {
    let project = project();
    let view = project.view();
    let message = "a depth counts hops from an entity: give an entity too";
    let explored = explore(
        &view,
        &ExplorationRequest {
            depth: Some(1),
            ..Default::default()
        },
    )
    .unwrap_err();
    let reviewed = review(
        &view,
        &ReviewRequest {
            depth: Some(1),
            ..Default::default()
        },
    )
    .unwrap_err();
    let exported = export(
        &view,
        &Request {
            depth: Some(2),
            ..Request::default()
        },
    )
    .unwrap_err();
    for error in [&explored.error, &reviewed.error, &exported.error] {
        assert_eq!(error.kind, OpErrorKind::InvalidInput);
        assert_eq!(error.code, DEPTH_WITHOUT_ENTITY);
        assert_eq!(error.message, message);
    }
    assert_eq!(explored.subject, Some(EntityField::EntityId));
    assert_eq!(reviewed.subject, Some(EntityField::EntityId));
    assert_eq!(exported.subject, Some(ExportField::Scope));
}
