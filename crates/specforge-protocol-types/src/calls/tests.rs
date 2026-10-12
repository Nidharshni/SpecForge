use super::*;

#[test]
fn a_pass_is_called_at_its_prefixed_export() {
    assert_eq!(pass_export("audit"), "__pass_audit");
    assert_eq!(pass_export("a-b_c"), "__pass_a-b_c");
}
