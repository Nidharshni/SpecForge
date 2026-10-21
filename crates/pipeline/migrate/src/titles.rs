//! The `titles` migration step (ADR 0148): a title for every block that has none.

use specforge_common::SourceSpan;
use specforge_graph::field_form::spell;
use specforge_graph::{Graph, ModifiedNodeChange, NodeAspect};
use specforge_parser::parse;

/// `text` with a title written into every block header that has none, entity blocks and unions alike: the block's
/// doc comment as the parse reads it (`Entity::doc_span`: the `//` lines directly above its header, no blank line
/// between, never the format header), each
/// line's text after `//` trimmed and the lines joined by one space, escaped as a `.spec` string (`field_form::spell::quoted`); the comment lines
/// are removed. With no doc comment, its id in Title Case words (`CompleteTaskCommand` → "Complete Task Command",
/// `complete_task` → "Complete Task"). The title is written after the block's name, on whichever line of its
/// header the name stands. `define`, `spec` and `ref` blocks, and text with a syntax error, are left as
/// they are. Idempotent: a text whose blocks all have titles is returned unchanged.
pub(crate) fn add_titles(text: &str) -> String {
    let file = parse(text, "titles.spec");
    if !file.errors.is_empty() {
        return text.to_string();
    }
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
    // From the last block up, so the line numbers of the blocks above stay valid.
    for entity in file.entities.iter().rev() {
        if entity.title.is_some() || !needs_title(entity.kind.raw.as_str()) {
            continue;
        }
        let Some(name) = entity.name_span.as_ref() else {
            continue;
        };
        let row = entity.span.start_line - 1;
        let (first, title) = match entity
            .doc_span
            .as_ref()
            .and_then(|doc| doc_comment(&lines, doc))
        {
            Some((first, title)) => (first, title),
            None => (row, words(entity.id.raw.as_str())),
        };
        lines[name.end_line - 1]
            .insert_str(name.end_col - 1, &format!(" {}", spell::quoted(&title)));
        lines.drain(first..row);
    }
    lines.concat()
}

/// Whether a block of `kind` takes a title: every block the graph places (`define` is never a node, `defaults` takes
/// none: E078).
fn needs_title(kind: &str) -> bool {
    kind != specforge_common::structural::DEFINE && kind != specforge_common::structural::DEFAULTS
}

/// Whether `change` is one this step makes: a node whose only change is its title, which it did not have in
/// `before`, the graph before the migration.
pub(crate) fn added(change: &ModifiedNodeChange, before: &Graph) -> bool {
    change.changed_fields.is_empty()
        && change.changed_aspects == [NodeAspect::Title]
        && before.node(&change.id).is_some_and(|node| {
            node.title
                .as_deref()
                .is_none_or(|title| title.trim().is_empty())
        })
}

/// The text of the doc comment the parse read at `doc`: the row of its first line, and each line's text after `//`
/// trimmed, joined by one space; none when every line is empty.
fn doc_comment(lines: &[String], doc: &SourceSpan) -> Option<(usize, String)> {
    let first = doc.start_line - 1;
    let parts: Vec<&str> = lines[first..doc.end_line]
        .iter()
        .map(|l| {
            l.trim_start()
                .trim_start_matches("//")
                .trim_start_matches('/')
                .trim()
        })
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| (first, parts.join(" ")))
}

/// `id` as Title Case words: split at `_` and before a capital that follows a lowercase letter or a digit.
/// Shortcut: a run of capitals (`HTTPServer`) stays one word; no tracked id has one.
fn words(id: &str) -> String {
    let mut out = String::new();
    let mut previous: Option<char> = None;
    for c in id.chars() {
        if c == '_' {
            previous = None;
            continue;
        }
        let boundary =
            previous.is_none_or(|p| c.is_uppercase() && (p.is_lowercase() || p.is_ascii_digit()));
        if boundary && !out.is_empty() {
            out.push(' ');
        }
        if boundary {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        previous = Some(c);
    }
    if out.is_empty() { id.to_string() } else { out }
}
