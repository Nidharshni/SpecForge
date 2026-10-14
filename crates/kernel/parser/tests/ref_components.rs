//! A one-line ref block's parts.

use specforge_parser::{FieldValue, parse};
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "parse_ref_blocks",
    verify = "ref block extracts scheme, kind, and identifier components"
)]
fn a_one_line_ref_splits_into_scheme_kind_and_identifier() {
    let result = parse(r#"ref gh.issue:42 "Support Wasm extensions""#, "test.spec");
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let fields = &result.entities[0].fields;
    assert!(matches!(fields.get("scheme"), Some(FieldValue::String(s)) if s == "gh"));
    assert!(matches!(fields.get("ref_kind"), Some(FieldValue::String(s)) if s == "issue"));
    assert!(matches!(fields.get("identifier"), Some(FieldValue::String(s)) if s == "42"));
}
