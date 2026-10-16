//! W020: a field no extension declares on the entity's kind. Every test starts from `.spec` text
//! (ADR 0115).

use specforge_test_macros::test as spec;

use specforge_extension_sdk::prelude::FieldType;

use crate::support::written::checked;
use crate::support::{build, candidates, coded_in, declare, formal, peer, software};

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "unregistered field name produces W020"
)]
fn unregistered_field_name_produces_w020() {
    let text = "behavior b1 \"B\" {\n  unknown_field \"\"\n}\n";
    let diags = checked(&build([software()]), &[("test.spec", text)]);
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
    let text = "\n\nbehavior b1 \"B\" {\n  bogus_field \"\"\n}\n";
    let diags = checked(&build([software()]), &[("my.spec", text)]);
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
    let span = w020[0].span.as_ref().expect("should contain source span");
    assert_eq!(span.file.as_str(), "my.spec");
    assert!(span.start_line >= 3, "{span:?}");
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "expression is checked like any other field (W020 where undeclared)"
)]
fn expression_is_checked_like_any_other_field() {
    let alone = build([software()]);
    // software's invariant and behavior declare no `expression`; without an
    // extension that declares it (formal enhances invariant), it is W020.
    let text = "invariant i1 \"I\" {\n  expression \"x\"\n}\n\n\
                behavior b1 \"B\" {\n  expression \"x\"\n}\n";
    let diags = checked(&alone, &[("test.spec", text)]);
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
        c.enhance("invariant", |e| {
            e.field("expression", |f| {
                f.field_type(FieldType::String)
                    .normative()
                    .proof_role("claim");
            });
        });
    });
    let with_formal = build([software(), formal]);
    assert!(with_formal.diagnostics().next().is_none());
    let diags = checked(&with_formal, &[("test.spec", text)]);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(diags[0].message.contains("behavior"), "{diags:?}");
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "registered field name does not produce W020"
)]
fn registered_field_name_does_not_produce_w020() {
    let text = "behavior b1 \"B\" {\n  contract {\n  }\n}\n";
    let diags = checked(&build([software()]), &[("test.spec", text)]);
    assert!(diags.is_empty(), "registered field should not produce W020");
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "title is not checked against the field registry"
)]
fn title_is_not_checked_against_the_field_registry() {
    let text = "behavior b1 \"B\" {\n  title \"T\"\n}\n";
    let diags = checked(&build([software()]), &[("test.spec", text)]);
    assert!(diags.is_empty(), "title should be skipped: {diags:?}");
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "field validation skipped when entity kind is unregistered"
)]
fn field_validation_skipped_when_entity_kind_is_unregistered() {
    let text = "wibble x1 \"X\" {\n  some_field \"\"\n}\n";
    let diags = checked(&build([software()]), &[("test.spec", text)]);
    // The entity is there (E024 names its kind) and its fields are left alone.
    assert!(!coded_in(&diags, "E024").is_empty(), "{diags:?}");
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
    let text = "invariant i1 \"I\" {\n  expression \"x\"\n  bogus \"x\"\n}\n";
    let build = build([software()]).with_candidates(candidates(&[formal()]));
    let diags = checked(&build, &[("test.spec", text)]);
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
    let e1 = "behavior b1 \"B\" {\n  unknown_field \"\"\n}\n";
    assert!(
        checked(&build, &[("test.spec", e1)])
            .iter()
            .any(|d| d.code == "W020")
    );
    // ensures: registered field → no W020
    let e2 = "behavior b2 \"B\" {\n  contract {\n  }\n}\n";
    assert!(checked(&build, &[("test.spec", e2)]).is_empty());
    // ensures: unregistered kind → skipped
    let e3 = "wibble x \"X\" {\n  field \"\"\n}\n";
    assert!(coded_in(&checked(&build, &[("test.spec", e3)]), "W020").is_empty());
}

#[spec(
    behavior = "two_phase_validate_semantic",
    verify = "field validation uses FieldRegistry"
)]
fn field_validation_uses_the_field_registry() {
    let text = "behavior my_beh \"B\" {\n  contract {\n  }\n  unknown_field \"\"\n}\n";
    let diags = checked(&build([software()]), &[("test.spec", text)]);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "W020" && d.message.contains("unknown_field")),
        "{diags:?}"
    );
    assert!(!diags.iter().any(|d| d.message.contains("'contract'")));
}
