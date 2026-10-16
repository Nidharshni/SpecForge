//! The field text: one string per written value, the rule every reader after the graph build
//! shares (ADR 0019).

use specforge_parser::FieldValue;

/// A field value's text: the one rule every reader after the graph build
/// shares (ADR 0019). Scalars as written; lists joined by `", "`; variant lists and type unions by
/// `" | "`; expressions by `", "` in their display form; a block's keys by `", "`. Empty
/// values are `""`.
///
/// The written value ([`specforge_graph::field_form::write`]) differs only for an expression
/// group, written one conjunct per line (ADR 0064 D3): the field text is one string per field
/// for rules, validators and passes; the written value is what every output writes and every
/// input reads back.
///
/// A joined list cannot be split back when an item itself contains the
/// joiner (`["a, b", "c"]` is `"a, b, c"`); [`field_items`] keeps them.
pub fn field_text(value: &FieldValue) -> String {
    let joined = |joiner: &str| field_items(value).unwrap_or_default().join(joiner);
    // No `_` arm: a new variant does not compile until it has a text.
    match value {
        FieldValue::String(s) | FieldValue::Identifier(s) | FieldValue::Date(s) => s.clone(),
        FieldValue::Integer(n) => n.to_string(),
        FieldValue::Boolean(b) => b.to_string(),
        FieldValue::List(_) | FieldValue::Expression(_) | FieldValue::Block(_) => joined(", "),
        FieldValue::VariantList(_) | FieldValue::TypeUnion(_) => joined(" | "),
    }
}

/// A list-shaped value's items, unjoined, each as its text: the list's
/// items' texts, a variant list's or type union's members, each expression's display form, a block's keys. `None` for a scalar. [`field_text`]
/// is these joined.
pub fn field_items(value: &FieldValue) -> Option<Vec<String>> {
    match value {
        FieldValue::String(_)
        | FieldValue::Identifier(_)
        | FieldValue::Date(_)
        | FieldValue::Integer(_)
        | FieldValue::Boolean(_) => None,
        FieldValue::VariantList(items) | FieldValue::TypeUnion(items) => Some(items.clone()),
        FieldValue::List(items) => Some(items.iter().map(|item| item.text.clone()).collect()),
        FieldValue::Expression(exprs) => Some(exprs.iter().map(ToString::to_string).collect()),
        FieldValue::Block(block) => {
            Some(block.entries().iter().map(|e| e.key.to_string()).collect())
        }
    }
}
