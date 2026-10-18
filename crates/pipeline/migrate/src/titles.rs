//! The `titles` migration step (ADR 0148): a title for every block that has none.

use specforge_graph::{Graph, ModifiedNodeChange, NodeAspect};
use specforge_parser::{header_row, parse};

/// `text` with a title written into every block header that has none, entity blocks and unions alike: the block's
/// doc comment (the `//` lines directly above its header, no blank line between, the format header excluded), each
/// line's text after `//` trimmed and the lines joined by one space, with `\` and `"` escaped; the comment lines
/// are removed. With no doc comment, its id in Title Case words (`CompleteTaskCommand` → "Complete Task Command",
/// `complete_task` → "Complete Task"). `define`, `spec` and `ref` blocks, and text with a syntax error, are left as
/// they are. Idempotent: a text whose blocks all have titles is returned unchanged.
pub(crate) fn add_titles(text: &str) -> String {
    let file = parse(text, "titles.spec");
    if !file.errors.is_empty() {
        return text.to_string();
    }
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
    let format_header = header_row(text);
    // From the last block up, so the line numbers of the blocks above stay valid.
    for entity in file.entities.iter().rev() {
        if entity.title.is_some() || entity.kind.raw.as_str() == "define" {
            continue;
        }
        let row = entity.span.start_line - 1;
        let Some(at) = lines
            .get(row)
            .and_then(|l| after_id(l, entity.id.raw.as_str()))
        else {
            continue;
        };
        let (first, title) = match doc_comment(&lines, row, format_header) {
            Some((first, title)) => (first, title),
            None => (row, words(entity.id.raw.as_str())),
        };
        lines[row].insert_str(at, &format!(" \"{}\"", escape(&title)));
        lines.drain(first..row);
    }
    lines.concat()
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

/// The byte offset just after `id` on a block's header `line` (`kind id`, indented or not), when it stands there.
fn after_id(line: &str, id: &str) -> Option<usize> {
    let word = |s: &str| {
        s.find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(s.len())
    };
    let kind_at = line.len() - line.trim_start().len();
    let kind_end = kind_at + word(&line[kind_at..]);
    let id_at = kind_end + (line[kind_end..].len() - line[kind_end..].trim_start().len());
    let id_end = id_at + word(&line[id_at..]);
    (kind_end > kind_at && id_end > id_at && &line[id_at..id_end] == id).then_some(id_end)
}

/// The doc comment of the block whose header is line `row`: the `//` lines directly above it, as the row of the
/// first and the text joined; none when the line above is not a comment, is the format header or the run is empty.
fn doc_comment(
    lines: &[String],
    row: usize,
    format_header: Option<usize>,
) -> Option<(usize, String)> {
    let mut first = row;
    while first > 0
        && Some(first - 1) != format_header
        && lines[first - 1].trim_start().starts_with("//")
    {
        first -= 1;
    }
    let parts: Vec<&str> = lines[first..row]
        .iter()
        .map(|l| {
            l.trim_start()
                .trim_start_matches("//")
                .trim_start_matches('/')
                .trim()
        })
        .filter(|p| !p.is_empty())
        .collect();
    (first < row && !parts.is_empty()).then(|| (first, parts.join(" ")))
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

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}
