use crate::{FieldValue, Node};
use specforge_parser::VerifyStatement;

/// An entity's obligations: its `verify` statements, in declaration order.
///
/// `verify` is reserved syntax (ADR 0002); what an obligation means comes
/// from the extensions. The statements are found wherever they sit among the
/// entity's fields. A type may declare a struct member named `verify`
/// (`verify string @optional`); that member is a field, not an obligation,
/// and must not hide the statements, which a first-match lookup of the
/// `verify` key would do.
pub fn obligations(node: &Node) -> &[VerifyStatement] {
    node.fields
        .entries()
        .iter()
        .find_map(|entry| match &entry.value {
            FieldValue::VerifyList(stmts) => Some(stmts.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}
