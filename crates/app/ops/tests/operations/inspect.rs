//! Inspect (`specforge_ops::read::inspect`): one entity's facts, read once over
//! the project view, as MCP inspect, the context prompt and the LSP hover
//! render them (ADR 0015, section "Inspect").

use specforge_common::{Diagnostic, DiagnosticData, Severity, SourceSpan, Sym};
use specforge_extension_sdk::prelude::FieldType;
use specforge_ops::read::inspect::inspect;
use specforge_ops::read::navigate::Reference;
use specforge_ops::view::ProjectView;
use specforge_project::coverage::Status;
use specforge_registry::Standing;
use specforge_test::prelude::*;

use crate::navigate::compile;
use crate::view_support::{Assembled, Viewed, registries, registries_with, row};

/// A project of every standing: `ob` (an obligated kind), `fr` (a testable
/// kind no W004 rule targets), `U` (a union of an obligated kind), `ab`
/// (an `abstract` behavior), `n` (a kind that is not testable), and `done`
/// (an obligated entity that declares two obligations).
const STANDINGS: &str = "\
behavior ob \"Ob\" {\n}\n\
constraint fr \"Free\" {\n}\n\
type U = a | b\n\
behavior ab \"Abstract\" {\n  abstract true\n}\n\
note n \"Note\" {\n}\n\
behavior done \"Done\" {\n  verify unit \"first\"\n  verify unit \"second\"\n}\n";

fn standings() -> Assembled {
    let build = registries_with(&["behavior", "type"], &["constraint"], |c| {
        c.kind("note", |k| {
            k.keyword("note");
        });
        c.enhance("behavior", |e| {
            e.field("abstract", |f| {
                f.field_type(FieldType::Bool).exempts_obligations();
            });
        });
    });
    Assembled::over(build).source(STANDINGS)
}

const IDS: [&str; 6] = ["ob", "fr", "U", "ab", "n", "done"];

fn standing(view: &ProjectView, id: &str) -> Standing {
    inspect(view, id).unwrap().standing.clone()
}

/// `(testable, obligated, exempt)`, how inspect states a standing.
fn shape(view: &ProjectView, id: &str) -> (bool, bool, bool) {
    let facts = inspect(view, id).unwrap();
    (
        facts.standing.testable,
        facts.standing.obligated(),
        facts.standing.exempt(),
    )
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "inspect reports an entity's standing as the coverage view counts it"
)]
fn standing_is_the_coverage_views() {
    let project = standings();
    let view = project.view();
    for id in IDS {
        let standing = standing(&view, id);
        let row = row(&view, id);
        assert_eq!(standing.testable, row.testable, "{id}");
        assert_eq!(standing.exempt(), row.exempt, "{id}");
        assert_eq!(standing.counts(), row.testable && !row.exempt, "{id}");
    }
    assert_eq!(shape(&view, "ob"), (true, true, false));
    assert_eq!(shape(&view, "fr"), (true, false, true));
    assert_eq!(shape(&view, "U"), (true, true, true));
    assert_eq!(shape(&view, "ab"), (true, true, true));
    assert_eq!(shape(&view, "n"), (false, false, false));
    assert_eq!(shape(&view, "done"), (true, true, false));
}

#[specforge_test(
    behavior = "inspect_entity_facts",
    verify = "an entity's coverage is its coverage row's verdict, obligation by obligation"
)]
fn coverage_is_the_coverage_rows_verdict() {
    let project = standings();
    project.record_passing(&[("done", "first")]);
    let view = project.view();
    let facts = inspect(&view, "done").unwrap();
    let coverage = specforge_ops::read::inspect::coverage(&view, &facts).unwrap();
    let row = row(&view, "done");
    assert_eq!(coverage.verdict, row.verdict);
    assert_eq!(coverage.verdict.obligations, 2);
    assert_eq!(coverage.verdict.proven, 1);
    assert_eq!(coverage.status(), Status::Partial);
    assert!(facts.standing.declares_obligations());
    assert_eq!(facts.standing.declares_obligations(), row.declared());
    assert!(coverage.recorded);
}

#[test]
fn without_a_report_the_coverage_is_unrecorded() {
    let project = standings();
    let view = project.view();
    let facts = inspect(&view, "done").unwrap();
    let coverage = specforge_ops::read::inspect::coverage(&view, &facts).unwrap();
    assert!(!coverage.recorded);
    assert_eq!(coverage.status(), Status::Uncovered);
    assert_eq!(coverage.verdict.tests, 0);
    assert!(facts.standing.declares_obligations());
    assert!(
        !inspect(&view, "ob")
            .unwrap()
            .standing
            .declares_obligations()
    );
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "a report that cannot be read is the coverage's error, and the standing still holds"
)]
fn an_unreadable_report_is_the_coverage_error_not_the_inspects() {
    let project = standings();
    let readable: Vec<Standing> = IDS.iter().map(|id| standing(&project.view(), id)).collect();
    std::fs::write(project.root().join("specforge-report.json"), "{not json").unwrap();
    let view = project.view();
    for (id, readable) in IDS.iter().zip(readable) {
        let facts = inspect(&view, id).expect("inspect does not fail on the report");
        let error = specforge_ops::read::inspect::coverage(&view, &facts)
            .expect_err("the coverage is the report's error");
        assert_eq!(error.code, "E045", "{id}");
        assert_eq!(facts.standing, &readable, "{id}");
    }
}

fn span(file: &str, start_line: usize, end_line: usize) -> SourceSpan {
    SourceSpan {
        file: Sym::new(file),
        start_line,
        start_col: 1,
        end_line,
        end_col: 2,
    }
}

#[specforge_test(
    behavior = "inspect_entity_facts",
    verify = "the diagnostics about an entity are its own, not those of an entity whose id contains it"
)]
fn diagnostics_are_those_about_the_entity() {
    // `task` on lines 1–3, `task_id_uniqueness` on 4–8.
    let source = "invariant task \"Task\" {\n  guarantee \"a task exists\"\n}\n\
                  invariant task_id_uniqueness \"Unique\" {\n  guarantee \"ids are unique\"\n  // one\n  // two\n}\n";
    let project = Assembled::over(registries(&[], &["invariant"])).source(source);
    let subject = |entity: &str| DiagnosticData::Subject {
        entity: entity.into(),
    };
    let reported = vec![
        Diagnostic::untyped("W003", Severity::Warning, "a finding").with_span(span(
            "main.spec",
            4,
            8,
        )),
        Diagnostic::untyped("W100", Severity::Warning, "a finding").with_span(span(
            "main.spec",
            2,
            2,
        )),
        Diagnostic::untyped("W101", Severity::Warning, "a finding").with_data(subject("task")),
        Diagnostic::untyped("W102", Severity::Warning, "a finding")
            .with_data(subject("task_id_uniqueness")),
        Diagnostic::untyped("W103", Severity::Warning, "task is everywhere"),
    ];
    let view = project.view().reporting(&reported);
    let codes = |id: &str| -> Vec<String> {
        inspect(&view, id)
            .unwrap()
            .diagnostics
            .into_iter()
            .map(|d| d.code)
            .collect()
    };
    assert_eq!(codes("task"), ["W100", "W101"]);
    assert_eq!(codes("task_id_uniqueness"), ["W003", "W102"]);
}

#[test]
fn facts_carry_the_node_kind_entry_headline_and_obligations() {
    let p = compile(
        &["@specforge/software", "@specforge/testing"],
        &[(
            "a.spec",
            "behavior login \"Log in\" {\n  contract \"The system MUST log a known user in\"\n  \
             category command\n  verify unit \"a known user logs in\"\n}\n",
        )],
    );
    let view = ProjectView::of(&p.project);
    let facts = inspect(&view, "login").unwrap();
    assert_eq!(facts.node.id.raw, "login");
    assert_eq!(facts.node.kind.raw, "behavior");
    assert_eq!(facts.node.title.as_deref(), Some("Log in"));
    let kind = facts
        .kind
        .expect("the software extension declares behavior");
    assert_eq!(kind.source_extension, "@specforge/software");
    assert!(kind.supports_verify);
    assert!(
        kind.declared
            .description
            .as_deref()
            .is_some_and(|d| !d.is_empty()),
        "the kind's description"
    );
    assert_eq!(facts.headline, Some("The system MUST log a known user in"));
    assert_eq!(facts.obligations.len(), 1);
    assert_eq!(
        specforge_ops::read::inspect::obligation_text(&facts.obligations[0]),
        "unit a known user logs in"
    );
    let fields: Vec<&str> = facts
        .node
        .fields
        .entries()
        .iter()
        .map(|e| e.key.as_str())
        .collect();
    assert_eq!(fields, ["contract", "category"]);

    // A kind no extension declares has no entry and no headline.
    let project = Assembled::over(Default::default()).source("gizmo g \"G\" {\n  note \"n\"\n}\n");
    let view = project.view();
    let facts = inspect(&view, "g").unwrap();
    assert!(facts.kind.is_none());
    assert!(facts.headline.is_none());
    assert!(facts.obligations.is_empty());
}

fn reference(peer: &str, peer_kind: &str, field: &str) -> Reference {
    Reference {
        peer: Sym::new(peer),
        peer_kind: Some(Sym::new(peer_kind)),
        field: Sym::new(field),
    }
}

#[test]
fn references_group_as_the_hover_shows_them() {
    let source = "\
behavior create_user \"Create\" {\n  types [user, role]\n  emits [user_created]\n}\n\
behavior update_user \"Update\" {\n  types [user]\n}\n\
feature user_management \"Users\" {\n  types [user]\n}\n\
type user \"User\" {\n}\n\
type role \"Role\" {\n}\n\
event user_created \"Created\" {\n}\n\
type orphan \"Orphan\" {\n}\n";
    let project = Assembled::over(Default::default()).source(source);
    let view = project.view();
    let create = inspect(&view, "create_user").unwrap().references;
    assert!(create.incoming.is_empty());
    assert_eq!(
        create.outgoing,
        [
            reference("user", "type", "types"),
            reference("role", "type", "types"),
            reference("user_created", "event", "emits"),
        ]
    );
    assert_eq!(create.refers_to(), ["role", "user", "user_created"]);

    let user = inspect(&view, "user").unwrap().references;
    assert!(user.outgoing.is_empty());
    assert_eq!(
        user.incoming,
        [
            reference("create_user", "behavior", "types"),
            reference("update_user", "behavior", "types"),
            reference("user_management", "feature", "types"),
        ]
    );
    assert_eq!(
        user.referenced_by(),
        ["create_user", "update_user", "user_management"]
    );

    let orphan = inspect(&view, "orphan").unwrap().references;
    assert!(orphan.incoming.is_empty() && orphan.outgoing.is_empty());
}

#[specforge_test(
    behavior = "inspect_entity_facts",
    verify = "an unknown entity is E003, entity_not_found"
)]
fn an_unknown_entity_is_not_found() {
    let project = standings();
    let error = inspect(&project.view(), "ghost").unwrap_err();
    assert!(error.error.is(specforge_common::codes::E003));
    assert_eq!(error.error.kind, specforge_ops::OpErrorKind::EntityNotFound);
    assert_eq!(
        error.error.message,
        "unresolved entity 'ghost' — not found in graph"
    );
}
