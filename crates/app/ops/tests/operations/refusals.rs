//! Every refusal of `check`, `trace` and `reach`, field by field: the kind,
//! the code, the message, the suggestion, the entity and the subject each
//! states. They are pinned once, before any refusal moves (round 13, plan 10).

use crate::view_support::{Assembled, Viewed};
use specforge_common::{Diagnostic, Severity, SourceSpan, Sym};
use specforge_graph::{Graph, Node};
use specforge_ops::analysis::check::{
    CheckField, CheckOptions, check, parse_lint_profiles, parse_severity,
};
use specforge_ops::read::trace::{Target, reach, trace};
use specforge_ops::view::EntityField;
use specforge_ops::{OpError, Refusal};
use specforge_parser::{EntityId, EntityKind, FieldMap};
use specforge_project::LintProfile;
use specforge_registry::RegistryBuild;
use std::fmt::Debug;

fn node(id: &str, kind: &str) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: None,
        fields: FieldMap::new(),
        source_span: SourceSpan {
            file: Sym::new("test.spec"),
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 0,
        },
        methods: Vec::new(),
        obligations: Vec::new(),
    }
}

/// `kind code | message | suggestion | entity | subject`, `-` for none.
fn line(error: &OpError, subject: Option<impl Debug>) -> String {
    let none = |text: Option<&str>| text.unwrap_or("-").to_string();
    format!(
        "{:?} {} | {} | {} | {} | {}",
        error.kind,
        error.code,
        error.message,
        none(error.suggestion.as_deref()),
        none(error.entity.as_deref()),
        subject.map_or("-".to_string(), |field| format!("Some({field:?})")),
    )
}

#[specforge_test_macros::test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn check_and_trace_refusals_are_pinned_field_by_field() {
    let severity = |name: &str| {
        let refusal = parse_severity(name).unwrap_err();
        line(&refusal.error, refusal.subject)
    };
    let profiles = |names: &[&str]| {
        let refusal = parse_lint_profiles(names).unwrap_err();
        line(&refusal.error, refusal.subject)
    };
    let reported = vec![Diagnostic::untyped("W002", Severity::Warning, "unused")];
    let rootless =
        specforge_project::testing::Assembled::over(RegistryBuild::default()).reporting(reported);
    let rootless = rootless.rootless_view();
    let no_root = |options: CheckOptions| {
        let error = check(&rootless, &options).unwrap_err();
        line(&error, None::<CheckField>)
    };

    let mut graph = Graph::new();
    graph.add_node(node("login", "behavior"));
    graph.add_node(node("signin", "feature"));
    let project = Assembled::over(Default::default()).graph(graph);
    let view = project.view();
    let traced = |id: &str| {
        let refusal = Refusal::<EntityField>::from(trace(&view, Target::Entity(id)).unwrap_err());
        line(&refusal.error, refusal.subject)
    };

    let no_project = "PreconditionFailed no_project | a lint profile or the build cache needs a \
                      project root, and this project has none | - | - | -";
    let rows: Vec<(&str, String, &str)> = vec![
        (
            "severity",
            severity("errors"),
            "InvalidInput unknown_severity | Unknown severity 'errors' (available: error, warning, info) | did you mean 'error'? | - | Some(Severity)",
        ),
        (
            "severity folds case",
            severity("ERRORS"),
            "InvalidInput unknown_severity | Unknown severity 'ERRORS' (available: error, warning, info) | did you mean 'error'? | - | Some(Severity)",
        ),
        (
            "severity, none close",
            severity("fatal"),
            "InvalidInput unknown_severity | Unknown severity 'fatal' (available: error, warning, info) | - | - | Some(Severity)",
        ),
        (
            "lint profile, the first unknown",
            profiles(&["inferred", "pedantik", "nonsense"]),
            "InvalidInput unknown_lint_profile | Unknown lint profile 'pedantik' (available: inferred, pedantic) | did you mean 'pedantic'? | - | Some(LintProfiles)",
        ),
        (
            "lint profile, none close",
            profiles(&["nonsense"]),
            "InvalidInput unknown_lint_profile | Unknown lint profile 'nonsense' (available: inferred, pedantic) | - | - | Some(LintProfiles)",
        ),
        (
            "check, cache without a root",
            no_root(CheckOptions {
                record_cache: true,
                ..CheckOptions::default()
            }),
            no_project,
        ),
        (
            "check, lint without a root",
            no_root(CheckOptions {
                lint_profiles: vec![LintProfile::Inferred],
                ..CheckOptions::default()
            }),
            no_project,
        ),
        (
            "trace, a near id",
            traced("logn"),
            "EntityNotFound E003 | unresolved entity 'logn' — not found in graph | did you mean 'login'? | logn | Some(EntityId)",
        ),
        (
            "trace, none close",
            traced("zzzzzzzz"),
            "EntityNotFound E003 | unresolved entity 'zzzzzzzz' — not found in graph | - | zzzzzzzz | Some(EntityId)",
        ),
        (
            "reach, a near id (an OpError: no subject)",
            line(&reach(&view, ["logn"]).unwrap_err(), None::<CheckField>),
            "EntityNotFound E003 | unresolved entity 'logn' — not found in graph | did you mean 'login'? | logn | -",
        ),
    ];
    for (label, got, want) in rows {
        assert_eq!(got, want, "{label}");
    }
}
