//! W012: a `ref` nothing references. Refs are grammar-level, so this check
//! runs with or without an extension loaded.

use specforge_common::Severity;
use specforge_registry::entity::{Direction, EntityRecord};
use specforge_test_macros::test as spec;

use crate::support::{build, check, coded_in, software, span};

fn w012(records: &[EntityRecord]) -> Vec<specforge_common::Diagnostic> {
    let diags = check(&build([software()]), records);
    coded_in(&diags, "W012").into_iter().cloned().collect()
}

#[spec(
    behavior = "detect_unreferenced_refs",
    verify = "unreferenced ref produces W012"
)]
fn an_unreferenced_ref_produces_w012() {
    let warnings = w012(&[
        EntityRecord::new("behavior", "alpha", span("main.spec")),
        EntityRecord::new("ref", "gh.issue:42", span("main.spec")),
    ]);
    assert_eq!(warnings.len(), 1, "orphan ref should produce W012");
    assert!(warnings[0].message.contains("gh.issue:42"));
    assert_eq!(warnings[0].severity, Severity::Warning);
    assert!(warnings[0].span.is_some());
}

#[spec(
    behavior = "detect_unreferenced_refs",
    verify = "referenced ref suppresses W012"
)]
fn a_referenced_ref_suppresses_w012() {
    let warnings = w012(&[
        EntityRecord::new("behavior", "alpha", span("main.spec")),
        EntityRecord::new("ref", "gh.issue:42", span("main.spec")).with_edges(
            Direction::Incoming,
            Some("References"),
            1,
        ),
    ]);
    assert!(
        warnings.is_empty(),
        "referenced ref should not produce W012"
    );
}

#[spec(
    behavior = "detect_unreferenced_refs",
    verify = "spec block is a root container and does not produce W012"
)]
fn a_spec_block_does_not_produce_w012() {
    // A spec block is the project's root container: nothing references it.
    let warnings = w012(&[
        EntityRecord::new("spec", "MyProject", span("main.spec")),
        EntityRecord::new("behavior", "alpha", span("main.spec")),
    ]);
    assert!(
        warnings.is_empty(),
        "spec block should not produce W012, got: {warnings:?}"
    );
}

#[test]
fn a_non_structural_kind_does_not_produce_w012() {
    // Extension kinds' orphan detection is a `no_incoming_edges` rule of the
    // extension, not core's.
    let warnings = w012(&[EntityRecord::new("behavior", "alpha", span("main.spec"))]);
    assert!(warnings.is_empty());
}

#[test]
fn w012_runs_with_no_extension_loaded() {
    let diags = check(
        &build([]),
        &[EntityRecord::new("ref", "gh.issue:42", span("main.spec"))],
    );
    assert_eq!(coded_in(&diags, "W012").len(), 1, "{diags:?}");
}

#[spec(
    behavior = "detect_unreferenced_refs",
    verify = "Detect Unreferenced Refs: unreferenced ref detection holds — graph_built_fired, unreferenced_detected, referenced_nodes_clean"
)]
fn the_orphan_refs_contract_holds() {
    // unreferenced_detected: a ref with zero incoming edges → W012.
    let orphan = w012(&[
        EntityRecord::new("behavior", "alpha", span("main.spec")),
        EntityRecord::new("ref", "gh.issue:42", span("main.spec")),
    ]);
    assert_eq!(orphan.len(), 1, "an orphan ref must produce W012");

    // referenced_nodes_clean: with an incoming edge → no W012.
    let linked = w012(&[
        EntityRecord::new("behavior", "alpha", span("main.spec")),
        EntityRecord::new("ref", "gh.issue:42", span("main.spec")).with_edges(
            Direction::Incoming,
            Some("References"),
            1,
        ),
    ]);
    assert!(linked.is_empty(), "a referenced ref must not produce W012");
}
