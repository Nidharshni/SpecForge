//! Own and inherited fields (ADR 0149): a block that does not write a field takes its file's `defaults <kind>`
//! entry for it. A typed edit copies an inherited list into an own field before it changes it, removes an own
//! field that restates the default (W161, asked of the graph's `FileDefaults` over the text being planned), and
//! never writes a `defaults` block.

use super::EditField;
use super::block::{Block, Items, Site, items_of, put_list};
use super::write::not_editable;
use crate::Refusal;
use specforge_graph::FileDefaults;
use specforge_graph::field_form::Reading;
use specforge_parser::SpecFile;
use specforge_protocol_types::FieldType;

/// The file's defaults as the graph build applies them (ADR 0149), read from `file`: the text being planned, as the
/// earlier edits of the list left it (ADR 0154).
pub fn of<'f>(site: &Site<'_>, file: &'f SpecFile) -> FileDefaults<'f> {
    FileDefaults::of(file, site.view.registries().field_forms()).0
}

/// `text` with the own `field` of the entity removed when it equals the file's default for it.
pub fn without_repeat(
    site: &Site<'_>,
    text: &str,
    field: &str,
) -> Result<String, Refusal<EditField>> {
    let block = Block::open(site, text)?;
    let repeats = block
        .entry(field)
        .is_some_and(|own| of(site, block.file()).restates(site.kind, own));
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
    let default = of(site, block.file()).entry(site.kind, field);
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
    if of(site, block.file()).entry(site.kind, field).is_none() {
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
