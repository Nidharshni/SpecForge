//! A reference to an entity of another kind than its field declares is E022, decided by the linker at the
//! reference (ADR 0117), over the build's own declarations and graph inputs.

use specforge_common::Diagnostic;
use specforge_extension_sdk::PeerDependency;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_test_macros::test as spec;

use crate::support::written::linked;
use crate::support::{build, coded_in, declare, extension};

/// `behavior` with `invariants` (to `invariant`s), `features` (to
/// `feature`s) and an unconstrained `refs`, from software; `feature` with
/// `behaviors` (to `behavior`s), from product.
fn two_extensions() -> (ExtensionDeclaration, ExtensionDeclaration) {
    let software = declare("@specforge/software", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior").testable(true).supports_verify(true);
            k.field("invariants", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("invariant");
            });
            k.field("features", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("feature");
            });
            k.field("refs", |f| {
                f.field_type(FieldType::ReferenceList);
            });
        });
        c.kind("Invariant", |k| {
            k.keyword("invariant").testable(true).supports_verify(true);
        });
    });
    let mut product = extension("@specforge/product");
    product.meta.peer_dependencies.push(PeerDependency {
        name: "@specforge/software".to_string(),
        version: ">=1.0.0".to_string(),
        optional: false,
    });
    product.kind("Feature", |k| {
        k.keyword("feature").testable(false);
        k.field("behaviors", |f| {
            f.field_type(FieldType::ReferenceList)
                .target_kind("behavior");
        });
    });
    (software, product.declaration())
}

/// The E022s the graph build reports on `text`, the two extensions loaded.
fn e022(text: &str) -> Vec<Diagnostic> {
    let (software, product) = two_extensions();
    let diags = linked(&build([software, product]), &[("test.spec", text)]);
    coded_in(&diags, "E022").into_iter().cloned().collect()
}

#[test]
fn a_reference_to_its_target_kind_links_silently() {
    // invariants -> invariant, and feature.behaviors -> behavior, from the peer.
    let diags = e022(
        "behavior b1 \"B\" {\n  invariants [inv1]\n}\ninvariant inv1 \"I\" {\n}\n\
         feature f1 \"F\" {\n  behaviors [b1]\n}\n",
    );
    assert!(diags.is_empty(), "{diags:?}");
}

#[spec(
    behavior = "link_entity_references",
    verify = "a reference to an existing entity of another kind than its field's target kind is E022 at the reference"
)]
fn a_reference_to_another_kind_is_e022_at_the_reference() {
    // A behavior id in the "features" field, which expects a feature.
    let diags = e022(
        "behavior b1 \"B\" {\n  features [some_behavior]\n}\nbehavior some_behavior \"S\" {\n}\n",
    );
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(diags[0].message.contains("some_behavior"));
    let span = diags[0].span.as_ref().unwrap();
    assert_eq!(
        (span.start_line, span.start_col, span.end_col),
        (2, 13, 26),
        "the reference token"
    );
}

#[test]
fn a_field_without_a_target_kind_links_any_kind() {
    let diags = e022("behavior b1 \"B\" {\n  refs [anything]\n}\nwhatever anything \"A\" {\n}\n");
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn a_missing_id_is_e003_alone() {
    let (software, product) = two_extensions();
    let diags = linked(
        &build([software, product]),
        &[(
            "test.spec",
            "behavior b1 \"B\" {\n  invariants [nonexistent]\n}\n",
        )],
    );
    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["E003"], "{diags:?}");
}

#[test]
fn an_undeclared_kinds_field_has_no_target() {
    let diags = e022("unknown_kind u1 \"U\" {\n  features [x]\n}\nbehavior x \"X\" {\n}\n");
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn each_wrong_kind_reference_is_its_own_e022_in_written_order() {
    let diags = e022(
        "behavior my_beh \"M\" {\n  features [b2, b1]\n}\nbehavior b1 \"B\" {\n}\nbehavior b2 \"B\" {\n}\n",
    );
    assert_eq!(diags.len(), 2, "{diags:?}");
    assert!(diags[0].message.contains("'b2'"), "{:?}", diags[0].message);
    assert!(diags[1].message.contains("'b1'"), "{:?}", diags[1].message);
}

#[test]
fn e022_names_the_target_the_field_the_kinds_and_the_source() {
    // An invariant in "features", which expects a feature.
    let diags = e022("behavior my_beh \"M\" {\n  features [inv1]\n}\ninvariant inv1 \"I\" {\n}\n");
    assert_eq!(diags.len(), 1, "{diags:?}");
    let msg = &diags[0].message;
    assert!(msg.contains("inv1"), "the target id: {msg}");
    assert!(msg.contains("features"), "the field name: {msg}");
    assert!(msg.contains("invariant"), "the actual kind: {msg}");
    assert!(msg.contains("feature"), "the expected kind: {msg}");
    assert!(msg.contains("my_beh"), "the source entity: {msg}");
    assert_eq!(
        diags[0].suggestion.as_deref(),
        Some("name a feature here, or drop the reference"),
        "no feature to suggest"
    );
}
