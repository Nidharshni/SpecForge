//! An entity's coverage status says whether it counts (ADR 0216): the
//! coverage row, its filter, inspect's coverage and a query with coverage
//! write the one status the coverage rule answers.

use serde_json::Value;
use specforge_ops::read::coverage::{CoverageQuery, coverage};
use specforge_ops::read::inspect::inspect;
use specforge_ops::read::query::{QueryRequest, query};
use specforge_project::coverage::Status;
use specforge_test::prelude::*;

use crate::view_support::{Assembled, Viewed, answered, registries_with, row};

/// `login` (counts, proven), `logout` (counts, declares none: W004),
/// `Choice` (a union of an obligated kind: exempt) and `signin` (a feature,
/// a kind that is not testable, whose obligation a passing test proves).
fn standings() -> Assembled {
    let build = registries_with(&["behavior", "type"], &[], |c| {
        c.kind("feature", |k| {
            k.keyword("feature");
        });
    });
    let project = Assembled::over(build).source(
        "behavior login \"Login\" {\n  verify unit \"logs in\"\n}\n\n\
         behavior logout \"Logout\" {\n}\n\n\
         type Choice = yes | no\n\n\
         feature signin \"Sign in\" {\n  verify unit \"signs in\"\n}\n",
    );
    project.record_passing(&[("login", "logs in"), ("signin", "signs in")]);
    project
}

/// Each entity's status as every document writes it.
const WRITTEN: [(&str, &str); 4] = [
    ("login", "covered"),
    ("logout", "uncovered"),
    ("Choice", "exempt"),
    ("signin", "not_testable"),
];

#[specforge_test(
    behavior = "compute_the_coverage_view",
    verify = "a row's status is exempt or not_testable for an entity that does not count, and otherwise how its obligations are proven"
)]
fn a_coverage_row_says_whether_its_entity_counts() {
    let project = standings();
    let view = project.view();
    for (id, status) in WRITTEN {
        let document = serde_json::to_value(row(&view, id).document()).unwrap();
        assert_eq!(document["status"], status, "{id}: {document}");
        assert!(document.get("exempt").is_none(), "{id}: {document}");
    }
}

#[specforge_test(
    behavior = "compute_the_coverage_view",
    verify = "a status filter selects the entities of that status, with or without an entity_id"
)]
fn a_status_filter_selects_the_same_rows_with_or_without_an_entity_id() {
    let project = standings();
    let view = project.view();
    for status in [Status::Covered, Status::Partial, Status::Uncovered] {
        let listed: Vec<String> = answered(coverage(
            &view,
            &CoverageQuery {
                status: Some(status),
                ..Default::default()
            },
        ))
        .rows
        .into_iter()
        .map(|row| row.entity_id)
        .collect();
        for (id, _) in WRITTEN {
            let named = answered(coverage(
                &view,
                &CoverageQuery {
                    entity_id: Some(id),
                    status: Some(status),
                    ..Default::default()
                },
            ))
            .rows;
            assert_eq!(
                !named.is_empty(),
                listed.contains(&id.to_string()),
                "{id} {status:?}"
            );
        }
    }
}

#[specforge_test(
    behavior = "inspect_entity_facts",
    verify = "an entity's coverage status is its coverage row's: whether it counts, then how it is proven"
)]
fn inspect_writes_the_status_its_coverage_row_writes() {
    let project = standings();
    let view = project.view();
    for (id, status) in WRITTEN {
        let facts = inspect(&view, id).unwrap();
        let coverage = specforge_ops::read::inspect::coverage(&view, &facts).unwrap();
        assert_eq!(
            serde_json::to_value(coverage.status).unwrap(),
            status,
            "{id}"
        );
    }
}

#[specforge_test(
    behavior = "query_graph_multi_resolution",
    verify = "--include-coverage gives an entity that does not count the status that says why"
)]
fn a_query_with_coverage_writes_the_status_that_says_whether_it_counts() {
    let project = standings();
    for (id, status) in WRITTEN {
        let request = QueryRequest {
            entity_id: id,
            depth: Some(0),
            include_coverage: true,
            ..QueryRequest::default()
        };
        let answered = query(&project.view(), &request).unwrap();
        let document: Value = serde_json::from_str(&answered.into_value_and_notices().0).unwrap();
        assert_eq!(document["nodes"][0]["coverage_status"], status, "{id}");
    }
}
