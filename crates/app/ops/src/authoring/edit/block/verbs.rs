//! The edits that change a block: `set`, `unset`, `retitle`, `verify` and `link` (with `unlink`).

use super::super::defaults::{self, Change};
use super::super::path::{self, FieldPath};
use super::super::write::not_editable;
use super::super::{EditField, Input};
use super::{Block, Items, Site, items_of, list_text, unreadable};
use crate::Refusal;
use specforge_graph::field_form::{Reading, spell};
use specforge_parser::FieldValue;
use specforge_parser::lex::{LexemeKind, lex};
use specforge_protocol_types::FieldType;

fn finish_edit(
    site: &Site<'_>,
    original: &str,
    before: &str,
    block: &Block,
) -> Result<String, Refusal<EditField>> {
    let after = block.finish(site)?;
    Ok(if after == before {
        original.to_string()
    } else {
        after
    })
}

/// `field` (or `field.entry`) set to `input`. The text unchanged when it already holds that.
pub fn set(
    site: &Site<'_>,
    text: &str,
    field: &str,
    input: &Input,
    annotations: Option<&[String]>,
) -> Result<String, Refusal<EditField>> {
    match path::resolve(site, field, None)? {
        FieldPath::Field { name, reading } => defaults::change(site, text, name, |block, _| {
            block.put(site, name, reading, input, annotations)?;
            Ok(Change::Changed)
        }),
        FieldPath::Entry {
            field: outer,
            entry: inner,
        } => defaults::change(site, text, outer, |block, _| {
            let Input::One(one) = input else {
                return Err(not_editable(
                    EditField::Value,
                    format!("'{outer}.{inner}' takes one text"),
                ));
            };
            block.put_entry(site, outer, inner, one, annotations)?;
            Ok(Change::Changed)
        }),
    }
}

/// `field` (or `field.entry`) removed, with the comment lines directly above it. The text unchanged when neither the
/// block nor its file's defaults hold it. An inherited list becomes `[]`, an inherited block field `{}`.
pub fn unset(site: &Site<'_>, text: &str, field: &str) -> Result<String, Refusal<EditField>> {
    match path::resolve(site, field, None)? {
        FieldPath::Entry {
            field: outer,
            entry: inner,
            ..
        } => defaults::change(site, text, outer, |block, _| {
            // Neither the block nor its file's defaults hold the entry: nothing to change (ADR 0151, ADR 0175).
            let Some((outer, Some(entry))) = path::nested(site, block, outer, inner)? else {
                return Ok(Change::Unchanged);
            };
            if matches!(&outer.value, FieldValue::Block(m) if m.entries().len() == 1) {
                return Ok(Change::Emptied);
            }
            let span = entry
                .value_span
                .clone()
                .ok_or_else(|| unreadable(site.id))?;
            block.remove(span.start_line - 1, span.end_line - 1);
            Ok(Change::Changed)
        }),
        FieldPath::Field { name, .. } => {
            defaults::change(site, text, name, |_, _| Ok(Change::Emptied))
        }
    }
}

/// The header title replaced, or added.
pub fn retitle(site: &Site<'_>, text: &str, title: &str) -> Result<String, Refusal<EditField>> {
    let mut block = Block::open(site, text)?;
    let before = block.text();
    let at = block.header();
    let line = block.lines[at].clone();
    let lexemes = lex(&line);
    let quoted_title = spell::quoted(title);
    let new = match lexemes.iter().find(|l| l.kind.is_str()) {
        Some(old) => format!("{}{}{}", &line[..old.start], quoted_title, &line[old.end..]),
        None => {
            let id = lexemes
                .iter()
                .filter(|l| l.is_name())
                .nth(1)
                .ok_or_else(|| unreadable(site.id))?;
            format!("{} {}{}", &line[..id.end], quoted_title, &line[id.end..])
        }
    };
    block.lines[at] = new;
    finish_edit(site, text, &before, &block)
}

/// `verify [kind] "text"` lines: added after the last, or removed. The text unchanged when there is nothing to do.
pub fn verify(
    site: &Site<'_>,
    text: &str,
    kind: Option<&str>,
    description: &str,
    add: bool,
) -> Result<String, Refusal<EditField>> {
    let mut block = Block::open(site, text)?;
    let before = block.text();
    let kind = kind.unwrap_or("");
    let obligations = &block.entity().obligations;
    let found = obligations
        .iter()
        .position(|o| o.kind == kind && o.description == description);
    let lines = block.verify_lines();
    if add {
        if found.is_some() {
            return Ok(text.to_string());
        }
        let kind_text = if kind.is_empty() {
            String::new()
        } else {
            format!("{kind} ")
        };
        block.put_verify(&format!("verify {kind_text}{}", spell::quoted(description)));
    } else {
        let Some((first, last)) = found.and_then(|i| lines.get(i).copied()) else {
            return Ok(text.to_string());
        };
        block.remove(first, last);
    }
    finish_edit(site, text, &before, &block)
}

/// `target` added to, or removed from, the list `field`; an inherited list is copied into an own field first.
pub fn link(
    site: &Site<'_>,
    text: &str,
    field: &str,
    target: &str,
    add: bool,
) -> Result<String, Refusal<EditField>> {
    let reading = match path::resolve(site, field, Some(target))? {
        FieldPath::Entry { field, entry, .. } => {
            return Err(path::whole_field(
                if add { "link" } else { "unlink" },
                field,
                entry,
            ));
        }
        FieldPath::Field { reading, .. } => reading,
    };
    let Reading::Declared(form) = reading else {
        return Err(not_editable(
            EditField::Field,
            format!("'{field}' takes any text; use set"),
        ));
    };
    let item = match form.field_type() {
        FieldType::ReferenceList | FieldType::StringList => spell::value(form, target),
        FieldType::Reference if !add => String::new(),
        _ => {
            return Err(not_editable(
                EditField::Field,
                format!("'{field}' is not a list; use set"),
            ));
        }
    };
    defaults::change(site, text, field, |block, held| {
        let current = match held.entry() {
            Some(entry) => {
                // A list written as one value is its one item, as the graph build stores it (ADR 0093).
                match &*site
                    .view
                    .registries()
                    .field_forms()
                    .stored(site.kind, entry)
                {
                    value @ FieldValue::List(_) => items_of(value),
                    _ if matches!(form.field_type(), FieldType::Reference) => None,
                    _ => {
                        return Err(not_editable(
                            EditField::Field,
                            format!("'{field}' is not a list"),
                        ));
                    }
                }
            }
            None => Some(Items {
                written: Vec::new(),
                texts: Vec::new(),
            }),
        };
        let Some(mut items) = current else {
            // A single `reference` field: unlink removes it when it names the target.
            return Ok(match held.entry().map(|e| &e.value) {
                Some(FieldValue::Identifier(t) | FieldValue::String(t)) if t == target => {
                    Change::Emptied
                }
                _ => Change::Unchanged,
            });
        };
        if let Some(span) = held.own().and_then(|e| e.value_span.as_ref())
            && lex(&block.lines[span.start_line - 1..span.end_line].join("\n"))
                .iter()
                .any(|l| l.kind == LexemeKind::Comment)
        {
            return Err(not_editable(
                EditField::Field,
                format!("'{field}' holds a comment; edit it by hand"),
            ));
        }
        let present = items.texts.iter().any(|t| t == target);
        if add == present {
            return Ok(Change::Unchanged);
        }
        if add {
            items.texts.push(target.to_string());
            items.written.push(item);
        } else {
            let mut keep = items
                .texts
                .iter()
                .map(|t| t != target)
                .collect::<Vec<_>>()
                .into_iter();
            items.written.retain(|_| keep.next().unwrap_or(true));
            items.texts.retain(|t| t != target);
        }
        if items.texts.is_empty() {
            return Ok(Change::Emptied);
        }
        let new = list_text(&items);
        block.place(site, field, &new, None);
        Ok(Change::Changed)
    })
}
