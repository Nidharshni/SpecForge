//! The `defaults` migration step (ADR 0149): the reference lists a file's blocks repeat, stated once.

use specforge_common::structural;
use specforge_graph::{FieldForms, FileDefaults};
use specforge_parser::{Entity, FieldEntry, FieldValue, Written, parse};

/// `text` with each reference list its blocks repeat moved into a `defaults <kind>` block (ADR 0149), the graph
/// unchanged: for each kind with two or more blocks in the file and no `defaults` block of its own, a field is
/// hoisted when every block of the kind writes it exactly once, as a one-line list of bare items with no annotation
/// and no comment on or directly above its line, and at least two blocks write the same list; that list becomes the
/// default and is deleted from those blocks, the others keep theirs. A kind's block is written only when it saves at
/// least one line. The block goes after the last import, or before the first form's doc comment as the parse reads
/// it (below the format header), with its keys padded to the longest; the layout of the edited blocks is left to
/// `specforge format`. A file with a syntax error is returned unchanged. A field is hoisted only where the graph's
/// defaults rule applies it under `forms` (`FileDefaults::entry`, ADR 0149 D4, ADR 0154 D1): the forms the migrate
/// operation's judge compiles with, so the hoist keeps the graph by construction and the judge's graph comparison is
/// the safety net (ADR 0186).
pub(crate) fn hoist_defaults(text: &str, forms: &FieldForms) -> String {
    let file = parse(text, "defaults.spec");
    if !file.errors.is_empty() {
        return text.to_string();
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut kinds: Vec<&str> = Vec::new();
    for entity in &file.entities {
        let kind = entity.kind.raw.as_str();
        if !structural::is_structural(kind) && !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    let has_defaults = |kind: &str| {
        file.entities
            .iter()
            .any(|e| e.kind.raw == structural::DEFAULTS && e.id.raw == kind)
    };
    let mut removed: Vec<usize> = Vec::new();
    let mut blocks: Vec<String> = Vec::new();
    for kind in kinds.into_iter().filter(|k| !has_defaults(k)) {
        let of_kind: Vec<&Entity> = file
            .entities
            .iter()
            .filter(|e| e.kind.raw == kind)
            .collect();
        if of_kind.len() < 2 {
            continue;
        }
        let mut fields: Vec<(String, String)> = Vec::new();
        let mut rows: Vec<usize> = Vec::new();
        for entry in of_kind[0].fields.entries() {
            let key = entry.key.as_str();
            let Some(listed) = of_kind
                .iter()
                .map(|e| one_line_list(&lines, e, key))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            // The most common list, the first written on a tie; at least two blocks write it.
            let common = listed
                .iter()
                .max_by_key(|l| {
                    (
                        listed.iter().filter(|o| o.items == l.items).count(),
                        std::cmp::Reverse(l.row),
                    )
                })
                .expect("a kind has blocks");
            let same: Vec<_> = listed.iter().filter(|l| l.items == common.items).collect();
            if same.len() < 2
                || fields.iter().any(|(k, _)| k == key)
                || !applied(forms, kind, key, &common.source)
            {
                continue;
            }
            fields.push((key.to_string(), common.source.clone()));
            rows.extend(same.iter().map(|l| l.row));
        }
        // The block is the header, the closing brace and a blank line, beside its fields.
        if rows.len() <= fields.len() + 3 {
            continue;
        }
        let width = fields.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        let mut block = format!("defaults {kind} {{\n");
        for (key, value) in &fields {
            block.push_str(&format!("  {key:<width$} {value}\n"));
        }
        block.push_str("}\n");
        blocks.push(block);
        removed.extend(rows);
    }
    if blocks.is_empty() {
        return text.to_string();
    }
    let (at, after_imports) = match file.imports.iter().map(|i| i.span.end_line).max() {
        Some(end) => (end, true),
        None => (first_form_row(&file.entities[0]), false),
    };
    let mut out = String::with_capacity(text.len());
    for index in 0..=lines.len() {
        if index == at {
            if after_imports {
                out.push('\n');
                out.push_str(&blocks.join("\n"));
                if lines.get(index).is_some_and(|l| !l.trim().is_empty()) {
                    out.push('\n');
                }
            } else {
                for block in &blocks {
                    out.push_str(block);
                    out.push('\n');
                }
            }
        }
        if let Some(line) = lines.get(index)
            && !removed.contains(&index)
        {
            out.push_str(line);
        }
    }
    out
}

/// Whether the graph's defaults rule applies `key`, written `value`, in a `defaults <kind>` block under `forms`:
/// asked of [`FileDefaults`] over that one-field block, so E077 is decided where the graph decides it.
fn applied(forms: &FieldForms, kind: &str, key: &str, value: &str) -> bool {
    let probe = parse(
        &format!("defaults {kind} {{\n  {key} {value}\n}}\n"),
        "defaults.spec",
    );
    FileDefaults::of(&probe, forms).0.entry(kind, key).is_some()
}

/// A field a block writes as one line holding a list of bare items.
struct Listed {
    /// The line's 0-based row.
    row: usize,
    items: Vec<String>,
    /// The list as written, brackets included.
    source: String,
}

/// `entity`'s field `key` when it is written exactly once, annotation-free, as a non-empty one-line list of bare
/// items alone on its line, with no comment directly above it.
fn one_line_list(lines: &[&str], entity: &Entity, key: &str) -> Option<Listed> {
    let mut entries = entity.fields.entries().iter().filter(|e| e.key == key);
    let entry: &FieldEntry = entries.next()?;
    if entries.next().is_some() || !entry.annotations.is_empty() {
        return None;
    }
    let FieldValue::List(items) = &entry.value else {
        return None;
    };
    if items.is_empty() || items.iter().any(|i| i.written != Written::Bare) {
        return None;
    }
    let span = entry.value_span.as_ref()?;
    if span.start_line != span.end_line {
        return None;
    }
    let row = span.start_line - 1;
    let line = lines.get(row)?;
    let source = line.get(span.start_col - 1..span.end_col - 1)?;
    let written = line.trim().strip_prefix(key)?;
    let above_is_comment = row > 0 && lines[row - 1].trim_start().starts_with("//");
    (written.starts_with(char::is_whitespace) && written.trim() == source && !above_is_comment)
        .then(|| Listed {
            row,
            items: items.iter().map(|i| i.text.clone()).collect(),
            source: source.to_string(),
        })
}

/// The 0-based row before which a block goes when the file has no import: the first line of the first form's doc
/// comment as the parse read it, else its header. Never above the format header, which is no doc comment (ADR 0202).
fn first_form_row(first: &Entity) -> usize {
    first
        .doc_span
        .as_ref()
        .map_or(first.span.start_line - 1, |doc| doc.start_line - 1)
}
