use super::{layered, reversed};
use crate::tests::host::*;
use serde_json::json;
use specforge_test::prelude::*;

/// Deliverables over `depends_on`: d2 and d3 on d1, d4 on d2.
fn deliverable_chain() -> G {
    G::default()
        .n("d1", "deliverable")
        .n("d2", "deliverable")
        .n("d3", "deliverable")
        .n("d4", "deliverable")
        .n("f1", "feature")
        .edge("d2", "d1", "depends_on")
        .edge("d3", "d1", "depends_on")
        .edge("d4", "d2", "depends_on")
        // A feature's depends_on on a deliverable is not a dependent.
        .edge("f1", "d1", "depends_on")
}

#[specforge_test(
    behavior = "surface_deliverable_dependents",
    verify = "deliverable-dependents returns DeliverableDependentPayload JSON"
)]
fn deliverable_dependents_answers_its_payload() {
    let dd = json_of(
        "deliverable_dependents",
        json!({"deliverable": "d1"}),
        &deliverable_chain(),
    );
    assert_eq!(
        dd,
        json!({"deliverable_id": "d1", "dependents": ["d2", "d3"], "count": 2})
    );
}

#[specforge_test(
    behavior = "surface_deliverable_dependents",
    verify = "missing deliverable ID returns error with suggestion"
)]
fn deliverable_dependents_of_a_mistyped_deliverable_suggests_the_nearest() {
    let error = not_found_suggesting(
        "deliverable_dependents",
        json!({"deliverable": "d5"}),
        &deliverable_chain(),
    );
    assert_eq!(error["suggestion"], "d1");
}

#[specforge_test(
    behavior = "pe_query_deliverable_dependents",
    verify = "deliverable with dependent returns that dependent"
)]
fn a_deliverable_depended_on_once_has_that_dependent() {
    let dd = json_of(
        "deliverable_dependents",
        json!({"deliverable": "d2"}),
        &deliverable_chain(),
    );
    assert_eq!(dd["dependents"], json!(["d4"]));
}

#[specforge_test(
    behavior = "pe_query_deliverable_dependents",
    verify = "deliverable with multiple dependents returns all sorted by ID"
)]
fn a_deliverables_dependents_are_sorted_by_id() {
    let dd = json_of(
        "deliverable_dependents",
        json!({"deliverable": "d1"}),
        &reversed(deliverable_chain()),
    );
    assert_eq!(dd["dependents"], json!(["d2", "d3"]));
}

#[specforge_test(
    behavior = "pe_query_deliverable_dependents",
    verify = "deliverable with no dependents returns empty list"
)]
fn a_deliverable_nothing_depends_on_has_no_dependents() {
    let dd = json_of(
        "deliverable_dependents",
        json!({"deliverable": "d4"}),
        &deliverable_chain(),
    );
    assert_eq!(
        dd,
        json!({"deliverable_id": "d4", "dependents": [], "count": 0})
    );
}

#[specforge_test(
    behavior = "product_query_correctness",
    verify = "reverse queries return exact inverse of forward queries"
)]
fn dependents_are_exactly_the_depends_on_references_reversed() {
    let g = deliverable_chain();
    for d in ["d1", "d2", "d3", "d4"] {
        let dd = json_of("deliverable_dependents", json!({ "deliverable": d }), &g);
        let mut expected: Vec<&str> = g
            .edges
            .iter()
            .filter(|e| e["label"] == "depends_on" && e["target"] == d)
            .map(|e| e["source"].as_str().unwrap())
            .filter(|s| s.starts_with('d'))
            .collect();
        expected.sort_unstable();
        assert_eq!(dd["dependents"], json!(expected), "{d}");
    }
    let g = layered();
    for f in ["base", "m_high", "top"] {
        let fd = json_of("feature_dependents", json!({ "feature": f }), &g);
        let mut expected: Vec<&str> = g
            .edges
            .iter()
            .filter(|e| e["label"] == "depends_on" && e["target"] == f)
            .map(|e| e["source"].as_str().unwrap())
            .collect();
        expected.sort_unstable();
        assert_eq!(fd["dependents"], json!(expected), "{f}");
    }
}
