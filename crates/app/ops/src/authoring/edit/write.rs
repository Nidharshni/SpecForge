//! The value writer: the text of a field's value, in the form its declaration decides (ADR 0147, ADR 0151) and
//! laid out with the file's format configuration (ADR 0153). What it returns reads back as the value it was
//! given, or the edit is refused.

use super::{EditField, Input};
use crate::{OpError, OpErrorKind, Refusal};
use specforge_formatter::FormatConfig;
use specforge_graph::FieldValue;
use specforge_graph::field_form::{Reading, spell};
use specforge_protocol_types::FieldType;

/// The refusal code of an edit that does not fit the field it names.
pub const NOT_EDITABLE: &str = "not_editable";

/// An edit that does not fit its field's form, refused about `field`.
pub fn not_editable(field: EditField, message: impl Into<String>) -> Refusal<EditField> {
    OpError::new(OpErrorKind::InvalidInput, NOT_EDITABLE, message).about(field)
}

/// `text` as a string value of `key` on a line that starts with `lead`: triple-quoted (its lines at `lead` plus one
/// indent of `config`, the closing quotes at `lead`) when it holds a line break or `lead key "…"` passes the
/// configuration's `max_width`; else, or when the triple-quoted form does not read back, `"…"`.
pub fn string(key: &str, text: &str, lead: &str, config: &FormatConfig) -> String {
    let one = spell::quoted(text);
    if !text.contains('\n') && lead.len() + key.len() + 1 + one.len() <= config.max_width {
        return one;
    }
    let pad = format!("{lead}{}", config.indent_str());
    let body: Vec<String> = text
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect();
    let triple = format!("\"\"\"\n{}\n{lead}\"\"\"", body.join("\n"));
    if reads_back(key, &triple, lead, &Expect::Text(text)) {
        triple
    } else {
        one
    }
}

/// What a written value must read back as.
enum Expect<'a> {
    Text(&'a str),
    Items(&'a [String]),
    Any,
}

/// Whether `key <written>` on a line that starts with `lead` parses, without error, to one field holding `expect`.
fn reads_back(key: &str, written: &str, lead: &str, expect: &Expect<'_>) -> bool {
    let lead = if lead.is_empty() { "  " } else { lead };
    let source = format!("probe probe_entity \"T\" {{\n{lead}{key} {written}\n}}\n");
    let file = specforge_parser::parse(&source, "probe.spec");
    let [entity] = file.entities.as_slice() else {
        return false;
    };
    let [entry] = entity.fields.entries() else {
        return false;
    };
    if !file.errors.is_empty() || entry.key.as_str() != key {
        return false;
    }
    match (expect, &entry.value) {
        (Expect::Text(text), FieldValue::String(read)) => read == text,
        (Expect::Items(items), FieldValue::List(read)) => {
            read.len() == items.len() && read.iter().zip(*items).all(|(r, i)| &r.text == i)
        }
        (Expect::Any, _) => true,
        _ => false,
    }
}

/// `[a, b]` of the already written `items`.
pub fn list(items: &[String]) -> String {
    format!("[{}]", items.join(", "))
}

fn one<'i>(input: &'i Input, key: &str, what: &str) -> Result<&'i str, Refusal<EditField>> {
    match input {
        Input::One(text) => Ok(text),
        Input::Many(items) if items.len() == 1 => Ok(&items[0]),
        _ => Err(not_editable(
            EditField::Value,
            format!("'{key}' takes one {what}; give a single value"),
        )),
    }
}

fn items_of(input: &Input, key: &str, split: bool) -> Result<Vec<String>, Refusal<EditField>> {
    match input {
        Input::Many(items) => Ok(items.clone()),
        Input::One(text) if split => Ok(text
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()),
        Input::One(text) => Ok(vec![text.clone()]),
        Input::Entries(_) => Err(not_editable(
            EditField::Value,
            format!("'{key}' is a list; give its items"),
        )),
    }
}

/// The text after `key ` on a line that starts with `lead` (the key line's leading whitespace as it stands once the
/// block is formatted), laid out with `config` (the file's format configuration; a multi-line value carries its own
/// line breaks), for `input` as the field read as `reading` takes it. `Undeclared` is the caller's refusal
/// (`unknown_field`); the writer reports it as not editable.
pub fn written(
    reading: Reading<'_>,
    key: &str,
    input: &Input,
    lead: &str,
    config: &FormatConfig,
) -> Result<String, Refusal<EditField>> {
    let items: Vec<String>;
    let (text, expect) = match reading {
        Reading::Undeclared => {
            return Err(not_editable(
                EditField::Field,
                format!("'{key}' is not a field of this kind"),
            ));
        }
        Reading::Member => {
            let text = one(input, key, "type")?;
            let text = if reads_back(key, text, lead, &Expect::Any) {
                text.to_string()
            } else {
                spell::quoted(text)
            };
            (text, Expect::Any)
        }
        Reading::Declared(form) => match form.field_type() {
            FieldType::String => {
                let text = one(input, key, "text")?;
                (string(key, text, lead, config), Expect::Text(text))
            }
            FieldType::Identifier
            | FieldType::Enum
            | FieldType::Integer
            | FieldType::Bool
            | FieldType::Reference => {
                let what = match form.field_type() {
                    FieldType::Integer => "integer",
                    FieldType::Bool => "boolean",
                    FieldType::Reference => "id",
                    _ => "word",
                };
                (spell::value(form, one(input, key, what)?), Expect::Any)
            }
            FieldType::ReferenceList => {
                items = items_of(input, key, true)?;
                let written: Vec<String> = items.iter().map(|i| spell::value(form, i)).collect();
                (list(&written), Expect::Items(&items))
            }
            FieldType::StringList => {
                items = items_of(input, key, false)?;
                let written: Vec<String> = items.iter().map(|i| spell::value(form, i)).collect();
                (list(&written), Expect::Items(&items))
            }
            FieldType::Block => return block(key, input, lead, config),
        },
    };
    if reads_back(key, &text, lead, &expect) {
        Ok(text)
    } else {
        Err(unreadable(key))
    }
}

fn unreadable(key: &str) -> Refusal<EditField> {
    not_editable(
        EditField::Value,
        format!("the value for '{key}' cannot be written so that it reads back unchanged"),
    )
}

fn block(
    key: &str,
    input: &Input,
    lead: &str,
    config: &FormatConfig,
) -> Result<String, Refusal<EditField>> {
    let Input::Entries(entries) = input else {
        return Err(not_editable(
            EditField::Value,
            format!("'{key}' is a block; give its entries as name: text"),
        ));
    };
    if entries.is_empty() {
        return Err(not_editable(
            EditField::Value,
            format!("'{key}' needs at least one entry"),
        ));
    }
    let pad = format!("{lead}{}", config.indent_str());
    let lines: Vec<String> = entries
        .iter()
        .map(|(name, text)| format!("{pad}{name} {}", string(name, text, &pad, config)))
        .collect();
    let text = format!("{{\n{}\n{lead}}}", lines.join("\n"));
    if reads_back(key, &text, lead, &Expect::Any) {
        Ok(text)
    } else {
        Err(unreadable(key))
    }
}

#[cfg(test)]
mod tests;
