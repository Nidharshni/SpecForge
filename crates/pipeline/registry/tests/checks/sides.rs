//! E079: a field written on the other side of a relation another kind owns (ADR 0150). Every test
//! starts from `.spec` text (ADR 0115).

use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_test_macros::test as spec;

use crate::support::written::checked;
use crate::support::{build, coded_in, declare};

/// `behavior` owns the reference fields `owned` (name, target kind, `inverse_of`); `other` is a
/// kind with no field.
fn owner(owned: &[(&str, &str, Option<&str>)], other: &str) -> ExtensionDeclaration {
    declare("@test/sides", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior");
            for (name, target, inverse) in owned {
                k.field(name, |f| {
                    f.field_type(FieldType::ReferenceList).target_kind(target);
                    if let Some(inverse) = inverse {
                        f.inverse_of(inverse);
                    }
                });
            }
        });
        c.kind("Other", |k| {
            k.keyword(other);
        });
    })
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "a field written on the non-owning side of a relation is E079 naming the owning field"
)]
fn a_field_written_on_the_non_owning_side_is_e079_naming_the_owning_field() {
    let build = build([owner(&[("features", "feature", None)], "feature")]);
    let text = "feature checkout \"C\" {\n  behaviors [pay]\n}\nbehavior pay \"P\" {\n}\n";
    let diags = checked(&build, &[("test.spec", text)]);
    let e079 = coded_in(&diags, "E079");
    assert_eq!(e079.len(), 1, "{diags:?}");
    assert!(e079[0].message.contains("behavior.features"), "{diags:?}");
    let suggestion = e079[0].suggestion.as_deref().unwrap_or_default();
    assert!(suggestion.contains("features [checkout]") && suggestion.contains("pay"));
    assert!(coded_in(&diags, "W020").is_empty(), "{diags:?}");
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "a reverse name an inverse_of declares is E079 even when its ids do not resolve"
)]
fn a_reverse_name_is_e079_even_when_its_ids_do_not_resolve() {
    let build = build([owner(
        &[("features", "feature", Some("behaviors"))],
        "feature",
    )]);
    let text = "feature checkout \"C\" {\n  behaviors [ghost]\n}\n";
    let diags = checked(&build, &[("test.spec", text)]);
    assert_eq!(coded_in(&diags, "E079").len(), 1, "{diags:?}");
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "an event's trigger is E079 naming the produces and consumes fields"
)]
fn an_event_trigger_is_e079_naming_produces_and_consumes() {
    let build = build([owner(
        &[("produces", "event", None), ("consumes", "event", None)],
        "event",
    )]);
    let text = "event paid \"P\" {\n  trigger pay\n}\nbehavior pay \"P\" {\n}\n";
    let diags = checked(&build, &[("test.spec", text)]);
    let e079 = coded_in(&diags, "E079");
    assert_eq!(e079.len(), 1, "{diags:?}");
    let suggestion = e079[0].suggestion.as_deref().unwrap_or_default();
    assert!(suggestion.contains("consumes [paid]") && suggestion.contains("produces [paid]"));
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "an unknown field of a kind's own relation stays W020"
)]
fn an_unknown_field_naming_its_own_kind_stays_w020() {
    let build = build([declare("@test/self", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior");
            k.field("refines", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("behavior");
            });
        });
    })]);
    let text = "behavior b1 \"B\" {\n  depends_on [b2]\n}\nbehavior b2 \"B\" {\n}\n";
    let diags = checked(&build, &[("test.spec", text)]);
    assert_eq!(coded_in(&diags, "W020").len(), 1, "{diags:?}");
    assert!(coded_in(&diags, "E079").is_empty(), "{diags:?}");
}
