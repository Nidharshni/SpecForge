//! A union's title survives formatting (ADR 0148).

use specforge_test_macros::test as specforge_test;

use crate::regressions::fmt;

#[specforge_test(
    behavior = "apply_format_rules",
    verify = "a union title survives formatting"
)]
fn a_union_title_survives_formatting() {
    assert_eq!(fmt("type k \"K\" = a|b\n"), "type k \"K\" = a | b\n");
}

#[specforge_test(
    behavior = "apply_format_rules",
    verify = "a union title survives formatting"
)]
fn a_long_titled_union_wraps_after_its_title() {
    let out = fmt(
        "type verify_kind \"Verify kind\" = unit | contract | integration | property | performance | mutation | load\n",
    );
    assert!(
        out.starts_with("type verify_kind \"Verify kind\" = unit\n  | contract\n"),
        "{out}"
    );
}
