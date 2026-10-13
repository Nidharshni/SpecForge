//! E022: a reference to an entity of another kind than its field declares.

use specforge_common::Diagnostic;
use specforge_extension_sdk::PeerDependency;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::entity::EntityRecord;
use specforge_test_macros::test as spec;

use crate::support::{build, check, coded_in, declare, extension, span};

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

/// The E022s of the checks over `records` of the two extensions.
fn e022(records: &[EntityRecord]) -> Vec<Diagnostic> {
    let (software, product) = two_extensions();
    let diags = check(&build([software, product]), records);
    coded_in(&diags, "E022").into_iter().cloned().collect()
}

#[test]
fn a_reference_to_the_right_kind_is_no_diagnostic() {
    let s = span("test.spec");
    let diags = e022(&[
        EntityRecord::new("behavior", "b1", s).with_reference("invariants", &["inv1"]),
        EntityRecord::new("invariant", "inv1", s),
    ]);
    assert!(diags.is_empty(), "{diags:?}");
}

#[spec(
    behavior = "check_entities_in_one_order",
    verify = "a reference to an existing entity of another kind than its field's target kind is E022"
)]
fn a_reference_to_the_wrong_kind_produces_e022() {
    let s = span("test.spec");
    // A behavior ID in the "features" field, which expects a feature.
    let diags = e022(&[
        EntityRecord::new("behavior", "b1", s).with_reference("features", &["some_behavior"]),
        EntityRecord::new("behavior", "some_behavior", s),
    ]);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(diags[0].message.contains("some_behavior"));
    assert!(diags[0].message.contains("behavior"));
    assert!(diags[0].message.contains("feature"));
}

#[test]
fn a_field_with_no_target_kind_constraint_is_no_diagnostic() {
    let s = span("test.spec");
    let diags = e022(&[
        EntityRecord::new("behavior", "b1", s).with_reference("refs", &["anything"]),
        EntityRecord::new("whatever", "anything", s),
    ]);
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn a_reference_to_a_missing_entity_is_not_this_checks() {
    let s = span("test.spec");
    let diags = e022(&[
        EntityRecord::new("behavior", "b1", s).with_reference("invariants", &["nonexistent"])
    ]);
    assert!(diags.is_empty(), "the linker's E003 reports it: {diags:?}");
}

#[test]
fn an_entity_of_an_unregistered_kind_is_skipped() {
    let s = span("test.spec");
    let diags = e022(&[
        EntityRecord::new("unknown_kind", "u1", s).with_reference("features", &["x"]),
        EntityRecord::new("behavior", "x", s),
    ]);
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn each_wrong_kind_reference_produces_its_own_e022() {
    let s = span("test.spec");
    let diags = e022(&[
        EntityRecord::new("behavior", "my_beh", s).with_reference("features", &["b1", "b2"]),
        EntityRecord::new("behavior", "b1", s),
        EntityRecord::new("behavior", "b2", s),
    ]);
    assert_eq!(diags.len(), 2, "{diags:?}");
}

#[test]
fn a_typed_reference_across_extensions_is_validated() {
    let s = span("test.spec");
    // feature.behaviors accepts a behavior: the right kind, from the peer.
    let diags = e022(&[
        EntityRecord::new("feature", "f1", s).with_reference("behaviors", &["b1"]),
        EntityRecord::new("behavior", "b1", s),
    ]);
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn e022_names_the_target_the_field_the_kinds_and_the_source() {
    let s = span("test.spec");
    // An invariant in "features", which expects a feature.
    let diags = e022(&[
        EntityRecord::new("behavior", "my_beh", s).with_reference("features", &["inv1"]),
        EntityRecord::new("invariant", "inv1", s),
    ]);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let msg = &diags[0].message;
    assert!(msg.contains("inv1"), "the target id: {msg}");
    assert!(msg.contains("features"), "the field name: {msg}");
    assert!(msg.contains("invariant"), "the actual kind: {msg}");
    assert!(msg.contains("feature"), "the expected kind: {msg}");
    assert!(msg.contains("my_beh"), "the source entity: {msg}");
}
