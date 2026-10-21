//! The migration crate's pure comparison: the graph a migration must keep
//! ([`structural_differences`]).
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_common::{Diagnostic, Sym};
use specforge_graph::{Graph, compute_graph_delta};
use specforge_migrate::structural_differences;
use specforge_test_macros::test as specforge_test;

/// The W054s for a migration that turned `pre` into `post`.
fn drift(pre: &Graph, post: &Graph) -> Vec<Diagnostic> {
    structural_differences(&compute_graph_delta(pre, post))
}

// F2: Post-migration structural equivalence (format-only migration)
#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "structural equivalence verified between pre and post graphs"
)]
fn format_only_migration_zero_differences() {
    use specforge_graph::{Edge, EntityId, EntityKind, FieldMap, FieldValue, Graph, Node};
    use specforge_parser::{ListItem, Written};

    // A graph as compiled from files whose lines start at `first_line`:
    // a format migration adds a header line, shifting every span.
    let build = |first_line: usize, contract: &str, extra_edge: bool| {
        let span = |line: usize| specforge_graph::SourceSpan {
            file: Sym::new("spec/test.spec"),
            start_line: line,
            start_col: 0,
            end_line: line,
            end_col: 10,
        };
        let mut graph = Graph::new();
        let mut fields = FieldMap::new();
        fields.push(Sym::new("contract"), FieldValue::String(contract.into()));
        fields.push(
            Sym::new("invariants"),
            FieldValue::List(vec![ListItem {
                text: "inv_a".into(),
                span: span(first_line + 2),
                written: Written::Bare,
            }]),
        );
        graph.add_node(Node {
            id: EntityId {
                raw: Sym::new("beh_a"),
            },
            kind: EntityKind {
                raw: Sym::new("behavior"),
            },
            title: Some("A".into()),
            source_span: span(first_line),
            fields,
            methods: Vec::new(),
            obligations: Vec::new(),
        });
        for (id, kind) in [("inv_a", "invariant"), ("inv_b", "invariant")] {
            graph.add_node(Node {
                id: EntityId { raw: Sym::new(id) },
                kind: EntityKind {
                    raw: Sym::new(kind),
                },
                title: None,
                source_span: span(first_line + 5),
                fields: FieldMap::new(),
                methods: Vec::new(),
                obligations: Vec::new(),
            });
        }
        graph.add_edge(Edge {
            source: Sym::new("beh_a"),
            target: Sym::new("inv_a"),
            label: Sym::new("invariants"),
        });
        if extra_edge {
            graph.add_edge(Edge {
                source: Sym::new("beh_a"),
                target: Sym::new("inv_b"),
                label: Sym::new("invariants"),
            });
        }
        graph
    };

    // Same entities, edges and field values; only the spans moved.
    let pre = build(1, "does stuff", false);
    let post = build(2, "does stuff", false);
    let diagnostics = drift(&pre, &post);
    assert!(
        diagnostics.is_empty(),
        "shifted spans are not a structural difference: {diagnostics:?}"
    );

    // A changed field value is a difference.
    let changed = build(2, "does other stuff", false);
    let diagnostics = drift(&pre, &changed);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "W054");
    assert_eq!(
        diagnostics[0].message,
        "entity 'beh_a' changed during migration: contract"
    );

    // So is an edge that appeared.
    let grown = build(2, "does stuff", true);
    let diagnostics = drift(&pre, &grown);
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "entity 'beh_a' changed during migration: edges",
            "edge beh_a-[invariants]->inv_b appeared after migration but was not present before",
        ]
    );
    assert!(diagnostics.iter().all(|d| d.code == "W054"));
}

// F3: Structural differences → warnings
#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "structural differences reported as warnings"
)]
fn structural_differences_produce_warnings() {
    use specforge_graph::{EntityId, EntityKind, FieldMap, Graph, Node, SourceSpan};

    let mut pre = Graph::new();
    pre.add_node(Node {
        id: EntityId {
            raw: Sym::new("foo"),
        },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: None,
        source_span: SourceSpan {
            file: Sym::new("test.spec"),
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 0,
        },
        fields: FieldMap::new(),
        methods: Vec::new(),
        obligations: Vec::new(),
    });

    let post = Graph::new(); // Empty graph — entity "foo" is missing

    let diagnostics = drift(&pre, &post);
    assert!(
        !diagnostics.is_empty(),
        "missing entity should produce diagnostic"
    );
    assert!(
        diagnostics.iter().any(|d| d.code == "W054"),
        "should emit W054: {diagnostics:?}"
    );
}

#[test]
fn new_entities_after_migration_reported() {
    use specforge_graph::{EntityId, EntityKind, FieldMap, Graph, Node, SourceSpan};

    let pre = Graph::new(); // empty before

    let mut post = Graph::new();
    post.add_node(Node {
        id: EntityId {
            raw: Sym::new("new_entity"),
        },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: None,
        source_span: SourceSpan {
            file: Sym::new("test.spec"),
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 0,
        },
        fields: FieldMap::new(),
        methods: Vec::new(),
        obligations: Vec::new(),
    });

    let diags = drift(&pre, &post);
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("new_entity") && d.message.contains("appeared")),
        "new entity should be reported: {diags:?}"
    );
}

#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "Validate Post-Migration Integrity: post-migration integrity validation holds — extension_hooks_complete_fired, structural_equivalence_checked, differences_reported, migration_validation_complete_emitted"
)]
fn post_migration_integrity_contract() {
    use specforge_graph::{Edge, EntityId, EntityKind, FieldMap, Graph, Node, SourceSpan};

    let make_node = |id: &str| Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new("behavior"),
        },
        title: None,
        source_span: SourceSpan {
            file: Sym::new("t.spec"),
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 0,
        },
        fields: FieldMap::new(),
        methods: Vec::new(),
        obligations: Vec::new(),
    };

    // Ensures: structural_equivalence_checked — identical graphs yield empty
    let g = Graph::new();
    assert!(drift(&g, &g).is_empty());

    // Ensures: differences_reported — missing entity yields W054
    let mut pre = Graph::new();
    pre.add_node(make_node("x"));
    let post = Graph::new();
    let diags = drift(&pre, &post);
    assert!(
        diags.iter().any(|d| d.code == "W054"),
        "missing entity → W054"
    );

    // Ensures: edge differences reported
    let mut pre2 = Graph::new();
    pre2.add_node(make_node("a"));
    pre2.add_node(make_node("b"));
    pre2.add_edge(Edge {
        source: Sym::new("a"),
        target: Sym::new("b"),
        label: Sym::new("refs"),
    });
    let mut post2 = Graph::new();
    post2.add_node(make_node("a"));
    post2.add_node(make_node("b"));
    let diags2 = drift(&pre2, &post2);
    assert!(
        diags2
            .iter()
            .any(|d| d.code == "W054" && d.message.contains("edge")),
        "missing edge → W054: {diags2:?}"
    );
}

#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "structural differences reported as warnings"
)]
fn cross_extension_broken_reference_produces_diagnostic() {
    use specforge_graph::{Edge, EntityId, EntityKind, FieldMap, Graph, Node, SourceSpan};

    let make_node = |id: &str, kind: &str| Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: None,
        source_span: SourceSpan {
            file: Sym::new("t.spec"),
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 0,
        },
        fields: FieldMap::new(),
        methods: Vec::new(),
        obligations: Vec::new(),
    };

    // Pre: behavior→feature cross-extension edge exists
    let mut pre = Graph::new();
    pre.add_node(make_node("login_flow", "behavior"));
    pre.add_node(make_node("user_auth", "feature"));
    pre.add_edge(Edge {
        source: Sym::new("login_flow"),
        target: Sym::new("user_auth"),
        label: Sym::new("implements"),
    });

    // Post: feature was removed (broken cross-extension reference)
    let mut post = Graph::new();
    post.add_node(make_node("login_flow", "behavior"));

    let diags = drift(&pre, &post);
    // Should report both the missing entity and the missing edge
    assert!(
        diags.iter().any(|d| d.message.contains("user_auth")),
        "missing cross-extension entity → diagnostic: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.message.contains("edge")),
        "missing cross-extension edge → diagnostic: {diags:?}"
    );
}

/// One way a migration can leave the graph, for the table below.
#[derive(Clone, Copy)]
enum Change {
    Moved,
    Title,
    Kind,
    Methods,
    Field,
    EdgeRemoved,
    NodeRemoved,
    NodeAdded,
}

/// The graph `alpha` (a behavior titled `Alpha`, `contract "x"`, method `run()`, an edge to `beta`) and `beta`,
/// changed by `change`.
fn graph_after(change: Option<Change>) -> Graph {
    use specforge_graph::{Edge, EntityId, EntityKind, FieldMap, FieldValue, Node, SourceSpan};
    use specforge_parser::MethodDecl;

    let shift = usize::from(matches!(change, Some(Change::Moved)));
    let span = |line: usize| SourceSpan {
        file: Sym::new("spec/a.spec"),
        start_line: line + shift,
        start_col: 0,
        end_line: line + shift,
        end_col: 10,
    };
    let node = |id: &str, kind: &str, title: Option<&str>, contract: &str, methods: bool, line| {
        let mut fields = FieldMap::new();
        fields.push(Sym::new("contract"), FieldValue::String(contract.into()));
        Node {
            id: EntityId { raw: Sym::new(id) },
            kind: EntityKind {
                raw: Sym::new(kind),
            },
            title: title.map(Into::into),
            source_span: span(line),
            fields,
            methods: if methods {
                vec![MethodDecl {
                    name: "run".into(),
                    name_span: None,
                    params: Vec::new(),
                    returns: None,
                    returns_span: None,
                    span: span(line + 1),
                }]
            } else {
                Vec::new()
            },
            obligations: Vec::new(),
        }
    };
    let mut graph = Graph::new();
    graph.add_node(node(
        "alpha",
        if matches!(change, Some(Change::Kind)) {
            "feature"
        } else {
            "behavior"
        },
        Some(if matches!(change, Some(Change::Title)) {
            "Renamed"
        } else {
            "Alpha"
        }),
        if matches!(change, Some(Change::Field)) {
            "y"
        } else {
            "x"
        },
        !matches!(change, Some(Change::Methods)),
        1,
    ));
    if !matches!(change, Some(Change::NodeRemoved)) {
        graph.add_node(node("beta", "behavior", None, "b", false, 5));
    }
    if matches!(change, Some(Change::NodeAdded)) {
        graph.add_node(node("gamma", "behavior", None, "g", false, 9));
    }
    if !matches!(change, Some(Change::EdgeRemoved | Change::NodeRemoved)) {
        graph.add_edge(Edge {
            source: Sym::new("alpha"),
            target: Sym::new("beta"),
            label: Sym::new("refs"),
        });
    }
    graph
}

#[specforge_test(
    invariant = "migration_semantic_preservation",
    verify = "pre-migration and post-migration entity graphs are structurally identical"
)]
#[specforge_test(
    behavior = "validate_post_migration_integrity",
    verify = "each change the graph delta names is one W054 warning, and an empty delta none"
)]
fn every_change_the_delta_names_is_one_w054_and_a_move_is_none() {
    let edge_gone = "edge alpha-[refs]->beta present before migration but missing after";
    let cases: [(Change, Vec<&str>); 8] = [
        (Change::Moved, vec![]),
        (
            Change::Title,
            vec!["entity 'alpha' changed during migration: title"],
        ),
        (
            Change::Kind,
            vec!["entity 'alpha' changed during migration: kind"],
        ),
        (
            Change::Methods,
            vec!["entity 'alpha' changed during migration: methods"],
        ),
        (
            Change::Field,
            vec!["entity 'alpha' changed during migration: contract"],
        ),
        (
            Change::EdgeRemoved,
            vec!["entity 'alpha' changed during migration: edges", edge_gone],
        ),
        (
            Change::NodeRemoved,
            vec![
                "entity 'beta' (behavior) present before migration but missing after",
                "entity 'alpha' changed during migration: edges",
                edge_gone,
            ],
        ),
        (
            Change::NodeAdded,
            vec!["entity 'gamma' (behavior) appeared after migration but was not present before"],
        ),
    ];
    let pre = graph_after(None);
    for (change, expected) in cases {
        let post = graph_after(Some(change));
        let diagnostics = drift(&pre, &post);

        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages, expected);
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code == "W054" && d.severity == specforge_common::Severity::Warning),
            "{diagnostics:?}"
        );
        assert_eq!(
            messages.is_empty(),
            compute_graph_delta(&pre, &post).is_empty()
        );
    }
}
