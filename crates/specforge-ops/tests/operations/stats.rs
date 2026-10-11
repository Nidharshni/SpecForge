use specforge_common::{Severity, SourceSpan, Sym};
use specforge_graph::{Edge, Graph, Node};
use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue, VerifyStatement};
use specforge_test::prelude::*;

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
        title: None,
        fields: FieldMap::new(),
        source_span: span(),
        methods: Vec::new(),
    }
}

fn node_with_verify(id: &str, kind: &str) -> Node {
    let mut fields = FieldMap::new();
    fields.push(
        Sym::new("verify"),
        FieldValue::VerifyList(vec![VerifyStatement {
            kind: "unit".to_string(),
            description: "it works".to_string(),
        }]),
    );
    Node {
        id: EntityId { raw: Sym::new(id) },
        kind: EntityKind {
            raw: Sym::new(kind),
        },
        title: None,
        fields,
        source_span: span(),
        methods: Vec::new(),
    }
}

// B:compute_project_statistics — verify unit "stats reports correct entity counts"
#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "stats reports correct entity counts"
)]
fn stats_reports_correct_entity_counts() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "behavior"));
    graph.add_node(node("b", "behavior"));
    graph.add_node(node("c", "feature"));

    let stats = crate::view_support::stats_of(&graph, &[], &[]);
    assert_eq!(stats.entity_count, 3);
    let count_of = |kind: &str| {
        stats
            .entity_counts
            .iter()
            .find(|row| row.kind == kind)
            .map(|row| row.count)
    };
    assert_eq!(count_of("behavior"), Some(2));
    assert_eq!(count_of("feature"), Some(1));
}

// B:compute_project_statistics — verify unit "stats reports correct entity counts"
// (covers edge count reporting)
#[specforge_test(behavior = "compute_project_statistics")]
fn stats_reports_edge_count() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature"));
    graph.add_node(node("b", "behavior"));
    graph.add_edge(Edge {
        source: Sym::new("a"),
        target: Sym::new("b"),
        label: Sym::new("behaviors"),
    });

    let stats = crate::view_support::stats_of(&graph, &[], &[]);
    assert_eq!(stats.edge_count, 1);
}

// B:compute_project_statistics — verify unit "stats reports the unconnected entity count"
#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "stats reports the unconnected entity count"
)]
fn stats_reports_the_unconnected_count() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "behavior")); // unconnected: no edges
    graph.add_node(node("b", "feature"));
    graph.add_node(node("c", "behavior"));
    graph.add_node(node("s", "behavior")); // unconnected: it references only itself
    graph.add_edge(Edge {
        source: Sym::new("b"),
        target: Sym::new("c"),
        label: Sym::new("behaviors"),
    });
    graph.add_edge(Edge {
        source: Sym::new("s"),
        target: Sym::new("s"),
        label: Sym::new("depends_on"),
    });

    let stats = crate::view_support::stats_of(&graph, &[], &[]);
    assert_eq!(stats.unconnected_count, 2); // "a" and "s"
}

// B:compute_project_statistics — verify unit "stats reports coverage percentage"
// (covers verified entity counting for coverage computation)
#[specforge_test(behavior = "compute_project_statistics")]
fn stats_reports_verified_count() {
    let mut graph = Graph::new();
    graph.add_node(node_with_verify("a", "behavior"));
    graph.add_node(node("b", "behavior")); // no verify

    let stats = crate::view_support::stats_of(&graph, &[], &[]);
    assert_eq!(stats.verified_count, 1);
}

// B:compute_project_statistics — verify unit "stats reports correct entity counts"
// (edge case: empty graph)
#[specforge_test(behavior = "compute_project_statistics")]
fn stats_on_empty_graph() {
    let graph = Graph::new();
    let stats = crate::view_support::stats_of(&graph, &[], &[]);
    assert_eq!(stats.entity_count, 0);
    assert_eq!(stats.edge_count, 0);
    assert_eq!(stats.unconnected_count, 0);
    assert_eq!(stats.verified_count, 0);
}

// B:compute_project_statistics — verify unit "stats reports coverage percentage"
#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "stats reports coverage percentage"
)]
fn stats_coverage_over_the_testable_kinds() {
    let mut graph = Graph::new();
    graph.add_node(node_with_verify("a", "behavior")); // testable + verified
    graph.add_node(node("b", "behavior")); // testable, not verified
    graph.add_node(node("c", "feature")); // not testable

    let testable = &["behavior"];
    let stats = crate::view_support::stats_of(&graph, testable, &[]);
    // 1 verified out of 2 testable = 50%
    assert_eq!(stats.testable_count, 2);
    assert_eq!(stats.verified_count, 1);
    assert!((stats.declared_pct - 50.0).abs() < 0.01);
}

// B:compute_project_statistics — verify unit "coverage is 0% when testable_entity_count is zero"
#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "coverage is 0% when testable_entity_count is zero"
)]
fn stats_coverage_zero_when_no_testable_entities() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature")); // not testable

    let testable: &[&str] = &["behavior"]; // no behaviors in graph
    let stats = crate::view_support::stats_of(&graph, testable, &[]);
    assert_eq!(stats.testable_count, 0);
    assert_eq!(stats.declared_pct, 0.0);
}

// B:compute_project_statistics — verify unit "stats reports diagnostic summary"
#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "stats reports diagnostic summary"
)]
fn stats_includes_diagnostic_summary() {
    let graph = Graph::new();
    let diagnostics = vec![
        specforge_common::Diagnostic::new(specforge_common::codes::E001, "bad ref".to_string()),
        specforge_common::Diagnostic::new(specforge_common::codes::W012, "orphan".to_string()),
    ];
    let stats = crate::view_support::stats_of(&graph, &[], &diagnostics);
    assert_eq!(stats.diagnostic_summary.errors, 1);
    assert_eq!(stats.diagnostic_summary.warnings, 1);
    assert_eq!(stats.diagnostic_summary.infos, 0);
}

// A type may declare a struct member named `verify`; the entity's
// obligations are its verify statements wherever they sit among its fields,
// so the member must not hide them (plan 02, S2).
#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "stats reports coverage percentage"
)]
fn stats_counts_obligations_behind_a_verify_member() {
    let mut payload = node_with_verify("Payload", "type");
    let mut fields = FieldMap::new();
    fields.push(Sym::new("verify"), FieldValue::Identifier("string".into()));
    for entry in payload.fields.entries() {
        fields.push(entry.key, entry.value.clone());
    }
    payload.fields = fields;
    let mut graph = Graph::new();
    graph.add_node(payload);
    graph.add_node(node("Status", "type"));

    let stats = crate::view_support::stats_of(&graph, &["type"], &[]);
    assert_eq!(stats.testable_count, 2);
    assert_eq!(stats.verified_count, 1, "Payload declares an obligation");
    assert!((stats.declared_pct - 50.0).abs() < 0.01);
}

#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "stats leaves the entities W004 exempts out of the testable count"
)]
fn stats_leaves_union_types_out_of_the_testable_count() {
    // `type Status = active | inactive`: no body to hold obligations.
    let mut status = node("Status", "type");
    status.fields.push(
        Sym::new("variants"),
        FieldValue::VariantList(vec!["active".into(), "inactive".into()]),
    );
    // A struct member that is only named `variants` exempts nothing.
    let mut named = node("Named", "type");
    named.fields.push(
        Sym::new("variants"),
        FieldValue::Identifier("string".into()),
    );
    let mut graph = Graph::new();
    graph.add_node(status);
    graph.add_node(named);
    graph.add_node(node_with_verify("Payload", "type"));

    let stats = crate::view_support::stats_of(&graph, &["type"], &[]);
    assert_eq!(stats.testable_count, 2, "Named and Payload");
    assert!((stats.declared_pct - 50.0).abs() < 0.01);
}

/// The contracts graph (a feature listing b, b depending on c, both
/// verified), plus an unverified behavior and a verified feature.
fn registries_graph() -> Graph {
    let mut graph = Graph::new();
    graph.add_node(node("a", "feature"));
    graph.add_node(node_with_verify("b", "behavior"));
    graph.add_node(node_with_verify("c", "behavior"));
    graph.add_node(node("d", "behavior"));
    graph.add_node(node_with_verify("e", "feature"));
    graph.add_edge(Edge {
        source: Sym::new("a"),
        target: Sym::new("b"),
        label: Sym::new("behaviors"),
    });
    graph.add_edge(Edge {
        source: Sym::new("b"),
        target: Sym::new("c"),
        label: Sym::new("depends_on"),
    });
    graph
}

/// `{:#?}` of the stats, less the type's name.
const STATS_TODAY: &str = r#"{
    entity_count: 5,
    entity_counts: [
        KindCount {
            kind: "behavior",
            count: 3,
        },
        KindCount {
            kind: "feature",
            count: 2,
        },
    ],
    edge_count: 2,
    unconnected_count: 2,
    testable_count: 3,
    declared_count: 2,
    verified_count: 3,
    declared_pct: 66.66666666666666,
    proof_pct: None,
    diagnostic_summary: Counts {
        errors: 1,
        warnings: 1,
        infos: 0,
    },
}"#;

/// `stats` as `{:#?}` prints it, from its opening brace.
fn debug_body(stats: &impl std::fmt::Debug) -> String {
    let text = format!("{stats:#?}");
    text[text.find('{').unwrap()..].to_string()
}

// Pins what the surfaces' stats path computed before plan 02 T3 (the
// coverage rule over the registries, then the tally): moved behind the
// stats operation, it must stay byte-identical. No spec obligation.
#[test]
fn stats_today_through_the_registries() {
    let project = crate::view_support::Project::of_graph(
        registries_graph(),
        crate::view_support::registries(&["behavior"], &[]),
    );
    let diagnostics = vec![
        specforge_common::Diagnostic::new(specforge_common::codes::E001, "err"),
        specforge_common::Diagnostic::untyped("W002", Severity::Warning, "warn"),
    ];
    let stats = specforge_ops::stats::stats(&project.view().reporting(&diagnostics)).unwrap();
    assert_eq!(debug_body(&stats), STATS_TODAY);
}

// B:compute_project_statistics — verify unit "the stats document has no proof_pct without recorded test results"
#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "the stats document has no proof_pct without recorded test results"
)]
fn stats_document_leaves_proof_pct_out_without_results() {
    let mut graph = Graph::new();
    graph.add_node(node("a", "behavior"));
    graph.add_node(node("b", "behavior"));

    let stats = crate::view_support::stats_of(&graph, &["behavior"], &[]);
    let document = serde_json::to_value(&stats).unwrap();

    assert!(document.get("proof_pct").is_none(), "{document}");
    assert_eq!(
        document["entity_counts"],
        serde_json::json!([{"kind": "behavior", "count": 2}])
    );
    assert_eq!(
        document["diagnostic_summary"],
        serde_json::json!({"errors": 0, "warnings": 0, "infos": 0})
    );
}
