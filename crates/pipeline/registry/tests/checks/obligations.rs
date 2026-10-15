//! `verify` statements are not fields (ADR 0094): a field named `verify` is checked like any field, and statements
//! on a kind that accepts none are W158.

use specforge_extension_sdk::prelude::*;
use specforge_registry::entity::EntityRecord;

use crate::support::{build, check, declare, software, span};

#[test]
#[ignore = "flipped by T6"]
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

#[test]
#[ignore = "flipped by T6"]
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
    assert_eq!(diags[0].code, "W158");
}
