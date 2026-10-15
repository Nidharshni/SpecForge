//! Who an enhancement's fields are credited to, and when an enhancement of
//! an unknown kind is silent (ADR 0096).

use specforge_registry::FieldType;
use specforge_test_macros::test as spec;

use crate::support::{build, coded, declare, diagnostics, software};

#[spec(
    behavior = "register_entity_enhancements",
    verify = "an enhancement's fields are credited to the extension that declares the enhancement"
)]
#[ignore = "flipped by T3"]
fn an_enhancements_fields_are_credited_to_its_declarer() {
    let mut enhancer = declare("@acme/enhancer", |c| {
        c.optional_peer("@specforge/software", "^1");
        c.enhance("behavior", "@specforge/software", |e| {
            e.field("layer", |f| {
                f.field_type(FieldType::String);
            });
            e.field("bad", |f| {
                f.field_type(FieldType::String);
            });
            e.field("layers", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("Layers")
                    .target_kind("behavior");
            });
        });
        c.edge("Layers", |e| {
            e.target_kind("behavior");
        });
    });
    enhancer.enhancements[0].fields[1].field_type = "nope".into();

    let build = build([software(), enhancer]);

    let layer = build.fields().get("behavior", "layer").unwrap();
    assert_eq!(layer.source_extension(), "@acme/enhancer");
    let w019 = coded(&build, "W019");
    assert_eq!(w019.len(), 1, "{:?}", diagnostics(&build));
    assert!(
        w019[0]
            .message
            .starts_with("extension '@acme/enhancer': unknown field type 'nope'"),
        "{}",
        w019[0].message
    );
    let layers = build.edges().get("Layers").unwrap();
    assert!(
        layers
            .writers()
            .iter()
            .all(|w| w.extension == "@acme/enhancer"),
        "{:?}",
        layers.writers()
    );
    assert_eq!(layers.writers().len(), 1);
}

#[spec(
    behavior = "register_entity_enhancements",
    verify = "an enhancement of an unknown kind is skipped silently when its extension declares an optional peer the project does not load"
)]
#[ignore = "flipped by T3"]
fn an_unknown_target_is_silent_only_behind_an_unloaded_optional_peer() {
    let enhance_absent = |name: &str, owner: &str, optional: bool| {
        declare(name, |c| {
            if optional {
                c.optional_peer("@acme/absent", "^1");
            }
            c.enhance("absent_kind", owner, |e| {
                e.field("x", |f| {
                    f.field_type(FieldType::String);
                });
            });
        })
    };

    // An unloaded optional peer, whatever owner the wire names: silent.
    let silent = build([software(), enhance_absent("@acme/a", "@acme/a", true)]);
    assert!(
        diagnostics(&silent).is_empty(),
        "{:?}",
        diagnostics(&silent)
    );
    assert!(!silent.fields().contains("absent_kind", "x"));

    // No optional peer, whatever owner the wire names: I004.
    let loud = build([software(), enhance_absent("@acme/b", "@acme/absent", false)]);
    assert_eq!(coded(&loud, "I004").len(), 1, "{:?}", diagnostics(&loud));
    assert_eq!(diagnostics(&loud).len(), 1);
}
