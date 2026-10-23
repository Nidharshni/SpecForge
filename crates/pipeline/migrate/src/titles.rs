//! The `titles` migration step (ADR 0148, ADR 0225): a title for every block that has none or a blank one.

use specforge_common::{SourceSpan, structural};
use specforge_graph::field_form::spell;
use specforge_graph::{Graph, ModifiedNodeChange, NodeAspect, title};
use specforge_parser::parse;

/// `text` with a title written for every block E076 reports but a spec block (its title is its name), entity
/// blocks, unions and refs alike: the block's doc comment as the parse reads it (`Entity::doc_span`: the `//` lines
/// directly above its header, no blank line between, never the format header), each
/// line's text after `//` trimmed and the lines joined by one space, escaped as a `.spec` string (`field_form::spell::quoted`); the comment lines
/// are removed. With no doc comment, its id in Title Case words (`CompleteTaskCommand` → "Complete Task Command",
/// `complete_task` → "Complete Task"). The title is written after the block's name, on whichever line of its
/// header the name stands, or in place of a blank one (only whitespace) where the parse read it. `define`,
/// `defaults` and `spec` blocks, titled blocks and text with a syntax error are left as they are. Idempotent: a
/// text whose blocks all have titles is returned unchanged.
pub(crate) fn add_titles(text: &str) -> String {
    let file = parse(text, "titles.spec");
    if !file.errors.is_empty() {
        return text.to_string();
    }
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
    // From the last block up, so the line numbers of the blocks above stay valid.
    for entity in file.entities.iter().rev() {
        // A spec block's title is its name: writing one would rename it (ADR 0187 D4).
        if !title::owes_title(entity) || entity.kind.raw == structural::SPEC {
            continue;
        }
        let Some(name) = entity.name_span.as_ref() else {
            continue;
        };
        let row = entity.span.start_line - 1;
        let (first, text) = match entity
            .doc_span
            .as_ref()
            .and_then(|doc| doc_comment(&lines, doc))
        {
            Some((first, text)) => (first, text),
            None => (row, words(entity.id.raw.as_str())),
        };
        let quoted = spell::quoted(&text);
        match &entity.title_span {
            Some(blank) => replace(&mut lines, blank, &quoted),
            None => lines[name.end_line - 1].insert_str(name.end_col - 1, &format!(" {quoted}")),
        }
        lines.drain(first..row);
    }
    lines.concat()
}

/// `lines` with the text `span` covers replaced by `text`; a span over several lines (a string holding a newline)
/// becomes one line.
fn replace(lines: &mut Vec<String>, span: &SourceSpan, text: &str) {
    let (first, last) = (span.start_line - 1, span.end_line - 1);
    let joined = format!(
        "{}{text}{}",
        &lines[first][..span.start_col - 1],
        &lines[last][span.end_col - 1..]
    );
    lines.splice(first..=last, [joined]);
}

/// Whether `change` is one this step makes: a node whose only change is its title, which it did not have (or had blank) in
/// `before`, the graph before the migration.
pub(crate) fn added(change: &ModifiedNodeChange, before: &Graph) -> bool {
    change.changed_fields.is_empty()
        && change.changed_aspects == [NodeAspect::Title]
        && before
            .node(&change.id)
            .is_some_and(|node| title::is_untitled(node.title.as_deref()))
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
