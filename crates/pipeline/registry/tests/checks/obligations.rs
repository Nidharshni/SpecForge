//! `verify` statements are not fields (ADR 0094): a field named `verify` is checked like any field, and statements
//! on a kind that accepts none are W158. Every test starts from `.spec` text (ADR 0115).

use specforge_test_macros::test as spec;

use specforge_common::codes;

use crate::support::written::checked;
use crate::support::{build, declare, software};

#[spec(
    behavior = "detect_unknown_entity_fields",
    verify = "a field named verify is checked like any field"
)]
fn a_field_named_verify_is_checked_like_any_field() {
    // `verify` followed by no string is a field (`verify string`), not a statement.
    let text = "behavior b1 \"B\" {\n  verify string\n}\n";
    let diags = checked(&build([software()]), &[("test.spec", text)]);
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
    let text = "behavior b1 \"B\" {\n  verify unit \"x\"\n}\n";
    let diags = checked(&build, &[("test.spec", text)]);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, codes::W158.id());
}
