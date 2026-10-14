//! The emitter's failures are typed: each variant carries what its message
//! names, `code()` is the catalogued diagnostic it is, and the message
//! carries no code (plan 05 T1, ADR 0015 "Query" Q4).

use specforge_common::{SourceSpan, Sym};
use specforge_diagnostics::codes;
use specforge_emitter::{EmitOptions, EmitterError};
use specforge_graph::{Edge, Graph, Node};
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

// A scoped emit of a non-existent entity is ScopeNotFound, whose code is E003.
#[test]
fn emit_nonexistent_scope_is_scope_not_found() {
    let graph = build_graph();
    let err = specforge_emitter::emit(
        &graph,
        &EmitOptions {
            scope: Some("nonexistent"),
            depth: Some(1),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        err,
        EmitterError::ScopeNotFound {
            entity_id: "nonexistent".to_string()
        }
    );
    assert_eq!(err.code(), Some(codes::E003));
}

#[test]
fn emit_scoped_nonexistent_returns_scope_not_found() {
    let graph = build_graph();
    let err = crate::support::scoped_json(&graph, "nonexistent").unwrap_err();
    assert!(
        matches!(err, EmitterError::ScopeNotFound { .. }),
        "expected ScopeNotFound, got: {err:?}"
    );
}

#[specforge_test_macros::test(
    behavior = "export_agent_graph_format",
    verify = "an export failure carries its code as a constant, never in its message"
)]
fn a_failure_carries_its_code_and_its_message_has_none() {
    let err = EmitterError::ScopeNotFound {
        entity_id: "foo".to_string(),
    };
    assert_eq!(err.code(), Some(codes::E003));
    assert_eq!(
        err.to_string(),
        "unresolved entity 'foo' — not found in graph"
    );

    let err = EmitterError::BudgetTooSmall {
        reason: "the budget of 5 cannot hold an export".to_string(),
    };
    assert_eq!(err.code(), Some(codes::E062));
    assert_eq!(err.to_string(), "the budget of 5 cannot hold an export");

    let err = EmitterError::Serialization("bad data".to_string());
    assert_eq!(err.code(), None);
    assert_eq!(err.to_string(), "bad data");
}

// A budget that cannot hold even an export with no entities is BudgetTooSmall,
// whose code is E062 (the one budget failure there is: the strategy is
// `prioritize`).
#[test]
fn a_budget_too_small_for_the_empty_export_is_budget_too_small() {
    let graph = build_graph();
    let err = specforge_emitter::emit(
        &graph,
        &EmitOptions {
            token_budget: Some(1),
            ..Default::default()
        },
    )
    .expect_err("a budget of one token cannot hold the envelope");
    assert!(
        matches!(err, EmitterError::BudgetTooSmall { .. }),
        "expected BudgetTooSmall, got: {err:?}"
    );
    assert_eq!(err.code(), Some(codes::E062));
}

#[test]
fn emitter_error_implements_std_error() {
    let err = EmitterError::Serialization("test".to_string());
    let _: &dyn std::error::Error = &err;
}
