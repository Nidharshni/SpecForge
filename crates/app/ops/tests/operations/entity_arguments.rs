//! An argument that names an entity, or counts hops from one, is refused by the operation (ADR 0101 D1,
//! ADR 0015 Q5): an entity the graph lacks is `view::not_found` about the request field that named it.

use crate::view_support::{Assembled, Viewed, registries};
use specforge_ops::OpErrorKind;
use specforge_ops::read::coverage::{CoverageQuery, coverage};
use specforge_ops::view::EntityField;
use specforge_test::prelude::*;

/// A project with one behavior, `login`.
fn project() -> Assembled {
    Assembled::over(registries(&["behavior"], &[])).source("behavior login \"Log in\" {\n}\n")
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
