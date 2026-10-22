//! What an edit's field argument names (ADR 0151, amended by round 16 plan 09): a field of the entity's kind, or
//! `field.entry`, one entry of a block field the kind declares. Every verb reads its argument here first; the kind's
//! answers stay the registry's, asked through `Site::reading_toward` (ADR 0157 D4).

use super::EditField;
use super::block::{Block, Site};
use super::write::{NOT_EDITABLE, not_editable};
use crate::{OpError, OpErrorKind, Refusal};
use specforge_graph::field_form::{Reading, is_word};
use specforge_parser::{FieldEntry, FieldValue};
use specforge_protocol_types::FieldType;

/// An edit's field argument, read against the entity's kind.
pub enum FieldPath<'p, 'r> {
    /// A field the kind declares, or a member of a kind that takes any field (never `Reading::Undeclared`).
    Field { name: &'p str, reading: Reading<'r> },
    /// `field.entry`: one entry of the block field `field`, which the kind declares.
    Entry { field: &'p str, entry: &'p str },
}

/// `path` read against `site`'s kind. `target` is a link's or unlink's value: an undeclared field is then told as the
/// other side of a relation by the target's kind (ADR 0157 D1).
///
/// Refused, before any text is read: `not_editable` when a dotted path's parts are not each one identifier;
/// `unknown_field` (with its near miss) or `other_side` when the field (the part before the dot) is not the kind's;
/// `not_editable` when a path's field is not a declared block field.
pub fn resolve<'p, 'r>(
    site: &'r Site<'_>,
    path: &'p str,
    target: Option<&str>,
) -> Result<FieldPath<'p, 'r>, Refusal<EditField>> {
    let Some((field, entry)) = path.split_once('.') else {
        let reading = site.reading_toward(path, target)?;
        return Ok(FieldPath::Field {
            name: path,
            reading,
        });
    };
    if !is_word(field) || !is_word(entry) {
        return Err(not_editable(
            EditField::Field,
            format!("'{path}' is not a field or a field.entry path: each part is one identifier"),
        ));
    }
    match site.reading_toward(field, target)? {
        Reading::Declared(form) if form.field_type() == FieldType::Block => {
            Ok(FieldPath::Entry { field, entry })
        }
        _ => Err(not_editable(
            EditField::Field,
            format!("'{field}' is not a block field, so it has no entry '{entry}'"),
        )),
    }
}

/// The refusal of `verb` (`link`, `unlink`, `create`), which takes a whole field, given `field.entry`.
pub fn whole_field(verb: &str, field: &str, entry: &str) -> Refusal<EditField> {
    let suggestion = match verb {
        "create" => format!("give '{field}' its entries as name: text"),
        _ => format!("an entry holds one text: use set or unset on '{field}.{entry}'"),
    };
    OpError::new(
        OpErrorKind::InvalidInput,
        NOT_EDITABLE,
        format!(
            "{verb} takes a whole field; '{field}.{entry}' is an entry of the block field '{field}'"
        ),
    )
    .with_suggestion(suggestion)
    .about(EditField::Field)
}

/// The block field `field` and its entry `entry`, as the block writes them: `None` when the block does not write
/// `field`; refused when it writes `field` as something other than a block (the compile's E061).
pub fn nested<'b>(
    site: &Site<'_>,
    block: &'b Block,
    field: &str,
    entry: &str,
) -> Result<Option<(&'b FieldEntry, Option<&'b FieldEntry>)>, Refusal<EditField>> {
    let Some(outer) = block.entry(field) else {
        return Ok(None);
    };
    let FieldValue::Block(map) = &outer.value else {
        return Err(not_editable(
            EditField::Field,
            format!(
                "'{field}' of '{}' is written as a value, not a block; fix it by hand or set it whole",
                site.id
            ),
        ));
    };
    Ok(Some((outer, map.entry(entry))))
}
