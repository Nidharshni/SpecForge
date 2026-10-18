//! Own and inherited fields (ADR 0149): a block that does not write a field takes its file's `defaults <kind>`
//! entry for it. A typed edit copies an inherited list into an own field before it changes it, removes an own
//! field that equals the default (W161), and never writes a `defaults` block.

use super::EditField;
use super::block::{Block, Items, Site, items_of, put_list};
use super::write::not_editable;
use crate::Refusal;
use specforge_common::structural;
use specforge_graph::field_form::{self, Reading};
use specforge_parser::{FieldEntry, SpecFile};
use specforge_protocol_types::FieldType;

/// The file's `defaults <kind>` entry for `field`.
pub fn default_entry<'f>(file: &'f SpecFile, kind: &str, field: &str) -> Option<&'f FieldEntry> {
    file.entities
        .iter()
        .find(|e| e.kind.raw.as_str() == structural::DEFAULTS && e.id.raw.as_str() == kind)?
        .fields
        .entries()
        .iter()
        .find(|e| e.key.as_str() == field)
}

/// `text` with the own `field` of the entity removed when it equals the file's default for it.
pub fn without_repeat(
    site: &Site<'_>,
    text: &str,
    field: &str,
) -> Result<String, Refusal<EditField>> {
    let block = Block::open(site, text)?;
    let repeats = match (
        block.entry(field),
        default_entry(block.file(), site.kind, field),
    ) {
        (Some(own), Some(default)) => {
            field_form::write(&own.value) == field_form::write(&default.value)
        }
        _ => false,
    };
    if repeats {
        put_list(site, text, &block.text(), field, None)
    } else {
        Ok(text.to_string())
    }
}

/// The list `items` is `field`'s new value: written as an own field, or removed from the block when the file's
/// default says the same (or it is empty and nothing is inherited). A `[]` over a default stays: it says "none".
pub fn write_list(
    site: &Site<'_>,
    text: &str,
    before: &str,
    field: &str,
    items: &Items,
) -> Result<String, Refusal<EditField>> {
    let block = Block::open(site, text)?;
    let default = default_entry(block.file(), site.kind, field);
    let same_as_default = default
        .and_then(|d| items_of(&d.value))
        .is_some_and(|d| d.texts == items.texts);
    let nothing = items.texts.is_empty() && default.is_none();
    let value = (!same_as_default && !nothing).then_some(items);
    put_list(site, text, before, field, value)
}

/// `unset` of a field the block does not write: an inherited list becomes `field []`; anything else is already
/// unset.
pub fn unset_inherited(
    site: &Site<'_>,
    text: &str,
    field: &str,
) -> Result<String, Refusal<EditField>> {
    let block = Block::open(site, text)?;
    if default_entry(block.file(), site.kind, field).is_none() {
        return Ok(text.to_string());
    }
    let is_list = matches!(
        site.view.registries().field_forms().reading(site.kind, field),
        Reading::Declared(form) if matches!(form.field_type(), FieldType::ReferenceList | FieldType::StringList)
    );
    if !is_list {
        return Err(not_editable(
            EditField::Field,
            format!("'{field}' comes from the file's defaults; use set to give this block its own"),
        ));
    }
    let none = Items {
        written: Vec::new(),
        texts: Vec::new(),
    };
    put_list(site, text, &block.text(), field, Some(&none))
}
