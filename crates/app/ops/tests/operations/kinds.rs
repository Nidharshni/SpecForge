//! The kinds a project knows (`ProjectView::kinds`, ADR 0015 "Query" Q3):
//! one answer to "is this a known kind" for a filter over entities and for
//! an argument that needs a kind's declaration.

use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Graph, Node};
use specforge_ops::OpErrorKind;
use specforge_parser::{EntityId, EntityKind, FieldMap};
use specforge_protocol_types::{EntityKindDescriptor, ExtensionDeclaration, HandshakeResponse};
use specforge_registry::build_registries;
use specforge_test_macros::test as specforge_test;

use crate::view_support::Project;

fn node(id: &str, kind: &str) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: None,
        fields: FieldMap::new(),
        source_span: SourceSpan {
            file: Sym::new("main.spec"),
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 0,
        },
        methods: Vec::new(),
    }
}

/// A project whose extension declares `behavior` and `adr`, with an entity
/// of each and one E024 entity of the undeclared kind `widget`.
fn project() -> Project {
    let kind = |name: &str| EntityKindDescriptor {
        name: name.into(),
        ..EntityKindDescriptor::default()
    };
    let declaration = ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: "@t/kinds".into(),
            version: "1.0.0".into(),
            ..HandshakeResponse::default()
        },
        entities: vec![kind("behavior"), kind("adr")],
        ..ExtensionDeclaration::default()
    };
    let mut graph = Graph::new();
    graph.add_node(node("login", "behavior"));
    graph.add_node(node("pick_db", "adr"));
    graph.add_node(node("gizmo", "widget"));
    Project::of_graph(graph, build_registries(vec![declaration]))
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "a kind filter reports each kind the project does not know with I020, naming the closest"
)]
fn a_filter_reports_each_unknown_kind_once_naming_the_closest() {
    let project = project();
    let view = project.view();
    let notices =
        view.kinds()
            .unknown_in(&["behaviour", "widget", "behaviour", "Behavior", "zzzzzzzz"]);

    let reported: Vec<(&str, &str, Option<&str>)> = notices
        .iter()
        .map(|n| (n.code.as_str(), n.message.as_str(), n.suggestion.as_deref()))
        .collect();
    assert_eq!(
        reported,
        [
            (
                "I020",
                "unknown entity kind 'behaviour'",
                Some("did you mean 'behavior'?")
            ),
            // A kind equal but for case is the closest, first.
            (
                "I020",
                "unknown entity kind 'Behavior'",
                Some("did you mean 'behavior'?")
            ),
            ("I020", "unknown entity kind 'zzzzzzzz'", None),
        ],
        "`widget` is written by an entity, so a filter knows it"
    );
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "an argument naming an undeclared kind is refused with unknown_kind naming the closest declared kind"
)]
fn an_argument_needs_a_declared_kind() {
    let project = project();
    let view = project.view();
    let kinds = view.kinds();

    assert!(kinds.declared("behavior").is_ok());
    assert!(kinds.has("widget") && !kinds.declares("widget"));

    // Written by an entity, never declared: a filter knows it, a
    // declaration-needing argument does not.
    let error = kinds.declared("widget").unwrap_err();
    assert_eq!(error.code, "unknown_kind");
    assert_eq!(error.kind, OpErrorKind::InvalidInput);
    assert_eq!(error.message, "unknown entity kind 'widget'");
    assert_eq!(error.suggestion, None);

    // Names are exact; the suggestion fixes the case in one step.
    let error = kinds.declared("ADR").unwrap_err();
    assert_eq!(error.message, "unknown entity kind 'ADR'");
    assert_eq!(error.suggestion.as_deref(), Some("did you mean 'adr'?"));
}
