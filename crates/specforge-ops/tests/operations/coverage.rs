use specforge_extension_sdk::prelude::FieldType;
use specforge_ops::coverage::{CoverageQuery, CoverageRow, coverage, row};
use specforge_project::coverage::Status;
use specforge_test::prelude::*;

use crate::view_support::{Project, registries_with};

/// login (proven), logout (one unproven obligation), a union, an abstract
/// behavior, a governance constraint that declares nothing and one that
/// declares an obligation, and a feature (not testable).
fn project() -> Project {
    let build = registries_with(&["behavior", "type"], &["constraint"], |c| {
        c.kind("feature", |k| {
            k.keyword("feature");
        });
        c.enhance("behavior", "@t/soft", |e| {
            e.field("abstract", |f| {
                f.field_type(FieldType::Bool).exempts_obligations();
            });
        });
    });
    let project = Project::new(
        r#"
behavior login "Login" {
  verify unit "logs in"
}

behavior logout "Logout" {
  verify unit "logs out"
}

behavior base "Base" {
  abstract true
}

type Status = active | inactive

constraint latency "Latency" {
}

constraint uptime "Uptime" {
  verify unit "stays up"
}

feature signin "Sign in" {
  verify unit "a feature's statement"
}
"#,
        build,
    );
    project.record_passing(&[("login", "logs in"), ("signin", "a feature's statement")]);
    project
}

fn ids(rows: &[CoverageRow]) -> Vec<&str> {
    rows.iter().map(|r| r.entity_id.as_str()).collect()
}

#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "with no filters the rows are the entities that count toward coverage"
)]
fn the_view_lists_what_counts_toward_coverage() {
    let project = project();
    let outcome = coverage(&project.view(), &CoverageQuery::default()).unwrap();
    // The union, the abstract behavior and the constraint that declares
    // nothing owe nothing; the feature's kind is not testable.
    assert_eq!(ids(&outcome.rows), ["login", "logout", "uptime"]);
    assert_eq!(outcome.rows.len(), outcome.summary.testable_total);
    assert!(outcome.rows.iter().all(|r| r.testable && !r.exempt));
    let proven = outcome
        .rows
        .iter()
        .filter(|r| r.status() == Status::Covered);
    assert_eq!(proven.count(), outcome.summary.testable_proven);

    // Narrowed by kind and by status.
    let behaviors = CoverageQuery {
        kind: Some("behavior"),
        ..Default::default()
    };
    let rows = coverage(&project.view(), &behaviors).unwrap().rows;
    assert_eq!(ids(&rows), ["login", "logout"]);
    let uncovered = CoverageQuery {
        status: Some(Status::Uncovered),
        ..Default::default()
    };
    let rows = coverage(&project.view(), &uncovered).unwrap().rows;
    assert_eq!(ids(&rows), ["logout", "uptime"]);
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "an entity is unverified when it counts toward coverage and is not proven"
)]
fn coverage_rows_and_stats_read_one_standing() {
    let project = project();
    let view = project.view();
    let outcome = coverage(&view, &CoverageQuery::default()).unwrap();
    let entities = view.entities();
    // Every listed row is its entity's standing, and only those that count
    // are listed.
    for row in &outcome.rows {
        let standing = entities.standing(&row.entity_id).unwrap();
        assert_eq!(row.testable, standing.testable, "{}", row.entity_id);
        assert_eq!(row.exempt, standing.exempt(), "{}", row.entity_id);
        assert!(standing.counts(), "{}", row.entity_id);
    }
    let counted = entities.iter().filter(|(_, s)| s.counts()).count();
    assert_eq!(outcome.rows.len(), counted);
    assert_eq!(outcome.rows.len(), outcome.summary.testable_total);
    // A row named by id is its standing too, counted or not.
    for (record, standing) in entities.iter() {
        let row = row(&view, &record.id).unwrap().unwrap();
        assert_eq!(
            (row.testable, row.exempt),
            (standing.testable, standing.exempt())
        );
    }
    // Stats count the same entities.
    let stats = specforge_ops::stats::stats(&view).unwrap();
    assert_eq!(stats.testable_count, counted);
}

#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "an exempt entity named by entity_id is returned with exempt true"
)]
fn an_exempt_entity_is_reachable_by_id() {
    let project = project();
    for exempt in ["Status", "base", "latency"] {
        let query = CoverageQuery {
            entity_id: Some(exempt),
            ..Default::default()
        };
        let rows = coverage(&project.view(), &query).unwrap().rows;
        assert_eq!(ids(&rows), [exempt]);
        assert!(rows[0].testable && rows[0].exempt, "{exempt}");
        assert_eq!(row(&project.view(), exempt).unwrap(), Some(rows[0].clone()));
    }
    // Not testable: reachable, neither testable nor exempt.
    let signin = row(&project.view(), "signin").unwrap().unwrap();
    assert!(!signin.testable && !signin.exempt);
    // An entity the graph lacks has no row.
    let ghost = CoverageQuery {
        entity_id: Some("ghost"),
        ..Default::default()
    };
    assert!(coverage(&project.view(), &ghost).unwrap().rows.is_empty());
    assert_eq!(row(&project.view(), "ghost").unwrap(), None);
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "an entity is unverified when it counts toward coverage and is not proven"
)]
fn unverified_is_counted_and_not_proven() {
    let project = project();
    let view = project.view();
    let coverage = view.coverage().unwrap();
    let ids: Vec<&str> = coverage
        .entities()
        .records()
        .iter()
        .map(|record| record.id.as_str())
        .collect();
    let unverified: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|id| coverage.is_unverified(id))
        .collect();
    // login is proven; the feature is not testable (though its test
    // passes); the union, the abstract behavior and the bare constraint
    // owe nothing.
    assert_eq!(unverified, ["logout", "uptime"]);
    for id in ids {
        let row = row(&view, id).unwrap().unwrap();
        assert_eq!(row.unverified(), coverage.is_unverified(id), "{id}");
    }
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "coverage is computed once per compile and report content, and again after the report changes"
)]
fn the_view_scores_against_the_report_as_it_is_now() {
    let project = project();
    let before = project.view().coverage().unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &project.view().coverage().unwrap()
    ));
    project.record_passing(&[("login", "logs in"), ("logout", "logs out")]);
    let after = project.view().coverage().unwrap();
    assert!(!after.is_unverified("logout"));
    assert!(before.is_unverified("logout"));
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "a coverage row is one JSON document on every surface"
)]
fn a_coverage_row_is_one_document() {
    let project = project();
    let view = project.view();

    let login = row(&view, "login").unwrap().unwrap().to_json();
    assert_eq!(
        login,
        serde_json::json!({
            "entity_id": "login",
            "kind": "behavior",
            "status": "covered",
            "declared": true,
            "linked": true,
            "evidence_collected": true,
            "obligations": 1,
            "proven": 1,
            "unproven": [],
            "exempt": false,
        })
    );

    let base = row(&view, "base").unwrap().unwrap().to_json();
    assert_eq!(base["exempt"], true);
    assert_eq!(base["declared"], false);
    assert_eq!(base["obligations"], 0);
    assert_eq!(base.as_object().unwrap().len(), 10);
}
