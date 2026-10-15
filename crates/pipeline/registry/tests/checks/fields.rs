//! W020: a field no extension declares on the entity's kind.

use specforge_common::{SourceSpan, Sym};
use specforge_graph::ValueShape;
use specforge_registry::entity::EntityRecord;
use specforge_test_macros::test as spec;

use specforge_extension_sdk::prelude::FieldType;

use crate::support::{build, check, coded_in, declare, peer, software, span};

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "unregistered field name produces W020"
)]
fn unregistered_field_name_produces_w020() {
    let entities = vec![
        EntityRecord::new("behavior", "b1", span("test.spec")).with_fields(&["unknown_field"]),
    ];
    let diags = check(&build([software()]), &entities);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "W020" && d.message.contains("unknown_field")),
        "{diags:?}"
    );
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "W020 includes field name, entity kind, and source span"
)]
fn w020_includes_field_name_entity_kind_and_source_span() {
    let s = SourceSpan {
        file: Sym::new("my.spec"),
        start_line: 5,
        start_col: 3,
        end_line: 5,
        end_col: 20,
    };
    let entities = vec![EntityRecord::new("behavior", "b1", &s).with_fields(&["bogus_field"])];
    let diags = check(&build([software()]), &entities);
    let w020 = coded_in(&diags, "W020");
    assert_eq!(w020.len(), 1, "{diags:?}");
    assert!(
        w020[0].message.contains("bogus_field"),
        "should contain field name"
    );
    assert!(
        w020[0].message.contains("behavior"),
        "should contain entity kind"
    );
    assert!(w020[0].span.is_some(), "should contain source span");
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "expression is checked like any other field (W020 where undeclared)"
)]
fn expression_is_checked_like_any_other_field() {
    let alone = build([software()]);
    // software's invariant and behavior declare no `expression`; without an
    // extension that declares it (formal enhances invariant), it is W020.
    let entities = vec![
        EntityRecord::new("invariant", "i1", span("test.spec")).with_fields(&["expression"]),
        EntityRecord::new("behavior", "b1", span("test.spec")).with_fields(&["expression"]),
    ];
    let diags = check(&alone, &entities);
    let flagged: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(diags.len(), 2, "{flagged:?}");
    assert!(
        diags
            .iter()
            .all(|d| d.code == "W020" && d.message.contains("'expression'")),
        "{flagged:?}"
    );

    // Declared on a kind (formal enhances invariant), it is a field of that
    // kind like any other.
    let formal = declare("@specforge/formal", |c| {
        c.meta.peer_dependencies = vec![peer("@specforge/software", "^1")];
        c.enhance("invariant", "@specforge/software", |e| {
            e.field("expression", |f| {
                f.field_type(FieldType::String)
                    .normative()
                    .proof_role("claim");
            });
        });
    });
    let with_formal = build([software(), formal]);
    assert!(with_formal.diagnostics().next().is_none());
    let diags = check(&with_formal, &entities);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(diags[0].message.contains("behavior"), "{diags:?}");
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "registered field name does not produce W020"
)]
fn registered_field_name_does_not_produce_w020() {
    let entities = vec![
        EntityRecord::new("behavior", "b1", span("test.spec")).with_value(
            "contract",
            ValueShape::Block,
            "",
        ),
    ];
    let diags = check(&build([software()]), &entities);
    assert!(diags.is_empty(), "registered field should not produce W020");
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "title is not checked against the field registry"
)]
fn title_is_not_checked_against_the_field_registry() {
    let entities =
        vec![EntityRecord::new("behavior", "b1", span("test.spec")).with_fields(&["title"])];
    let diags = check(&build([software()]), &entities);
    assert!(diags.is_empty(), "title should be skipped");
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "field validation skipped when entity kind is unregistered"
)]
fn field_validation_skipped_when_entity_kind_is_unregistered() {
    let entities = vec![
        EntityRecord::new("nonexistent_kind", "x1", span("test.spec")).with_fields(&["some_field"]),
    ];
    let diags = check(&build([software()]), &entities);
    assert!(
        coded_in(&diags, "W020").is_empty(),
        "unregistered kind should skip field validation to avoid cascading diagnostics"
    );
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "an undeclared field a builtin enhancement adds suggests its extension"
)]
fn w020_for_a_builtin_enhancements_field_names_its_extension() {
    let diags = check(
        &build([software()]),
        &[EntityRecord::new("invariant", "i1", span("test.spec"))
            .with_fields(&["expression", "bogus"])],
    );
    assert_eq!(diags.len(), 2, "{diags:?}");
    assert!(
        diags[0]
            .suggestion
            .as_deref()
            .unwrap()
            .contains("specforge add @specforge/formal"),
        "{diags:?}"
    );
    assert_eq!(diags[1].suggestion, None, "{diags:?}");
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "Detect Unknown Entity Fields: unknown field detection holds — registries_populated_fired, unknown_fields_diagnosed, cascading_avoided"
)]
fn detect_unknown_entity_fields_contract() {
    let build = build([software()]);
    // ensures: unknown field → W020
    let e1 = vec![
        EntityRecord::new("behavior", "b1", span("test.spec")).with_fields(&["unknown_field"]),
    ];
    assert!(check(&build, &e1).iter().any(|d| d.code == "W020"));
    // ensures: registered field → no W020
    let e2 = vec![
        EntityRecord::new("behavior", "b2", span("test.spec")).with_value(
            "contract",
            ValueShape::Block,
            "",
        ),
    ];
    assert!(check(&build, &e2).is_empty());
    // ensures: unregistered kind → skipped
    let e3 =
        vec![EntityRecord::new("unknown_kind", "x", span("test.spec")).with_fields(&["field"])];
    assert!(coded_in(&check(&build, &e3), "W020").is_empty());
}

#[spec(
    behavior = "two_phase_validate_semantic",
    verify = "field validation uses FieldRegistry"
)]
fn field_validation_uses_the_field_registry() {
    let entities = vec![
        EntityRecord::new("behavior", "my_beh", span("test.spec"))
            .with_value("contract", ValueShape::Block, "")
            .with_fields(&["unknown_field"]),
    ];
    let diags = check(&build([software()]), &entities);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "W020" && d.message.contains("unknown_field")),
        "{diags:?}"
    );
    assert!(!diags.iter().any(|d| d.message.contains("'contract'")));
}
