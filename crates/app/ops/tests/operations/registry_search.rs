//! `specforge search`'s document: the shape every command writes a diagnostic in, and no `null` (ADR 0052 D1).

use specforge_common::{Diagnostic, DiagnosticList, codes};
use specforge_ops::registry::{Found, Searched};
use specforge_protocol_types::DeclaredCategory;
use specforge_test_macros::test as specforge_test;

fn searched() -> Searched {
    Searched {
        found: vec![Found {
            name: "@acme/x".into(),
            version: "1.0.0".into(),
            description: "d".into(),
            registry: "local".into(),
        }],
        failures: vec![Diagnostic::new(codes::R005, "down")],
        asked: 2,
    }
}

#[specforge_test(
    behavior = "search_registry",
    verify = "search's document writes its diagnostics as check does and carries no null"
)]
fn the_search_document_leaves_out_an_absent_category() {
    let searched = searched();

    let without = serde_json::to_value(searched.document("x", None)).unwrap();
    let with = serde_json::to_value(searched.document("x", Some(DeclaredCategory::Edges))).unwrap();

    assert!(without.get("contributes").is_none(), "{without}");
    assert_eq!(with["contributes"], "edges");
    assert_eq!(with["results"][0]["registry"], "local");
}

#[specforge_test(
    behavior = "search_registry",
    verify = "search's document writes its diagnostics as check does and carries no null"
)]
fn the_search_document_writes_failures_in_the_shared_diagnostic_shape() {
    let searched = searched();

    let json = serde_json::to_value(searched.document("x", None)).unwrap();

    let shared = serde_json::to_value(DiagnosticList(searched.failures.clone())).unwrap();
    assert_eq!(json["diagnostics"], shared);
    assert!(json["diagnostics"][0].get("title").is_some(), "{json}");
}
