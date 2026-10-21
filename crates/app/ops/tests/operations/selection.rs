//! A read's selection (ADR 0177): the entity a request names, the hops counted from it and the
//! kinds it keeps, checked once by the project view.

use crate::view_support::{Assembled, Viewed, registries};
use specforge_ops::OpErrorKind;
use specforge_ops::read::coverage::{CoverageQuery, coverage};
use specforge_ops::read::explore::{ExplorationRequest, explore};
use specforge_ops::read::export::{ExportField, Request, export, export_recorded};
use specforge_ops::read::query::{ListRequest, SearchRequest, list, search};
use specforge_ops::read::review::{ReviewRequest, review};
use specforge_ops::view::{DEPTH_WITHOUT_ENTITY, EntityField, Select};
use specforge_test_macros::test as specforge_test;

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

#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "a kind of kinds the project does not know matches nothing and is reported as I020, on every surface"
)]
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

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "a depth without the entity it counts from is one refusal, about the entity, on every read"
)]
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

#[specforge_test(
    behavior = "select_entities_over_the_view",
    verify = "an empty kind is a kind the project does not know: it lists nothing and is an I020 notice"
)]
fn an_empty_kind_is_a_kind_the_project_lacks() {
    let project = project();
    let view = project.view();
    let empty = |notices: &[specforge_common::Diagnostic]| {
        assert_eq!(codes(notices), ["I020"], "{notices:?}");
        assert_eq!(notices[0].message, "unknown entity kind ''");
    };

    let listed = list(
        &view,
        &ListRequest {
            kind: Some(""),
            ..Default::default()
        },
    );
    assert!(listed.notices().len() == 1, "{:?}", listed.notices());
    empty(listed.notices());
    assert!(listed.into_value_and_notices().0.is_empty());

    let covered = coverage(
        &view,
        &CoverageQuery {
            kind: Some(""),
            ..Default::default()
        },
    )
    .unwrap();
    empty(covered.notices());
    let searched = search(
        &view,
        &SearchRequest {
            kinds: vec![""],
            ..Default::default()
        },
    )
    .unwrap();
    empty(searched.notices());
    let explored = explore(
        &view,
        &ExplorationRequest {
            kind: Some(""),
            ..Default::default()
        },
    )
    .unwrap();
    empty(explored.notices());
}

/// A project with a feature `auth` and a behavior `login` that belongs to it.
fn linked() -> Assembled {
    Assembled::over(registries(&["behavior", "feature"], &[]))
        .source("feature auth \"Auth\" {\n}\n\nbehavior login \"Log in\" {\n  features [auth]\n}\n")
}

#[test]
fn the_view_resolves_an_entity_with_its_record_and_standing() {
    let project = linked();
    let view = project.view();

    let entity = view.entity("login", EntityField::EntityId).unwrap();
    assert_eq!(entity.node.id.raw.as_str(), "login");
    assert_eq!(entity.record.id, "login");
    let (record, standing) = view.entities().get("login").unwrap();
    assert_eq!(entity.record.id, record.id);
    assert_eq!(format!("{:?}", entity.standing), format!("{standing:?}"));

    let refusal = view.entity("logn", EntityField::EntityId).unwrap_err();
    assert_eq!(refusal.subject, Some(EntityField::EntityId));
    assert_eq!(refusal.error.code, "E003");
    assert_eq!(
        refusal.error.suggestion.as_deref(),
        Some("did you mean 'login'?")
    );
}

#[test]
fn a_selection_refuses_a_depth_before_the_entity() {
    let project = linked();
    let view = project.view();
    let select = |entity, depth| {
        view.select(&Select {
            entity,
            named_by: EntityField::EntityId,
            depth,
            kinds: &[],
        })
    };

    let refusal = select(None, Some(1)).err().unwrap();
    assert_eq!(refusal.error.code, DEPTH_WITHOUT_ENTITY);
    assert_eq!(refusal.subject, Some(EntityField::EntityId));
    let refusal = select(Some("logn"), Some(1)).err().unwrap();
    assert_eq!(refusal.error.code, "E003");
    assert!(select(Some("login"), Some(1)).is_ok());
    assert!(select(None, None).unwrap().entity.is_none());
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "a read selection selects what its entity reaches within its depth, of its kinds; the entity is selected only when its kind is kept, and an export draws it always"
)]
fn a_selection_selects_what_its_entity_reaches_of_its_kinds() {
    let project = linked();
    let view = project.view();
    let select = |entity, depth, kinds| {
        view.select(&Select {
            entity,
            named_by: EntityField::EntityId,
            depth,
            kinds,
        })
        .unwrap()
    };
    let ids = |selected: &specforge_graph::Selected| -> Vec<String> {
        selected.ids().map(|id| id.to_string()).collect()
    };

    let given = select(Some("login"), Some(0), &[]).selected(None);
    assert_eq!(ids(&given), ["login"]);
    let defaulted = select(Some("login"), None, &[]).selected(Some(1));
    let mut reached: Vec<String> = view
        .graph()
        .reach("login", Some(1))
        .unwrap()
        .iter()
        .map(|r| r.id.to_string())
        .collect();
    reached.sort();
    assert_eq!(ids(&defaulted), reached);
    let kept = select(Some("login"), None, &["feature"]).selected(None);
    assert_eq!(ids(&kept), ["auth"]);
    assert!(!kept.contains("login"));
    assert!(kept.drawn().any(|id| id.as_str() == "login"));
    assert_eq!(
        ids(&select(None, None, &[]).selected(None)),
        ["auth", "login"]
    );
}

#[test]
fn a_kind_filter_keeps_the_kinds_given_and_reports_each_unknown_one() {
    let project = linked();
    let view = project.view();

    let all = view.kind_filter(&[]);
    assert!(all.kinds().is_empty() && all.notices().is_empty());

    let some = view.kind_filter(&["behaviour", "behaviour", "feature"]);
    assert_eq!(some.kinds(), ["behaviour", "behaviour", "feature"]);
    assert_i020_for_behaviour(some.notices());
}
