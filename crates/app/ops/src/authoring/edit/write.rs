//! The value writer: the text of a field's value, in the form its declaration decides (ADR 0147, ADR 0151).
//! What it returns reads back as the value it was given, or the edit is refused.

use super::{EditField, Input};
use crate::{OpError, OpErrorKind, Refusal};
use specforge_graph::FieldValue;
use specforge_graph::field_form::{Reading, is_word};
use specforge_parser::lex::is_entity_id;
use specforge_protocol_types::FieldType;

/// A one-line `key "…"` longer than this is written triple-quoted (the formatter's default `max_width`).
const MAX_WIDTH: usize = 100;

/// The refusal code of an edit that does not fit the field it names.
pub const NOT_EDITABLE: &str = "not_editable";

/// An edit that does not fit its field's form, refused about `field`.
pub fn not_editable(field: EditField, message: impl Into<String>) -> Refusal<EditField> {
    OpError::new(OpErrorKind::InvalidInput, NOT_EDITABLE, message).about(field)
}

/// `text` as a one-line `"…"` string.
pub fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `text` as a string value of `key` at `indent`: triple-quoted (content two columns in, the closing quotes at
/// `indent`) when it holds a line break or `key "…"` passes the width; else, or when the triple-quoted form does
/// not read back, `"…"`.
pub fn string(key: &str, text: &str, indent: usize) -> String {
    let one = quoted(text);
    if !text.contains('\n') && indent + key.len() + 1 + one.len() <= MAX_WIDTH {
        return one;
    }
    let pad = " ".repeat(indent + 2);
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
    let triple = format!("\"\"\"\n{}\n{}\"\"\"", body.join("\n"), " ".repeat(indent));
    if reads_back(key, &triple, indent, &Expect::Text(text)) {
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

/// Whether `key <written>` at `indent` parses, without error, to one field holding `expect`.
fn reads_back(key: &str, written: &str, indent: usize, expect: &Expect<'_>) -> bool {
    let source = format!(
        "probe probe_entity \"T\" {{\n{}{key} {written}\n}}\n",
        " ".repeat(indent.max(2))
    );
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

/// A bare word, else quoted.
fn word_or_quoted(text: &str) -> String {
    if is_word(text) && !matches!(text, "true" | "false") {
        text.to_string()
    } else {
        quoted(text)
    }
}

/// An entity id (a word or a scheme reference), bare; else quoted.
fn id_or_quoted(text: &str) -> String {
    if is_entity_id(text) {
        text.to_string()
    } else {
        quoted(text)
    }
}

/// A string-list item: quoted, except a scheme reference id (`gh.issue:7`), which stays bare.
fn string_item(text: &str) -> String {
    if text.contains(':') && is_entity_id(text) {
        text.to_string()
    } else {
        quoted(text)
    }
}

/// One item of a list field of type `field_type`, as it is written.
pub fn list_item(field_type: FieldType, text: &str) -> String {
    if field_type == FieldType::ReferenceList {
        id_or_quoted(text)
    } else {
        string_item(text)
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

/// The text after `key ` on its line (a multi-line value carries its own line breaks, indented for a key at
/// `indent`) for `input` as the field read as `reading` takes it. `Undeclared` is the caller's refusal
/// (`unknown_field`); the writer reports it as not editable.
pub fn written(
    reading: Reading<'_>,
    key: &str,
    input: &Input,
    indent: usize,
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
            let text = if reads_back(key, text, indent, &Expect::Any) {
                text.to_string()
            } else {
                quoted(text)
            };
            (text, Expect::Any)
        }
        Reading::Declared(form) => match form.field_type() {
            FieldType::String => {
                let text = one(input, key, "text")?;
                (string(key, text, indent), Expect::Text(text))
            }
            FieldType::Identifier | FieldType::Enum => {
                (word_or_quoted(one(input, key, "word")?), Expect::Any)
            }
            FieldType::Integer => {
                let text = one(input, key, "integer")?;
                match text.trim().parse::<i64>() {
                    Ok(n) => (n.to_string(), Expect::Any),
                    Err(_) => (quoted(text), Expect::Any),
                }
            }
            FieldType::Bool => {
                let text = one(input, key, "boolean")?;
                match text {
                    "true" | "false" => (text.to_string(), Expect::Any),
                    _ => (quoted(text), Expect::Any),
                }
            }
            FieldType::Reference => (id_or_quoted(one(input, key, "id")?), Expect::Any),
            FieldType::ReferenceList => {
                items = items_of(input, key, true)?;
                let written: Vec<String> = items.iter().map(|i| id_or_quoted(i)).collect();
                (list(&written), Expect::Items(&items))
            }
            FieldType::StringList => {
                items = items_of(input, key, false)?;
                let written: Vec<String> = items.iter().map(|i| string_item(i)).collect();
                (list(&written), Expect::Items(&items))
            }
            FieldType::Block => return block(key, input, indent),
        },
    };
    if reads_back(key, &text, indent, &expect) {
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

fn block(key: &str, input: &Input, indent: usize) -> Result<String, Refusal<EditField>> {
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
    let pad = " ".repeat(indent + 2);
    let lines: Vec<String> = entries
        .iter()
        .map(|(name, text)| format!("{pad}{name} {}", string(name, text, indent + 2)))
        .collect();
    let text = format!("{{\n{}\n{}}}", lines.join("\n"), " ".repeat(indent));
    if reads_back(key, &text, indent, &Expect::Any) {
        Ok(text)
    } else {
        Err(unreadable(key))
    }
}

#[cfg(test)]
mod tests;
