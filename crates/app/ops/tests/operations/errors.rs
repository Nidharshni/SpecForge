use crate::view_support::{Assembled, Viewed};
use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_ops::read::trace::TraceError;
use specforge_parser::{EntityId, EntityKind, FieldMap};

fn span() -> SourceSpan {
    SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 0,
    }
}

fn node(id: &str, kind: &str) -> Node {
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: Some(format!("Title {}", id)),
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
        obligations: Vec::new(),
    }
}

fn build_graph() -> Graph {
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature"));
    graph.add_node(node("b", "behavior"));
    graph.add_edge(Edge {
        source: "a".into(),
        target: "b".into(),
        label: "behaviors".into(),
    });
    graph
}

// M2: trace for non-existent entity returns TraceError::EntityNotFound
#[test]
fn trace_nonexistent_returns_entity_not_found() {
    let project = Assembled::over(Default::default()).graph(build_graph());
    let err = specforge_ops::read::trace::trace(
        &project.view(),
        specforge_ops::read::trace::Target::Entity("nonexistent"),
    )
    .unwrap_err();
    assert!(
        matches!(err, TraceError::EntityNotFound { .. }),
        "expected EntityNotFound, got: {err:?}"
    );
}

#[specforge_test_macros::test(
    behavior = "compute_traceability_chain",
    verify = "tracing an entity the graph lacks is E003 naming the closest entity"
)]
fn tracing_an_unknown_entity_is_e003_with_a_suggestion() {
    let project = Assembled::over(Default::default()).graph(build_graph());
    let trace = |id| {
        specforge_ops::read::trace::trace(
            &project.view(),
            specforge_ops::read::trace::Target::Entity(id),
        )
        .unwrap_err()
    };
    let near = trace("bb");
    assert_eq!(
        near,
        TraceError::EntityNotFound {
            entity_id: "bb".into(),
            near: Some("b".into()),
        }
    );
    let error = specforge_ops::OpError::from(near);
    assert_eq!(error.code, "E003");
    assert_eq!(error.message, "unresolved entity 'bb' — not found in graph");
    assert_eq!(error.suggestion.as_deref(), Some("did you mean 'b'?"));
    // Nothing close: no suggestion.
    let far = specforge_ops::OpError::from(trace("zzzzzzzz"));
    assert_eq!(far.suggestion, None);
}

// Plan 05 (T2): a missing entity is one refusal on every read view.
#[specforge_test_macros::test(
    invariant = "mcp_structured_error_responses",
    verify = "a missing entity is one E003 refusal naming the closest entity on every read view"
)]
fn a_missing_entity_is_one_refusal() {
    use specforge_ops::read::export::{Request, export};
    use specforge_ops::read::navigate::Navigator;
    use specforge_ops::{OpError, OpErrorKind};

    let mut graph = Graph::new();
    graph.add_node(node("login", "behavior"));
    graph.add_node(node("signin", "feature"));
    let project = Assembled::over(Default::default()).graph(graph);
    let view = project.view();
    let navigator = Navigator::new(view);

    let refusals: Vec<(&str, OpError)> = vec![
        (
            "export scope",
            export(
                &view,
                &Request {
                    scope: Some("logn"),
                    ..Request::default()
                },
            )
            .unwrap_err()
            .error,
        ),
        (
            "inspect",
            specforge_ops::read::inspect::inspect(&view, "logn")
                .unwrap_err()
                .error,
        ),
        (
            "explore",
            specforge_ops::read::explore::explore(
                &view,
                &specforge_ops::read::explore::ExplorationRequest {
                    entity_id: Some("logn"),
                    ..Default::default()
                },
            )
            .unwrap_err()
            .error,
        ),
        (
            "review",
            specforge_ops::read::review::review(
                &view,
                &specforge_ops::read::review::ReviewRequest {
                    entity_id: Some("logn"),
                    ..Default::default()
                },
            )
            .unwrap_err()
            .error,
        ),
        ("navigator", navigator.definition("logn").unwrap_err().error),
        (
            "rename",
            specforge_ops::authoring::rename::plan(&navigator, "logn", "fine_name")
                .unwrap_err()
                .error,
        ),
        (
            "trace",
            OpError::from(
                specforge_ops::read::trace::trace(
                    &view,
                    specforge_ops::read::trace::Target::Entity("logn"),
                )
                .unwrap_err(),
            ),
        ),
    ];
    for (surface, error) in &refusals {
        assert_eq!(error.kind, OpErrorKind::EntityNotFound, "{surface}");
        assert_eq!(error.code, "E003", "{surface}");
        assert_eq!(
            error.message, "unresolved entity 'logn' — not found in graph",
            "{surface}"
        );
        assert_eq!(
            error.suggestion.as_deref(),
            Some("did you mean 'login'?"),
            "{surface}"
        );
        assert_eq!(error.entity.as_deref(), Some("logn"), "{surface}");
    }
}

/// A project that enables nothing, to add a registry package to.
fn empty_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    dir
}

fn add_package(
    dir: &std::path::Path,
    registry: &specforge_ops::registry::testing::MemoryRegistry,
    specifier: &str,
) -> specforge_ops::Refusal<specforge_ops::extension::AddField> {
    use specforge_ops::asking::Asking;
    use specforge_ops::extension::{self, AddRequest};
    extension::add(
        &AddRequest {
            root: &specforge_common::ProjectRoot::of(dir).unwrap(),
            extensions: vec![specifier.to_string()],
            base: None,
            allow_unsigned: false,
            asking: Asking::Nobody,
            dry_run: false,
            containment: &specforge_ops::Containment::unbounded(),
        },
        registry,
        &specforge_ops::testing::candidates(),
    )
    .unwrap_err()
}

#[specforge_test_macros::test(
    invariant = "mcp_structured_error_responses",
    verify = "a package the registry or the trust check refuses is never an internal_error"
)]
fn a_registry_refusal_is_never_an_internal_error() {
    use specforge_ops::OpErrorKind;
    use specforge_ops::asking::Asking;
    use specforge_ops::registry::testing::{MemoryRegistry, Published, declaration};
    use specforge_ops::registry::{Registry, TrustPolicy};
    use specforge_package::{PackageName, Version};

    let registry = MemoryRegistry::new()
        .serving(Published::new(
            b"bytes".to_vec(),
            declaration("@acme/unsigned", "1.0.0", &[]),
        ))
        .listing_none("@acme/empty");
    let dir = empty_project();

    let unsigned = add_package(dir.path(), &registry, "@acme/unsigned");
    assert_eq!(unsigned.error.code, "R-TRUST-001", "{unsigned:?}");
    assert_eq!(unsigned.error.kind, OpErrorKind::PermissionDenied);

    let empty = add_package(dir.path(), &registry, "@acme/empty");
    assert_eq!(empty.error.code, "R-RES-002", "{empty:?}");
    assert_eq!(empty.error.kind, OpErrorKind::ExtensionNotFound);

    let missing = registry
        .fetch(
            &PackageName::parse("@acme/unsigned").unwrap(),
            &Version::parse("9.9.9").unwrap(),
            TrustPolicy::new(true, Asking::Nobody),
        )
        .unwrap_err();
    assert_eq!(missing.code, "R006", "{missing:?}");
    assert_eq!(missing.kind, OpErrorKind::ExtensionNotFound);
}
