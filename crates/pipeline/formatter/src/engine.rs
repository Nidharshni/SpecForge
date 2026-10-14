use crate::config::FormatConfig;
use specforge_common::{Diagnostic, SourceSpan, Sym, codes};
use specforge_parser::cst::Cst;
use specforge_parser::lex::lex;
use tree_sitter::Node;
use tree_sitter_specforge::kind;

mod block;
mod emitter;
use emitter::Emitter;

/// Result of formatting a source string.
#[derive(Debug, Clone)]
pub struct FormatResult {
    pub formatted: String,
    pub diagnostics: Vec<Diagnostic>,
}

/// Format a complete .spec source string: every top-level form as the
/// parser reads it (`specforge_parser::cst`), a form the parser could not
/// read whole kept as written, with one W142 spanning its lines.
pub fn format_source(source: &str, config: &FormatConfig) -> FormatResult {
    let cst = match Cst::read(source, None) {
        Ok(cst) => cst,
        Err(_) => {
            return FormatResult {
                formatted: source.to_string(),
                diagnostics: vec![Diagnostic::new(codes::E001, "Failed to parse source")],
            };
        }
    };
    let source_lines: Vec<&str> = source.lines().collect();
    let mut emitter = Emitter::new(&source_lines, config);
    for piece in cst.pieces() {
        emitter.piece(piece);
    }
    emitter.finish()
}

/// Format a range of lines within a source string.
/// The range is expanded to complete block boundaries. Every byte outside the
/// widened lines is the source's, line endings included; the widened lines
/// read as `format_source` writes them.
pub fn format_range(
    source: &str,
    start_line: usize,
    end_line: usize,
    config: &FormatConfig,
) -> FormatResult {
    let cst = match Cst::read(source, None) {
        Ok(cst) => cst,
        Err(_) => {
            return FormatResult {
                formatted: source.to_string(),
                diagnostics: vec![],
            };
        }
    };

    // Find the blocks that overlap with the requested range
    let (expanded_start, expanded_end) = expand_to_block_boundaries(&cst, start_line, end_line);

    let lines: Vec<&str> = source.lines().collect();
    // Blank lines at either end of the range lie between blocks: they stay
    // as they are (formatted alone, the range would lose them).
    let is_blank = |row: usize| lines[row].trim().is_empty();
    let mut expanded_start = expanded_start;
    let mut expanded_end = expanded_end.min(lines.len().saturating_sub(1));
    while expanded_start <= expanded_end && expanded_start < lines.len() && is_blank(expanded_start)
    {
        expanded_start += 1;
    }
    while expanded_end > expanded_start && is_blank(expanded_end) {
        expanded_end -= 1;
    }
    if expanded_start > expanded_end || expanded_start >= lines.len() {
        // Nothing but blank lines (or nothing at all) to format.
        return FormatResult {
            formatted: source.to_string(),
            diagnostics: vec![],
        };
    }

    // Each line's first byte; a line's terminator belongs to it.
    let starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let from = starts[expanded_start];
    let to = starts
        .get(expanded_end + 1)
        .copied()
        .unwrap_or(source.len());
    let range_source = &source[from..to];
    let range_result = format_source(range_source, config);
    let mut middle = range_result.formatted;
    // The file's last line, unterminated: so is its formatted text (the final newline is the whole
    // document's to add).
    if !range_source.ends_with('\n') && middle.ends_with('\n') {
        middle.pop();
    }
    let formatted = format!("{}{middle}{}", &source[..from], &source[to..]);

    // The range was formatted as its own source: its regions are reported
    // at their lines in the whole document.
    let diagnostics = range_result
        .diagnostics
        .into_iter()
        .map(|d| match &d.span {
            Some(span) if d.is(codes::W142) => kept_region(
                span.start_line - 1 + expanded_start,
                span.end_line - 1 + expanded_start,
                span.end_col - 1,
            ),
            _ => d,
        })
        .collect();

    FormatResult {
        formatted,
        diagnostics,
    }
}

/// W142 for a region kept verbatim, rows `first..=last` (0-based), the last
/// `last_line_len` bytes long: spanned from the start of its first line to
/// the end of its last (1-based lines and columns, end exclusive; the file
/// is left for the caller, who knows the document).
fn kept_region(first: usize, last: usize, last_line_len: usize) -> Diagnostic {
    Diagnostic::new(
        codes::W142,
        format!(
            "Parse error at lines {}-{}, error region preserved verbatim",
            first + 1,
            last + 1,
        ),
    )
    .with_span(SourceSpan {
        file: Sym::new(""),
        start_line: first + 1,
        start_col: 1,
        end_line: last + 1,
        end_col: last_line_len + 1,
    })
}

/// Collect error regions (start_row, end_row) from the CST.
fn collect_error_regions(root: Node) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    collect_errors_recursive(root, &mut regions);
    // Merge overlapping regions
    regions.sort_by_key(|r| r.0);
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for region in regions {
        if let Some(last) = merged.last_mut()
            && region.0 <= last.1 + 1
        {
            last.1 = last.1.max(region.1);
            continue;
        }
        merged.push(region);
    }
    merged
}

fn collect_errors_recursive(node: Node, regions: &mut Vec<(usize, usize)>) {
    if node.is_error() || node.is_missing() {
        regions.push((node.start_position().row, node.end_position().row));
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_errors_recursive(cursor.node(), regions);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if a line falls within an error region.
fn in_error_region(line: usize, error_regions: &[(usize, usize)]) -> bool {
    error_regions
        .iter()
        .any(|(start, end)| line >= *start && line <= *end)
}

/// Expand a line range to complete block boundaries (the top-level items of
/// every piece, at their document rows).
fn expand_to_block_boundaries(cst: &Cst, start_line: usize, end_line: usize) -> (usize, usize) {
    let mut expanded_start = start_line;
    let mut expanded_end = end_line;

    for piece in cst.pieces() {
        let root = piece.root();
        let mut cursor = root.walk();
        for node in root.children(&mut cursor) {
            let node_start = piece.row() + node.start_position().row;
            let node_end = piece.row() + node.end_position().row;

            // If the block overlaps with our range, expand to include the full block
            if node_end >= start_line && node_start <= end_line {
                expanded_start = expanded_start.min(node_start);
                expanded_end = expanded_end.max(node_end);
            }
        }
    }

    (expanded_start, expanded_end)
}

fn node_text<'s>(node: Node, source: &'s str) -> &'s str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

fn overlaps(node: Node, error_regions: &[(usize, usize)]) -> bool {
    (node.start_position().row..=node.end_position().row)
        .any(|row| in_error_region(row, error_regions))
}

/// Whether a comment sits between `node`'s tokens, outside the values the
/// formatter keeps comments in by itself (a nested block's members, a
/// list kept as written).
fn holds_comment(node: Node) -> bool {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children.into_iter().any(|child| match child.kind() {
        kind::COMMENT => true,
        kind::NESTED_BLOCK | kind::LIST => false,
        _ => holds_comment(child),
    })
}

/// `node`'s text on one line: its lexemes (`specforge_parser::lex`), one
/// space where whitespace separated two. `node` holds no comment
/// (`holds_comment`); a string stays as written.
fn one_line(node: Node, source: &str) -> String {
    let text = node_text(node, source);
    let mut out = String::with_capacity(text.len());
    let mut previous_end = None;
    for lexeme in lex(text) {
        if previous_end.is_some_and(|end| end < lexeme.start) {
            out.push(' ');
        }
        out.push_str(lexeme.text(text));
        previous_end = Some(lexeme.end);
    }
    out
}

/// `node`'s text as written: the first line from the node's start, the
/// rest with their own indentation, trailing whitespace trimmed outside
/// strings.
fn as_written(node: Node, source: &str) -> Vec<String> {
    let mut lines = lines_keeping_strings(node, source);
    if let Some(first) = lines.first_mut() {
        *first = first.trim_start().to_string();
    }
    lines
}

/// `node`'s text split into lines, trailing whitespace trimmed, except at a
/// line break inside a string literal: a string's lines are its value, so
/// its trailing whitespace is kept (the formatter keeps strings
/// byte-for-byte).
fn lines_keeping_strings(node: Node, source: &str) -> Vec<String> {
    fn strings(node: Node, found: &mut Vec<(usize, usize)>) {
        if matches!(node.kind(), kind::STRING | kind::TRIPLE_QUOTED_STRING) {
            found.push((node.start_byte(), node.end_byte()));
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            strings(child, found);
        }
    }
    let mut spans = Vec::new();
    strings(node, &mut spans);
    let text = node_text(node, source);
    let mut offset = node.start_byte();
    let mut pieces: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        pieces.pop();
    }
    pieces
        .into_iter()
        .map(|piece| {
            let line_break = offset + piece.len();
            offset = line_break + 1;
            let inside = spans.iter().any(|&(a, b)| a < line_break && line_break < b);
            let piece = piece.strip_suffix('\r').unwrap_or(piece);
            if inside {
                piece.to_string()
            } else {
                piece.trim_end().to_string()
            }
        })
        .collect()
}

/// A comment's text: trailing whitespace trimmed, and a space inserted in
/// `//text`. Anything else is kept as written (`///`, `//!`, indentation
/// after `//`).
fn comment_text(node: Node, source: &str) -> String {
    normalize_comment(node_text(node, source).trim_end())
}

/// Normalize comment spacing: `//text` gets one space after `//`, and
/// trailing whitespace goes. Everything else is the author's: `///` and
/// `//!` stay, and so does indentation after `//` (nested bullets, tables).
fn normalize_comment(line: &str) -> String {
    let line = line.trim_end();
    let trimmed = line.trim_start();
    let leading_ws = &line[..line.len() - trimmed.len()];
    match trimmed.strip_prefix("//") {
        Some(rest)
            if rest
                .chars()
                .next()
                .is_some_and(|c| !c.is_whitespace() && c != '/' && c != '!') =>
        {
            format!("{leading_ws}// {rest}")
        }
        _ => line.to_string(),
    }
}

#[cfg(test)]
mod methods_tests;

#[cfg(test)]
mod tests;

/// The emitter keeps every comment where it was, keeps statement order, and
/// never splits a value: each test pins one bug of the previous engine.
#[cfg(test)]
mod emitter_tests;
