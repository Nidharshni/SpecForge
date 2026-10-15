//! `verify` statements are not fields (ADR 0094): a field named `verify` is checked like any field, and statements
//! on a kind that accepts none are W158.

use specforge_registry::entity::EntityRecord;
use specforge_test_macros::test as spec;

use specforge_common::codes;

use crate::support::{build, check, declare, software, span};

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "a field named verify is checked like any field"
)]
fn a_field_named_verify_is_checked_like_any_field() {
    let entities =
        vec![EntityRecord::new("behavior", "b1", span("test.spec")).with_fields(&["verify"])];
    let diags = check(&build([software()]), &entities);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "W020");
    assert!(diags[0].message.contains("'verify'"), "{diags:?}");
    assert!(
        !diags[0]
            .suggestion
            .as_deref()
            .is_some_and(|s| s.contains("@specforge/testing")),
        "{diags:?}"
    );
}

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "verify statements on a kind no extension made testable produce W158"
)]
fn statements_on_a_kind_that_accepts_none_are_w158() {
    let build = build([declare("@specforge/software", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior")
                .testable(true)
                .singleton(false)
                .supports_verify(false);
        });
    })]);
    let entities =
        vec![EntityRecord::new("behavior", "b1", span("test.spec")).with_obligation("unit", "x")];
    let diags = check(&build, &entities);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, codes::W158.id());
}
