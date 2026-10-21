//! What a block's field holds, its own or its file's default (ADR 0149, ADR 0154, ADR 0175). A block that does not
//! write a field takes its file's `defaults <kind>` entry for it. Every typed edit that changes a field does it
//! through [`change`]: an inherited list or block field is copied into an own field first, the verb changes the own
//! field, and what is left is removed when it holds what the default holds, as the graph stores both (asked of the
//! graph's `FileDefaults` over the text being planned) or written as none (`[]`, `{}`) when it is left empty over a default. No edit writes a
//! `defaults` block.

use super::EditField;
use super::block::{Block, Site, items_of, list_text};
use super::write::not_editable;
use crate::Refusal;
use specforge_graph::field_form::Reading;
use specforge_graph::{FileDefaults, Held};
use specforge_parser::{FieldEntry, SpecFile};
use specforge_protocol_types::FieldType;

/// What a verb did to the field [`change`] handed it.
pub enum Change {
    /// Nothing; the verb did not touch the block. The text comes back as it was.
    Unchanged,
    /// The verb wrote the field's new value into the block.
    Changed,
    /// The field is to hold nothing: its last item or entry went, or it was unset. The verb did not touch the block.
    Emptied,
}

/// The file's defaults as the graph build applies them (ADR 0149), read from `file`: the text being planned, as the
/// earlier edits of the list left it (ADR 0154).
fn of<'f>(site: &Site<'_>, file: &'f SpecFile) -> FileDefaults<'f> {
    FileDefaults::of(file, site.view.registries().field_forms()).0
}

/// `field` of `site`'s entity, changed by `verb` under the defaults rule (ADR 0149 D3, ADR 0151 D5, ADR 0154,
/// ADR 0175).
///
/// - **Ordering.** Read the field argument first (`path::resolve`), so a path refusal comes before any text is read.
///   `field` is the whole field's name: for `field.entry`, the block field's.
/// - **What `verb` gets.** The block, laid out, with the copy already in it, and what the block holds for `field`
///   (`Held`, cloned: the verb may mutate the block, ADR 0182 D3). An inherited list or block field arrives as the
///   copy, `Own`; a default the module does not copy (one value) arrives `Inherited`; `Absent` when the block does
///   not write the field and its file gives no default.
/// - **What `verb` returns.** `Changed` after writing into the block; `Unchanged` and `Emptied` without touching
///   it.
/// - **The result.** Only `site`'s block changes; it is canonical; no own field of it restates its default; a field
///   emptied over a default holds none (`[]` for a list, `{}` for a block field); the original `text` comes back
///   byte for byte when the block's canonical text did not change.
/// - **Refusals.** `block_unreadable`; whatever `verb` refuses; `not_editable` when a field of one value over a
///   default is emptied (it cannot say none).
pub fn change(
    site: &Site<'_>,
    text: &str,
    field: &str,
    verb: impl FnOnce(&mut Block, Held<FieldEntry>) -> Result<Change, Refusal<EditField>>,
) -> Result<String, Refusal<EditField>> {
    let mut block = Block::open(site, text)?;
    let before = block.text();
    let copied = match of(site, block.file()).held(block.entity(), field) {
        Held::Inherited(default) => copy(site, &block, field, default),
        _ => None,
    };
    if let Some(copy) = copied {
        block.place(site, field, &copy, None);
        block = Block::open(site, &block.text())?; // the copy is now the block's own field
    }
    let held = of(site, block.file()).held(block.entity(), field).cloned();
    let over_default = held.has_default();
    let after = match verb(&mut block, held)? {
        Change::Unchanged => return Ok(text.to_string()),
        Change::Changed => block.finish(site)?,
        Change::Emptied => emptied(site, block, field, over_default)?,
    };
    let after = without_repeat(site, &after, field)?;
    Ok(if after == before {
        text.to_string()
    } else {
        after
    })
}

/// The own field `default` is copied into: a list as the graph stores it (ADR 0154 D4), a block field as its
/// defaults block writes it; `None` for anything else.
fn copy(site: &Site<'_>, block: &Block, field: &str, default: &FieldEntry) -> Option<String> {
    let forms = site.view.registries().field_forms();
    let Reading::Declared(form) = forms.reading(site.kind, field) else {
        return None;
    };
    match form.field_type() {
        FieldType::ReferenceList | FieldType::StringList => {
            items_of(&forms.stored(site.kind, default)).map(|items| list_text(&items))
        }
        FieldType::Block => default.value_span.as_ref().map(|span| block.text_of(span)),
        _ => None,
    }
}

/// `field` left holding nothing: its line goes, or, over a default, it is written as none.
fn emptied(
    site: &Site<'_>,
    mut block: Block,
    field: &str,
    over_default: bool,
) -> Result<String, Refusal<EditField>> {
    if !over_default {
        remove_own(&mut block, field);
        return block.finish(site);
    }
    let none = match site
        .view
        .registries()
        .field_forms()
        .reading(site.kind, field)
    {
        Reading::Declared(form)
            if matches!(
                form.field_type(),
                FieldType::ReferenceList | FieldType::StringList
            ) =>
        {
            "[]"
        }
        Reading::Declared(form) if form.field_type() == FieldType::Block => "{}",
        _ => {
            return Err(not_editable(
                EditField::Field,
                format!(
                    "'{field}' comes from the file's defaults; use set to give this block its own"
                ),
            ));
        }
    };
    block.place(site, field, none, None);
    block.finish(site)
}

fn remove_own(block: &mut Block, field: &str) {
    if let Some(span) = block.entry(field).and_then(|e| e.value_span.clone()) {
        block.remove(span.start_line - 1, span.end_line - 1);
    }
}

/// `text` with the own `field` of the entity removed when it holds what the file's default holds, as the graph stores
/// both (ADR 0182 D4).
fn without_repeat(site: &Site<'_>, text: &str, field: &str) -> Result<String, Refusal<EditField>> {
    let mut block = Block::open(site, text)?;
    let forms = site.view.registries().field_forms();
    let repeats = block
        .entry(field)
        .is_some_and(|own| of(site, block.file()).repeats(site.kind, own, forms));
    if !repeats {
        return Ok(text.to_string());
    }
    remove_own(&mut block, field);
    block.finish(site)
}
