//! How a value is spelled in `.spec` text (ADR 0147, ADR 0153): the one escape table ([`escaped`], [`quoted`]), the one rule for when a
//! value may stand bare, and each declared type's one-line spelling. `FieldForms::rewrite` (format's rewrite and
//! W159), the typed-edit writer, migrate's `titles` step and W159's message read it; nothing else quotes a value. It also says where a
//! quoted value writes the id it reads ([`id_span`]).

use specforge_common::SourceSpan;
use specforge_parser::Written;
use specforge_parser::lex::is_entity_id;
use specforge_protocol_types::FieldType;

use super::{FieldForm, is_word};

/// `text` as the inside of a one-line `"…"`: `\`, `"`, line feed, tab and carriage return escaped, every other
/// character as it is. What a template writes between quotes; [`quoted`] is this between two `"`.
pub fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
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
    out
}

/// `text` as a one-line `"…"`: [`escaped`] between quotes. The parser's string reading gives `text` back.
pub fn quoted(text: &str) -> String {
    format!("\"{}\"", escaped(text))
}

/// Where a value written `written` at `span` writes the id `id` it reads (ADR 0218): one byte inside each quote of a
/// `"…"` on one line exactly two bytes wider than `id`, three inside each of a `"""…"""` exactly six wider when `id`
/// is an entity id; else `span` (a bare word, a scheme ref or a literal is its text; a value with an escape, padding
/// or a line break has no bytes that spell its id).
///
/// Exact, not a guess: a `"…"` reads each escape (see [`quoted`]) as one character and keeps any other byte, so
/// content as long as `id` holds no escape and is `id`. An entity id holds none of the escaped characters, so a value
/// six bytes wider than one is `"""id"""`.
pub fn id_span(written: Written, span: &SourceSpan, id: &str) -> SourceSpan {
    let width = span.end_col.checked_sub(span.start_col);
    let one_line = span.start_line == span.end_line;
    let quote = match (written, width) {
        (Written::Quoted, Some(w)) if one_line && w == id.len() + 2 => 1,
        (Written::Quoted, Some(w)) if one_line && w == id.len() + 6 && is_entity_id(id) => 3,
        _ => return span.clone(),
    };
    SourceSpan {
        start_col: span.start_col + quote,
        end_col: span.end_col - quote,
        ..span.clone()
    }
}

/// `text` written as `written`: `Quoted` through [`quoted`]; bare, a scheme ref or a literal as it is.
pub fn spelled(text: &str, written: Written) -> String {
    match written {
        Written::Quoted => quoted(text),
        Written::Bare | Written::SchemeRef | Written::Literal => text.to_string(),
    }
}

/// Whether `text` may stand bare as a value of a field of `field_type` whose form is bare (ADR 0147 D1): a word,
/// or on a `reference_list` an entity id (a scheme ref id too, ADR 0153 D3); never `true` / `false` (they read back
/// as booleans). A scalar `reference` takes a word only: a scheme ref id does not parse bare outside a list.
pub fn holds_bare(field_type: FieldType, text: &str) -> bool {
    !matches!(text, "true" | "false")
        && match field_type {
            FieldType::ReferenceList => is_entity_id(text),
            _ => is_word(text),
        }
}

/// How `text`, one value of a field of `form` or one item of its list, is written on one line:
///
/// | declared | written |
/// |---|---|
/// | `integer` | the integer, when `text` is one; else quoted (E061 reports it) |
/// | `bool` | `true` / `false`; anything else quoted (E061) |
/// | `string_list` item | a scheme ref id bare (it then names an entity, ADR 0093); else quoted |
/// | `reference_list` item | an entity id bare (a word or a scheme ref id), never `true` / `false`; else quoted |
/// | `reference`, `enum`, `identifier` | bare when [`holds_bare`] (a word); else quoted |
/// | `string`, `block` | quoted |
pub fn value(form: &FieldForm, text: &str) -> String {
    let field_type = form.field_type();
    match field_type {
        FieldType::Integer => text
            .trim()
            .parse::<i64>()
            .map_or_else(|_| quoted(text), |n| n.to_string()),
        FieldType::Bool if matches!(text, "true" | "false") => text.to_string(),
        FieldType::StringList if text.contains(':') && is_entity_id(text) => text.to_string(),
        _ => match form.written() {
            Some(Written::Bare) if holds_bare(field_type, text) => text.to_string(),
            _ => quoted(text),
        },
    }
}
