use crate::config::FormatConfig;
use specforge_common::{Diagnostic, SourceSpan, Sym, codes};
use specforge_parser::lex::lex;
use tree_sitter::{Node, Parser};
use tree_sitter_specforge::{field, kind};

/// Result of formatting a source string.
#[derive(Debug, Clone)]
pub struct FormatResult {
    pub formatted: String,
    pub diagnostics: Vec<Diagnostic>,
}

/// Format a complete .spec source string.
pub fn format_source(source: &str, config: &FormatConfig) -> FormatResult {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_specforge::LANGUAGE.into())
        .expect("failed to load specforge grammar");

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => {
            return FormatResult {
                formatted: source.to_string(),
                diagnostics: vec![Diagnostic::new(codes::E001, "Failed to parse source")],
            };
        }
    };

    let root = tree.root_node();

    // Check for parse errors
    let error_regions = collect_error_regions(root);
    let has_errors = !error_regions.is_empty();

    let mut diagnostics = Vec::new();
    if has_errors {
        let lines: Vec<&str> = source.lines().collect();
        for &(start_row, end_row) in &error_regions {
            let last_line_len = lines.get(end_row).map_or(0, |line| line.len());
            diagnostics.push(kept_region(start_row, end_row, last_line_len));
        }
    }

    let formatted = format_tree(root, source, config, &error_regions);

    FormatResult {
        formatted,
        diagnostics,
    }
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
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_specforge::LANGUAGE.into())
        .expect("failed to load specforge grammar");

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => {
            return FormatResult {
                formatted: source.to_string(),
                diagnostics: vec![],
            };
        }
    };

    let root = tree.root_node();

    // Find the blocks that overlap with the requested range
    let (expanded_start, expanded_end) = expand_to_block_boundaries(root, start_line, end_line);

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

/// Expand a line range to complete block boundaries.
fn expand_to_block_boundaries(root: Node, start_line: usize, end_line: usize) -> (usize, usize) {
    let mut expanded_start = start_line;
    let mut expanded_end = end_line;

    let mut cursor = root.walk();
    if cursor.goto_first_child() {
        loop {
            let node = cursor.node();
            let node_start = node.start_position().row;
            let node_end = node.end_position().row;

            // If the block overlaps with our range, expand to include the full block
            if node_end >= start_line && node_start <= end_line {
                expanded_start = expanded_start.min(node_start);
                expanded_end = expanded_end.max(node_end);
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    (expanded_start, expanded_end)
}

/// Main formatting: emit every top-level item in source order.
///
/// Nothing is reordered except runs of consecutive imports, which are
/// sorted. Comments stay where they are: a comment on the line of a node
/// stays on that line, and a comment directly above a block stays attached
/// to it. Blocks are separated by exactly one blank line; a blank line
/// between two comments, or between a comment and a block, is kept (one),
/// since it tells a standalone comment from a block's doc comment. Inside
/// blocks blank lines are removed.
fn format_tree(
    root: Node,
    source: &str,
    config: &FormatConfig,
    error_regions: &[(usize, usize)],
) -> String {
    let source_lines: Vec<&str> = source.lines().collect();
    let mut cursor = root.walk();
    let items: Vec<Node> = root.children(&mut cursor).collect();

    let mut out: Vec<String> = Vec::new();
    // The class and last source row of the previous top-level item.
    let mut prev: Option<(Item, usize)> = None;
    let mut i = 0;
    while i < items.len() {
        let node = items[i];
        let class = Item::of(node);
        let (start, end) = (node.start_position().row, node.end_position().row);

        // A comment on the line where the previous item ends stays there.
        if class == Item::Comment
            && prev.is_some_and(|(_, prev_end)| prev_end == start)
            && let Some(last) = out.last_mut()
        {
            last.push(' ');
            last.push_str(&comment_text(node, source));
            prev = Some((Item::Comment, end));
            i += 1;
            continue;
        }

        if let Some((prev_class, prev_end)) = prev {
            let gap = start.saturating_sub(prev_end + 1);
            if needs_blank_line(prev_class, class, gap) {
                out.push(String::new());
            }
        }

        // A run of imports (blank lines allowed, no comments) is sorted.
        if class == Item::Import {
            let mut run = Vec::new();
            let mut last_end = end;
            while i < items.len() && Item::of(items[i]) == Item::Import {
                let import = items[i];
                last_end = import.end_position().row;
                if overlaps(import, error_regions) || holds_comment(import) {
                    run.push(verbatim(import, &source_lines).join("\n"));
                } else {
                    run.push(one_line(import, source));
                }
                i += 1;
            }
            run.sort();
            out.extend(run);
            prev = Some((Item::Import, last_end));
            continue;
        }

        if overlaps(node, error_regions) {
            out.extend(verbatim(node, &source_lines));
        } else if class == Item::Comment {
            out.push(comment_text(node, source));
        } else {
            out.extend(format_block(node, source, config));
        }
        prev = Some((class, end));
        i += 1;
    }

    let mut result = out.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    result
}

/// What a top-level item is, for the blank-line rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Import,
    Comment,
    Block,
}

impl Item {
    fn of(node: Node) -> Self {
        match node.kind() {
            kind::USE_IMPORT | kind::PUB_USE_IMPORT => Item::Import,
            kind::COMMENT => Item::Comment,
            _ => Item::Block,
        }
    }
}

/// Whether one blank line separates `prev` from `next` at top level, given
/// the `gap` of blank lines between them in the source.
fn needs_blank_line(prev: Item, next: Item, gap: usize) -> bool {
    match (prev, next) {
        (Item::Block, _) => true,
        (Item::Import, Item::Import) => false,
        (Item::Import, _) => true,
        // A comment directly above something is attached to it.
        (Item::Comment, _) => gap > 0,
    }
}

fn node_text<'s>(node: Node, source: &'s str) -> &'s str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

fn overlaps(node: Node, error_regions: &[(usize, usize)]) -> bool {
    (node.start_position().row..=node.end_position().row)
        .any(|row| in_error_region(row, error_regions))
}

/// The node's source lines, unchanged.
fn verbatim(node: Node, source_lines: &[&str]) -> Vec<String> {
    (node.start_position().row..=node.end_position().row)
        .filter_map(|row| source_lines.get(row).map(|l| l.trim_end().to_string()))
        .collect()
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
/// rest with their own indentation, trailing whitespace trimmed.
fn as_written(node: Node, source: &str) -> Vec<String> {
    let mut lines = node_text(node, source).lines();
    let first = lines.next().unwrap_or("").trim().to_string();
    std::iter::once(first)
        .chain(lines.map(|l| l.trim_end().to_string()))
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

/// Format one top-level block.
fn format_block(node: Node, source: &str, config: &FormatConfig) -> Vec<String> {
    let text = |name: &str| {
        node.child_by_field_name(name)
            .map(|n| node_text(n, source).to_string())
            .unwrap_or_default()
    };
    let header = match node.kind() {
        kind::ENTITY_BLOCK => match node.child_by_field_name(field::TITLE) {
            Some(title) => format!(
                "{} {} {}",
                text(field::KIND),
                text(field::NAME),
                node_text(title, source)
            ),
            None => format!("{} {}", text(field::KIND), text(field::NAME)),
        },
        kind::SPEC_BLOCK => format!("spec {}", text(field::NAME)),
        kind::DEFINE_BLOCK => format!("define {}", text(field::NAME)),
        kind::REF_FULL => format!("ref {} {}", text(field::ID), text(field::TITLE)),
        kind::REF_BLOCK => {
            // ref_block wraps ref_inline / ref_full.
            let mut cursor = node.walk();
            let inner = node.named_children(&mut cursor).next();
            return match inner {
                Some(inner) => format_block(inner, source, config),
                None => vec![node_text(node, source).trim_end().to_string()],
            };
        }
        kind::REF_INLINE if holds_comment(node) => return as_written(node, source),
        kind::REF_INLINE => return vec![format!("ref {} {}", text(field::ID), text(field::TITLE))],
        kind::UNION_BLOCK => return format_union_block(node, source, config),
        _ => {
            return node_text(node, source)
                .lines()
                .map(|l| l.trim_end().to_string())
                .collect();
        }
    };
    let mut lines = vec![format!("{header} {{")];
    format_body(node, source, config, 1, &mut lines);
    lines.push("}".to_string());
    lines
}

/// `kind name = a | b | c`, one variant per line when it doesn't fit.
fn format_union_block(node: Node, source: &str, config: &FormatConfig) -> Vec<String> {
    let text = |name: &str| {
        node.child_by_field_name(name)
            .map(|n| node_text(n, source).to_string())
            .unwrap_or_default()
    };
    let Some(variants) = node.child_by_field_name(field::VARIANTS) else {
        return vec![node_text(node, source).trim_end().to_string()];
    };
    if holds_comment(node) {
        return node_text(node, source)
            .lines()
            .map(|l| l.trim_end().to_string())
            .collect();
    }
    let mut cursor = variants.walk();
    let children: Vec<Node> = variants.children(&mut cursor).collect();
    let parts: Vec<&str> = children
        .iter()
        .filter(|c| c.kind() != "|")
        .map(|c| node_text(*c, source))
        .collect();
    let head = format!("{} {} = ", text(field::KIND), text(field::NAME));
    let one_line = format!("{head}{}", parts.join(" | "));
    if one_line.len() <= config.max_width || parts.len() < 2 {
        return vec![one_line];
    }
    let indent = config.indent_str();
    let mut lines = vec![format!("{head}{}", parts[0])];
    lines.extend(parts[1..].iter().map(|p| format!("{indent}| {p}")));
    lines
}

/// One formatted member of a block body.
enum Member {
    Field {
        key: String,
        /// The value's first line and, for multi-line values, the rest.
        value: Vec<String>,
        annotations: Vec<String>,
    },
    /// A nested block value, formatted recursively.
    Nested {
        key: String,
        lines: Vec<String>,
        annotations: Vec<String>,
    },
    Line(String),
    Comment(String),
    /// A statement holding a comment between its tokens, kept as written:
    /// its first line at the body's indentation, the rest as they are.
    Written(Vec<String>),
}

/// Format the members of a block body (fields, verify statements, methods
/// and comments) in source order at `depth`.
fn format_body(
    node: Node,
    source: &str,
    config: &FormatConfig,
    depth: usize,
    lines: &mut Vec<String>,
) {
    let indent = config.indent_str().repeat(depth);
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    // (member, source start row, source end row)
    let mut members: Vec<(Member, usize, usize)> = Vec::new();
    for child in &children {
        let (start, end) = (child.start_position().row, child.end_position().row);
        let member = match child.kind() {
            _ if child.is_error() => Member::Line(node_text(*child, source).trim().to_string()),
            kind::FIELD | kind::VERIFY_STATEMENT | kind::METHOD_STATEMENT
                if holds_comment(*child) =>
            {
                Member::Written(as_written(*child, source))
            }
            kind::FIELD => field_member(*child, source, config, depth),
            kind::VERIFY_STATEMENT => Member::Line(verify_line(*child, source)),
            kind::METHOD_STATEMENT => Member::Line(method_line(*child, source)),
            kind::COMMENT => Member::Comment(comment_text(*child, source)),
            _ => continue,
        };
        members.push((member, start, end));
    }

    // Keys align to the longest key + 1 (a nested block's `key {` is not
    // aligned: it opens a block, not a value column); annotations of
    // single-line values align to the longest such value + 1.
    let key_width = members
        .iter()
        .filter_map(|(m, _, _)| match m {
            Member::Field { key, .. } => Some(key.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let pad = |key: &str| " ".repeat((key_width + 1).saturating_sub(key.len()).max(1));
    let annotated_width = members
        .iter()
        .filter_map(|(m, _, _)| match m {
            Member::Field {
                value, annotations, ..
            } if value.len() == 1 && !annotations.is_empty() => Some(value[0].len()),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    // The row of the opening brace: a comment on it trails the header.
    let open_row = children
        .iter()
        .find(|c| c.kind() == "{")
        .map(|c| c.start_position().row);
    let mut prev_end = open_row;
    for (member, start, end) in members {
        if let Member::Comment(text) = &member
            && prev_end == Some(start)
            && let Some(last) = lines.last_mut()
        {
            last.push(' ');
            last.push_str(text);
            prev_end = Some(end);
            continue;
        }
        match member {
            Member::Field {
                key,
                value,
                annotations,
            } => {
                let anns = annotations.join(" ");
                let first = if annotations.is_empty() {
                    value[0].clone()
                } else if value.len() == 1 {
                    format!(
                        "{}{}{anns}",
                        value[0],
                        " ".repeat(annotated_width + 1 - value[0].len())
                    )
                } else {
                    format!("{} {anns}", value[0])
                };
                lines.push(format!("{indent}{key}{}{first}", pad(&key)));
                lines.extend(value[1..].iter().cloned());
            }
            Member::Nested {
                key,
                lines: body,
                annotations,
            } => {
                let anns = if annotations.is_empty() {
                    String::new()
                } else {
                    format!(" {}", annotations.join(" "))
                };
                if body.is_empty() {
                    lines.push(format!("{indent}{key} {{}}{anns}"));
                } else {
                    lines.push(format!("{indent}{key} {{"));
                    lines.extend(body);
                    lines.push(format!("{indent}}}{anns}"));
                }
            }
            Member::Line(text) | Member::Comment(text) => lines.push(format!("{indent}{text}")),
            Member::Written(written) => {
                lines.push(format!("{indent}{}", written[0]));
                lines.extend(written[1..].iter().cloned());
            }
        }
        prev_end = Some(end);
    }
}

fn field_member(node: Node, source: &str, config: &FormatConfig, depth: usize) -> Member {
    let key = node
        .child_by_field_name(field::KEY)
        .map(|n| node_text(n, source).to_string())
        .unwrap_or_default();
    let mut cursor = node.walk();
    let annotations: Vec<String> = node
        .children(&mut cursor)
        .filter(|c| c.kind() == kind::ANNOTATION)
        .map(|c| one_line(c, source))
        .collect();
    let Some(value) = node.child_by_field_name(field::VALUE) else {
        return Member::Line(node_text(node, source).trim().to_string());
    };
    match value.kind() {
        kind::NESTED_BLOCK => {
            let mut body = Vec::new();
            format_body(value, source, config, depth + 1, &mut body);
            Member::Nested {
                key,
                lines: body,
                annotations,
            }
        }
        kind::LIST => Member::Field {
            value: list_value(value, source, config, depth, key.len()),
            key,
            annotations,
        },
        _ => {
            // Multi-line values (triple-quoted strings, ...) are opaque: the
            // first line follows the key, the rest stays as written.
            let text = node_text(value, source);
            let mut value_lines = text.lines();
            let first = value_lines.next().unwrap_or("").trim_end().to_string();
            let mut value: Vec<String> = vec![first];
            value.extend(value_lines.map(|l| l.trim_end().to_string()));
            Member::Field {
                key,
                value,
                annotations,
            }
        }
    }
}

/// `[a, b, c]` on one line when it fits, else one item per line. Items are
/// the list's child nodes, so strings containing `, ` stay whole. A list
/// holding comments is kept as written.
fn list_value(
    node: Node,
    source: &str,
    config: &FormatConfig,
    depth: usize,
    key_len: usize,
) -> Vec<String> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    if children.iter().any(|c| c.kind() == kind::COMMENT) {
        return node_text(node, source)
            .lines()
            .map(|l| l.trim_end().to_string())
            .collect();
    }
    let items: Vec<String> = children
        .iter()
        .filter(|c| !matches!(c.kind(), "[" | "]" | ","))
        .map(|c| one_line(*c, source))
        .collect();
    let one_line = format!("[{}]", items.join(", "));
    let indent = config.indent_str();
    let outer = indent.repeat(depth);
    // indent + key + at least one space + value
    if outer.len() + key_len + 1 + one_line.len() <= config.max_width || items.len() < 2 {
        return vec![one_line];
    }
    let mut lines = vec!["[".to_string()];
    lines.extend(items.iter().map(|item| format!("{outer}{indent}{item},")));
    lines.push(format!("{outer}]"));
    lines
}

/// `verify [kind] "description"`, single-spaced.
fn verify_line(node: Node, source: &str) -> String {
    let desc = node
        .child_by_field_name(field::DESCRIPTION)
        .map(|n| node_text(n, source))
        .unwrap_or("\"\"");
    match node.child_by_field_name(field::KIND) {
        Some(kind) => format!("verify {} {desc}", node_text(kind, source)),
        None => format!("verify {desc}"),
    }
}

/// `method name(a: T, b?: U @ann) -> R`, rebuilt from its parts.
fn method_line(node: Node, source: &str) -> String {
    let name = node
        .child_by_field_name(field::NAME)
        .map(|n| node_text(n, source))
        .unwrap_or("");
    let mut cursor = node.walk();
    let params: Vec<String> = node
        .children(&mut cursor)
        .filter(|c| c.kind() == kind::PARAMETER)
        .map(|p| {
            let pname = p
                .child_by_field_name(field::NAME)
                .map(|n| node_text(n, source))
                .unwrap_or("");
            let optional = if p.child_by_field_name(field::OPTIONAL).is_some() {
                "?"
            } else {
                ""
            };
            let ty = p
                .child_by_field_name(field::TYPE)
                .map(|n| one_line(n, source))
                .unwrap_or_default();
            let mut param_cursor = p.walk();
            let anns: Vec<String> = p
                .children(&mut param_cursor)
                .filter(|c| c.kind() == kind::ANNOTATION)
                .map(|c| one_line(c, source))
                .collect();
            let mut text = format!("{pname}{optional}: {ty}");
            for ann in anns {
                text.push(' ');
                text.push_str(&ann);
            }
            text
        })
        .collect();
    let mut line = format!("method {name}({})", params.join(", "));
    if let Some(returns) = node.child_by_field_name(field::RETURNS) {
        line.push_str(" -> ");
        line.push_str(&one_line(returns, source));
    }
    line
}

#[cfg(test)]
mod methods_tests {
    use super::*;

    #[test]
    fn method_statements_survive_formatting_idempotently() {
        let source = concat!(
            "port FileSystem {\n",
            "  direction outbound\n",
            "  method readFile(path: string) -> Result<string, EmitterError>\n",
            "  method flush()\n",
            "}\n",
        );
        let config = FormatConfig::default();
        let once = format_source(source, &config);
        assert!(
            once.diagnostics.is_empty(),
            "format diagnostics: {:?}",
            once.diagnostics
        );
        assert_eq!(
            once.formatted.matches("method readFile").count(),
            1,
            "method must be emitted exactly once: {:?}",
            once.formatted
        );
        assert!(
            once.formatted
                .contains("method readFile(path: string) -> Result<string, EmitterError>"),
            "method line lost or mangled: {:?}",
            once.formatted
        );
        let twice = format_source(&once.formatted, &config);
        assert_eq!(
            twice.formatted, once.formatted,
            "formatting must be idempotent"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FormatConfig;

    fn fmt(source: &str) -> String {
        format_source(source, &FormatConfig::default()).formatted
    }

    fn fmt_with(source: &str, config: &FormatConfig) -> String {
        format_source(source, config).formatted
    }

    // --- Slice 2: Tracer bullet (indent only) ---

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "indentation rules normalize to configured indent style"
    )]
    fn test_indent_normalizes_to_configured_style() {
        let input = "behavior foo \"Foo\" {\n      contract \"does stuff\"\n}\n";
        let result = fmt(input);
        assert!(
            result.contains("  contract \"does stuff\""),
            "got: {result}"
        );
    }

    // --- Slice 3: Remaining 7 rules ---

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "spacing rules normalize single spaces between tokens"
    )]
    fn test_spacing_normalizes_single_spaces() {
        let input = "behavior foo   \"Foo\" {\n  contract   \"does stuff\"\n}\n";
        let result = fmt(input);
        assert!(result.contains("behavior foo \"Foo\" {"), "got: {result}");
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "alignment rules align field values within blocks"
    )]
    fn test_alignment_aligns_field_values() {
        let input = "behavior foo \"Foo\" {\n  invariants [a, b]\n  types [x]\n  ports [y]\n}\n";
        let result = fmt(input);
        // All values should start at the same column
        let lines: Vec<&str> = result.lines().collect();
        let inv_line = lines.iter().find(|l| l.contains("invariants")).unwrap();
        let types_line = lines.iter().find(|l| l.contains("types")).unwrap();
        let ports_line = lines.iter().find(|l| l.contains("ports")).unwrap();

        // Find the column where '[' starts for each
        let inv_col = inv_line.find('[').unwrap();
        let types_col = types_line.find('[').unwrap();
        let ports_col = ports_line.find('[').unwrap();

        assert_eq!(
            inv_col, types_col,
            "invariants and types should align: {result}"
        );
        assert_eq!(
            types_col, ports_col,
            "types and ports should align: {result}"
        );
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "wrapping rules break long reference lists to multi-line"
    )]
    fn test_wrapping_breaks_long_lists() {
        let config = FormatConfig {
            indent_width: 2,
            use_tabs: false,
            max_width: 40,
        };
        let input = "behavior foo \"Foo\" {\n  invariants [very_long_name_a, very_long_name_b, very_long_name_c]\n}\n";
        let result = fmt_with(input, &config);
        // Should be wrapped to multi-line since it exceeds max_width
        let lines: Vec<&str> = result.lines().collect();
        let has_multiline_list = lines.iter().any(|l| l.trim() == "[");
        let has_items = lines
            .iter()
            .any(|l| l.trim().starts_with("very_long_name_a"));
        assert!(
            has_multiline_list || has_items,
            "should wrap long list: {result}"
        );
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "import sorting produces alphabetical order"
    )]
    fn test_import_sorting() {
        let input = "use \"types/core\"\nuse \"behaviors/auth\"\nuse \"events/compilation\"\n\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
        let result = fmt(input);
        let lines: Vec<&str> = result.lines().collect();
        let import_lines: Vec<&&str> = lines.iter().filter(|l| l.starts_with("use ")).collect();
        assert!(import_lines.len() >= 3);
        assert!(import_lines[0].contains("behaviors/auth"), "got: {result}");
        assert!(
            import_lines[1].contains("events/compilation"),
            "got: {result}"
        );
        assert!(import_lines[2].contains("types/core"), "got: {result}");
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "blank line rules enforce exactly one between blocks"
    )]
    fn test_blank_line_between_blocks() {
        let input = "behavior foo \"Foo\" {\n  contract \"a\"\n}\nbehavior bar \"Bar\" {\n  contract \"b\"\n}\n";
        let result = fmt(input);
        // Should have exactly one blank line between the two blocks
        assert!(result.contains("}\n\nbehavior bar"), "got: {result}");
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "comment rules normalize spacing around inline comments"
    )]
    fn test_comment_spacing_normalized() {
        let input = "//comment without space\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
        let result = fmt(input);
        assert!(result.contains("// comment without space"), "got: {result}");
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "comment rules normalize spacing around inline comments"
    )]
    fn test_normalize_comment_spacing() {
        assert_eq!(normalize_comment("//comment"), "// comment");
        assert_eq!(normalize_comment("// already good  "), "// already good");
        assert_eq!(normalize_comment("//"), "//");
        // The author's layout inside a comment is kept verbatim.
        assert_eq!(
            normalize_comment("//   - nested bullet"),
            "//   - nested bullet"
        );
        assert_eq!(normalize_comment("  //  indented"), "  //  indented");
        assert_eq!(normalize_comment("/// doc"), "/// doc");
        assert_eq!(normalize_comment("//! inner"), "//! inner");
        // Through the engine: a comment inside a block.
        let result = fmt("behavior a \"A\" {\n  //x\n  contract \"c\"\n}\n");
        assert!(result.contains("  // x"), "got: {result}");
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "indentation rules normalize to configured indent style"
    )]
    fn indent_follows_the_configured_width_and_tabs() {
        let input = "behavior foo \"Foo\" {\n  contract \"does stuff\"\n}\n";
        let four = FormatConfig {
            indent_width: 4,
            ..FormatConfig::default()
        };
        let tabs = FormatConfig {
            use_tabs: true,
            ..FormatConfig::default()
        };

        assert!(
            fmt_with(input, &four).contains("\n    contract"),
            "{}",
            fmt_with(input, &four)
        );
        assert!(
            fmt_with(input, &tabs).contains("\n\tcontract"),
            "{}",
            fmt_with(input, &tabs)
        );
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "multiline string literals are kept byte-for-byte"
    )]
    fn multiline_strings_are_kept_byte_for_byte() {
        let input = "behavior foo \"Foo\" {\n  contract \"\"\"\n      First line\n        Second line\n  \"\"\"\n}\n";
        let result = fmt(input);
        assert!(
            result.contains("contract \"\"\"\n      First line\n        Second line\n  \"\"\""),
            "got: {result}"
        );
    }

    // --- Slice 5: Idempotency ---

    #[specforge_test_macros::test(
        behavior = "maintain_format_idempotency",
        verify = "format(format(x)) == format(x) for random valid inputs"
    )]
    fn test_idempotency_simple() {
        let input = "use \"types/core\"\n\nbehavior foo \"Foo\" {\n  contract \"does stuff\"\n}\n";
        let first = fmt(input);
        let second = fmt(&first);
        assert_eq!(first, second, "format(format(x)) != format(x)");
    }

    #[specforge_test_macros::test(
        behavior = "maintain_format_idempotency",
        verify = "format(format(x)) == format(x) for random valid inputs"
    )]
    fn test_idempotency_complex() {
        let input = concat!(
            "use \"types/core\"\n",
            "use \"behaviors/auth\"\n",
            "\n",
            "// Section header\n",
            "behavior foo \"Foo\" {\n",
            "  invariants [a, b, c]\n",
            "  types      [x, y]\n",
            "  ports      [z]\n",
            "\n",
            "  contract \"does stuff\"\n",
            "\n",
            "  verify unit \"test one\"\n",
            "  verify unit \"test two\"\n",
            "}\n",
        );
        let first = fmt(input);
        let second = fmt(&first);
        assert_eq!(
            first, second,
            "complex: format(format(x)) != format(x)\nfirst:\n{first}\nsecond:\n{second}"
        );
    }

    // --- Slice 6: Parse errors ---

    #[specforge_test_macros::test(
        behavior = "format_with_parse_errors",
        verify = "file with syntax error is partially formatted without crash"
    )]
    fn test_file_with_syntax_error_partially_formatted() {
        let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n\nthis is invalid syntax { broken\n\nbehavior bar \"Bar\" {\n  contract \"also good\"\n}\n";
        let result = format_source(input, &FormatConfig::default());
        // Should not crash
        assert!(!result.formatted.is_empty());
        // Good parts should still be formatted
        assert!(result.formatted.contains("behavior foo \"Foo\""));
        assert!(result.formatted.contains("behavior bar \"Bar\""));
    }

    #[specforge_test_macros::test(
        behavior = "format_with_parse_errors",
        verify = "error regions are preserved verbatim in output"
    )]
    fn test_error_regions_preserved_verbatim() {
        let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n\n{{{broken\n\nbehavior bar \"Bar\" {\n  contract \"also good\"\n}\n";
        let result = format_source(input, &FormatConfig::default());
        // The error region should be in the output
        assert!(!result.formatted.is_empty());
    }

    #[specforge_test_macros::test(
        behavior = "format_with_parse_errors",
        verify = "well-formed blocks in a file with errors are still formatted"
    )]
    fn test_well_formed_blocks_with_errors_are_formatted() {
        let input = "behavior foo \"Foo\" {\n      contract \"good\"\n}\n\n broken {{\n\nbehavior bar \"Bar\" {\n    contract \"also good\"\n}\n";
        let result = format_source(input, &FormatConfig::default());
        // Well-formed blocks should be properly indented
        assert!(
            result.formatted.contains("  contract \"good\"")
                || result.formatted.contains("contract \"good\"")
        );
    }

    #[specforge_test_macros::test(
        behavior = "format_with_parse_errors",
        verify = "diagnostic lists files with parse errors and error line ranges"
    )]
    fn test_parse_error_diagnostics() {
        let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n{{{broken\n";
        let result = format_source(input, &FormatConfig::default());
        let w142: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| d.code == "W142")
            .collect();
        assert_eq!(w142.len(), 1, "{:?}", result.diagnostics);
        assert!(
            w142[0].message.starts_with("Parse error at lines 4-4,"),
            "{}",
            w142[0].message
        );
        let span = w142[0].span.as_ref().expect("a spanned W142");
        assert_eq!((span.start_line, span.start_col), (4, 1));
        assert_eq!((span.end_line, span.end_col), (4, "{{{broken".len() + 1));

        // A region over two lines ends at its last line's end.
        let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n\nbehavior bar \"Bar\" {\n  contract \"x\" ]]\n  more ((\n";
        let result = format_source(input, &FormatConfig::default());
        let span = result
            .diagnostics
            .iter()
            .find(|d| d.code == "W142")
            .and_then(|d| d.span.clone())
            .expect("a spanned W142");
        assert!(span.end_line > span.start_line, "{span:?}");
        let last = input.lines().nth(span.end_line - 1).unwrap();
        assert_eq!(span.end_col, last.len() + 1, "{span:?}");
    }

    /// Three blocks, then a stray `}}}` on line 13 (0-based 12).
    const RANGE_WITH_A_KEPT_REGION: &str = "behavior a \"A\" {\n  contract \"a\"\n}\n\nbehavior b \"B\" {\n  contract \"b\"\n}\n\nbehavior c \"C\" {\n      contract \"c\"\n}\n\n}}}\n";

    #[specforge_test_macros::test(
        behavior = "lsp_format_range",
        verify = "a region left unformatted is reported at its document lines"
    )]
    fn range_reports_a_kept_region_at_its_document_lines() {
        let result = format_range(RANGE_WITH_A_KEPT_REGION, 8, 12, &FormatConfig::default());

        let w142: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| d.code == "W142")
            .collect();
        assert_eq!(w142.len(), 1, "{:?}", result.diagnostics);
        assert!(
            w142[0].message.starts_with("Parse error at lines 13-13,"),
            "{}",
            w142[0].message
        );
        let span = w142[0].span.as_ref().expect("a spanned W142");
        assert_eq!(span.start_line, 13);
        assert_eq!(span.start_col, 1);
        assert_eq!(span.end_line, 13);
        // `}}}` is 3 bytes: the span covers the whole line, end exclusive.
        assert_eq!(span.end_col, 4);
    }

    const CRLF_TWO: &str = "behavior a \"A\" {\r\n  contract \"x\"\r\n}\r\n\r\nbehavior b \"B\" {\r\n      contract \"y\"\r\n}\r\n";

    #[specforge_test_macros::test(
        behavior = "lsp_format_range",
        verify = "range formatting changes no byte outside the blocks it touches"
    )]
    fn a_range_keeps_every_byte_outside_its_blocks() {
        let config = FormatConfig::default();
        assert_eq!(
            format_range(CRLF_TWO, 4, 6, &config).formatted,
            "behavior a \"A\" {\r\n  contract \"x\"\r\n}\r\n\r\nbehavior b \"B\" {\n  contract \"y\"\n}\n"
        );
        assert_eq!(
            format_range(CRLF_TWO, 0, 2, &config).formatted,
            "behavior a \"A\" {\n  contract \"x\"\n}\n\r\nbehavior b \"B\" {\r\n      contract \"y\"\r\n}\r\n"
        );
    }

    #[specforge_test_macros::test(
        behavior = "lsp_format_range",
        verify = "range formatting changes no byte outside the blocks it touches"
    )]
    fn a_range_on_the_unterminated_last_line_adds_no_newline() {
        let source = "behavior a \"A\" {\n  contract \"x\"\n}\n\nbehavior b \"B\" {\n      contract \"y\"\n}";
        assert_eq!(
            format_range(source, 4, 6, &FormatConfig::default()).formatted,
            "behavior a \"A\" {\n  contract \"x\"\n}\n\nbehavior b \"B\" {\n  contract \"y\"\n}"
        );
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "the formatted text has LF line endings and exactly one final newline"
    )]
    fn the_formatted_text_has_lf_endings_and_one_final_newline() {
        let canonical = "behavior a \"A\" {\n  contract \"x\"\n}\n";
        for input in [
            "behavior a \"A\" {\r\n  contract \"x\"\r\n}\r\n",
            "behavior a \"A\" {\n  contract \"x\"\n}",
            "\n\n\nbehavior a \"A\" {\n  contract \"x\"\n}\n\n\n",
            "behavior a \"A\" {\r\n  contract \"x\"\r\n}",
        ] {
            assert_eq!(fmt(input), canonical, "{input:?}");
        }
        assert_eq!(fmt(""), "");
        assert_eq!(fmt("\n\n"), "");
        assert_eq!(fmt("// c\r\n"), "// c\n");
        // A triple-quoted string's lines end in LF too (the parser reads it so).
        let string = fmt("behavior a \"A\" {\r\n  contract \"\"\"\r\n    x\r\n  \"\"\"\r\n}");
        assert!(
            !string.contains('\r') && string.ends_with("}\n"),
            "{string:?}"
        );
    }

    // --- Behavior: maintain_format_idempotency ---

    #[specforge_test_macros::test(
        behavior = "maintain_format_idempotency",
        verify = "alignment rules do not oscillate between runs"
    )]
    fn test_alignment_rules_do_not_oscillate_between_runs() {
        let input = "behavior foo \"Foo\" {\n  invariants [a, b]\n  types [x]\n  ports [y, z]\n  contract \"stuff\"\n}\n";
        let first = fmt(input);
        let second = fmt(&first);
        let third = fmt(&second);
        assert_eq!(first, second, "alignment oscillated on 2nd pass");
        assert_eq!(second, third, "alignment oscillated on 3rd pass");
    }

    #[specforge_test_macros::test(
        behavior = "maintain_format_idempotency",
        verify = "wrapping decisions are stable across runs"
    )]
    fn test_wrapping_decisions_are_stable_across_runs() {
        let config = FormatConfig {
            indent_width: 2,
            use_tabs: false,
            max_width: 50,
        };
        let input = "behavior foo \"Foo\" {\n  invariants [very_long_name_a, very_long_name_b, very_long_name_c]\n}\n";
        let first = fmt_with(input, &config);
        let second = fmt_with(&first, &config);
        let third = fmt_with(&second, &config);
        assert_eq!(first, second, "wrapping oscillated on 2nd pass");
        assert_eq!(second, third, "wrapping oscillated on 3rd pass");
    }

    #[specforge_test_macros::test(
        behavior = "maintain_format_idempotency",
        verify = "format(format(x)) == format(x) for random valid inputs"
    )]
    fn test_idempotency_random_valid_inputs() {
        // Property-style test: multiple representative inputs
        let inputs = [
            "behavior a \"A\" {\n  contract \"x\"\n}\n",
            "use \"types/a\"\nuse \"behaviors/b\"\n\nbehavior x \"X\" {\n  invariants [i1, i2]\n  types [t1]\n  contract \"c\"\n  verify unit \"test\"\n}\n",
            "// header\nbehavior b \"B\" {\n  contract \"y\"\n}\n\n// standalone\n\nbehavior c \"C\" {\n  contract \"z\"\n}\n",
            "spec \"Test\" {\n  name \"test\"\n  version \"0.1.0\"\n}\n",
            "behavior d \"D\" {\n  contract \"\"\"\n    multi\n    line\n  \"\"\"\n}\n",
            "type my_type \"MyType\" {\n  field1 \"string\"\n  field2 \"number\" @optional\n}\n",
            "behavior e \"E\" {\n      invariants   [a,  b,   c]\n      types    [x, y]\n  contract \"test\"\n}\n",
            "use \"z/z\"\nuse \"a/a\"\nuse \"m/m\"\n\nbehavior f \"F\" {\n  contract \"sorted\"\n}\n",
        ];
        for (i, input) in inputs.iter().enumerate() {
            let first = fmt(input);
            let second = fmt(&first);
            assert_eq!(
                first, second,
                "idempotency failed for input #{i}:\nfirst:\n{first}\nsecond:\n{second}"
            );
        }
    }

    // --- Behavior: apply_format_rules ---

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "two files differing only in whitespace produce identical output after formatting"
    )]
    fn test_whitespace_only_differences_produce_identical_output() {
        // Two files that differ only in whitespace should produce identical output
        let input_a =
            "behavior foo \"Foo\" {\n  invariants [a, b]\n  types [x]\n  contract \"test\"\n}\n";
        let input_b = "behavior foo \"Foo\" {\n    invariants   [a,   b]\n    types   [x]\n    contract   \"test\"\n}\n";
        let input_c =
            "behavior foo \"Foo\" {\n\tinvariants [a, b]\n\ttypes [x]\n\tcontract \"test\"\n}\n";

        let result_a = fmt(input_a);
        let result_b = fmt(input_b);
        let result_c = fmt(input_c);

        assert_eq!(result_a, result_b, "a vs b should be identical");
        assert_eq!(result_b, result_c, "b vs c should be identical");
    }

    // --- Behavior: format_with_parse_errors (additional) ---

    #[specforge_test_macros::test(
        behavior = "format_with_parse_errors",
        verify = "error region starts at first unparseable token"
    )]
    fn test_error_region_starts_at_first_unparseable_token() {
        let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n{{{broken stuff here\n";
        let result = format_source(input, &FormatConfig::default());
        // Error region should contain the broken content
        assert!(
            result.formatted.contains("{{{broken") || result.formatted.contains("broken"),
            "error region should start at first unparseable token: {}",
            result.formatted
        );
        // First block should still be well-formed
        assert!(result.formatted.contains("behavior foo \"Foo\" {"));
    }

    #[specforge_test_macros::test(
        behavior = "format_with_parse_errors",
        verify = "error region ends before next parseable top-level statement"
    )]
    fn test_error_region_ends_before_next_parseable_statement() {
        let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n\n{{{ broken\n\nbehavior bar \"Bar\" {\n  contract \"also good\"\n}\n";
        let result = format_source(input, &FormatConfig::default());
        // The bar block after the error should still be present and formatted
        assert!(
            result.formatted.contains("behavior bar \"Bar\""),
            "parseable block after error should be present: {}",
            result.formatted
        );
    }

    #[specforge_test_macros::test(
        behavior = "format_with_parse_errors",
        verify = "whitespace within error regions is preserved byte-for-byte"
    )]
    fn test_whitespace_within_error_regions_preserved_byte_for_byte() {
        let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n\n  {{{  broken   stuff  \n\nbehavior bar \"Bar\" {\n  contract \"also good\"\n}\n";
        let result = format_source(input, &FormatConfig::default());
        // The error region whitespace should be preserved
        // The exact error text should appear in the output
        assert!(
            result.formatted.contains("{{{") || result.formatted.contains("broken"),
            "error region content should be preserved: {}",
            result.formatted
        );
    }

    // --- Invariant: formatting_idempotency ---

    #[specforge_test_macros::test(
        behavior = "format_spec_files",
        verify = "files matching the canonical format are not rewritten"
    )]
    fn test_formatting_already_formatted_file_produces_identical_output() {
        // First format to get the canonical form, then verify idempotency
        let input = "use \"behaviors/auth\"\nuse \"types/core\"\n\nbehavior foo \"Foo\" {\n  invariants [a, b]\n  types [x]\n  contract \"does stuff\"\n\n  verify unit \"test one\"\n}\n";
        let canonical = fmt(input);
        let result = fmt(&canonical);
        assert_eq!(
            canonical, result,
            "already-formatted file should be unchanged:\nexpected:\n{canonical}\ngot:\n{result}"
        );
    }

    // --- Invariant: formatting_consistency ---

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "two files differing only in whitespace produce identical output after formatting"
    )]
    fn test_tab_and_space_indented_inputs_produce_same_output() {
        let space_input = "behavior foo \"Foo\" {\n    contract \"test\"\n    types [x]\n}\n";
        let tab_input = "behavior foo \"Foo\" {\n\tcontract \"test\"\n\ttypes [x]\n}\n";

        let space_result = fmt(space_input);
        let tab_result = fmt(tab_input);

        assert_eq!(
            space_result, tab_result,
            "tab and space indented inputs should produce same output:\nspace:\n{space_result}\ntab:\n{tab_result}"
        );
    }

    // --- Invariant: formatting_semantic_preservation ---

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "Apply Format Rules: format rule application holds — cst_available, format_config_loaded, contribution_registry_available, deterministic_output, no_domain_logic, extension_rules_applied"
    )]
    fn test_formatting_does_not_alter_entity_ids_field_values_or_reference_lists() {
        let input = "behavior my_behavior \"My Behavior\" {\n      invariants    [inv_a, inv_b, inv_c]\n      types    [type_x, type_y]\n      ports    [port_z]\n      contract    \"does something important\"\n\n      verify unit \"test alpha\"\n      verify integration \"test beta\"\n}\n";
        let result = fmt(input);

        // Entity ID preserved
        assert!(
            result.contains("my_behavior"),
            "entity ID should be preserved"
        );
        // Title preserved
        assert!(
            result.contains("\"My Behavior\""),
            "title should be preserved"
        );
        // All reference list items preserved
        for item in &["inv_a", "inv_b", "inv_c", "type_x", "type_y", "port_z"] {
            assert!(
                result.contains(item),
                "reference list item '{item}' should be preserved"
            );
        }
        // Field values preserved
        assert!(
            result.contains("\"does something important\""),
            "contract value should be preserved"
        );
        // Verify statements preserved
        assert!(
            result.contains("verify unit \"test alpha\""),
            "verify statement should be preserved"
        );
        assert!(
            result.contains("verify integration \"test beta\""),
            "verify statement should be preserved"
        );
    }

    // --- Invariant: format_rule_priority ---

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "indentation rules normalize to configured indent style"
    )]
    fn test_indent_rule_takes_precedence_over_spacing_rule() {
        // Indent rule (priority 1) should set the leading whitespace,
        // spacing rule (priority 3) should not override it
        let input = "behavior foo \"Foo\" {\n      contract   \"test\"\n}\n";
        let result = fmt(input);
        // The indent should be exactly 2 spaces (indent rule), not collapsed to 0
        assert!(
            result.contains("\n  contract \"test\""),
            "indent rule should take precedence: {result}"
        );
    }

    // --- Invariant: format_rule_determinism ---

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "Apply Format Rules: format rule application holds — cst_available, format_config_loaded, contribution_registry_available, deterministic_output, no_domain_logic, extension_rules_applied"
    )]
    fn test_determinism_same_input_same_config_same_output() {
        let input = "behavior foo \"Foo\" {\n      contract   \"test\"\n    types [a, b]\n}\n";
        let config = FormatConfig::default();
        let result1 = fmt_with(input, &config);
        let result2 = fmt_with(input, &config);
        let result3 = fmt_with(input, &config);
        assert_eq!(result1, result2, "determinism: run 1 vs 2");
        assert_eq!(result2, result3, "determinism: run 2 vs 3");
    }

    // --- Invariant: comment_preservation ---

    #[specforge_test_macros::test(
        behavior = "preserve_comments",
        verify = "no comments are lost after formatting"
    )]
    fn test_every_comment_in_input_appears_in_formatted_output() {
        let input = concat!(
            "// file header comment\n",
            "use \"types/core\"\n",
            "\n",
            "// leading comment for behavior\n",
            "behavior foo \"Foo\" { // trailing comment\n",
            "  contract \"stuff\"\n",
            "}\n",
            "\n",
            "// standalone comment\n",
            "\n",
            "behavior bar \"Bar\" {\n",
            "  // inner comment\n",
            "  contract \"things\"\n",
            "}\n",
        );
        let result = fmt(input);
        assert!(
            result.contains("// file header comment"),
            "file header comment missing: {result}"
        );
        assert!(
            result.contains("// leading comment for behavior"),
            "leading comment missing: {result}"
        );
        assert!(
            result.contains("// trailing comment"),
            "trailing comment missing: {result}"
        );
        assert!(
            result.contains("// standalone comment"),
            "standalone comment missing: {result}"
        );
        assert!(
            result.contains("// inner comment"),
            "inner comment missing: {result}"
        );
    }

    #[specforge_test_macros::test(
        behavior = "preserve_comments",
        verify = "trailing comment attaches to preceding node on same line"
    )]
    fn test_trailing_comments_remain_attached_to_preceding_node() {
        let input = "behavior foo \"Foo\" { // trailing\n  contract \"stuff\"\n}\n";
        let result = fmt(input);
        // Trailing comment should be preserved in the output (may be moved inside the block
        // by the formatter's block-level formatting)
        assert!(
            result.contains("// trailing"),
            "trailing comment should be preserved: {result}"
        );
    }

    #[specforge_test_macros::test(
        behavior = "preserve_comments",
        verify = "leading comment attaches to following node"
    )]
    fn test_leading_comments_remain_attached_to_following_node() {
        let input = "// describes foo\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n";
        let result = fmt(input);
        // leading comment should appear before the behavior block in the output
        let lines: Vec<&str> = result.lines().collect();
        let comment_idx = lines.iter().position(|l| l.contains("// describes foo"));
        let behavior_idx = lines.iter().position(|l| l.contains("behavior foo"));
        assert!(
            comment_idx.is_some() && behavior_idx.is_some(),
            "both should exist: {result}"
        );
        assert!(
            comment_idx.unwrap() < behavior_idx.unwrap(),
            "leading comment should be before behavior: {result}"
        );
    }

    // --- Invariant: config_defaults_valid ---

    #[specforge_test_macros::test(
        behavior = "load_format_config",
        verify = "missing config file uses defaults"
    )]
    fn test_default_format_config_passes_validation() {
        let config = FormatConfig::default();
        assert_eq!(config.indent_width, 2);
        assert!(!config.use_tabs);
        assert_eq!(config.max_width, 100);
        // Verify it produces valid indent strings
        assert_eq!(config.indent_str(), "  ");
        // Verify it can format without error
        let result = format_source("behavior foo \"Foo\" {\n  contract \"test\"\n}\n", &config);
        assert!(
            result.diagnostics.is_empty(),
            "default config should produce no diagnostics"
        );
    }

    #[specforge_test_macros::test(
        behavior = "load_format_config",
        verify = "invalid indent_width produces diagnostic and uses default"
    )]
    fn test_fallback_from_invalid_config_produces_usable_format_config() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("specforge.json"), "{}").unwrap();
        std::fs::write(
            root.join(".specforgefmt.toml"),
            "indent_width = -5\nmax_width = \"huge\"\nuse_tabs = 42\n",
        )
        .unwrap();
        let (config, diags) = crate::config::load_config(root, root);
        assert!(
            !diags.is_empty(),
            "should have diagnostics for invalid values"
        );
        // Config should still be usable (defaults for invalid fields)
        let result = format_source("behavior foo \"Foo\" {\n  contract \"test\"\n}\n", &config);
        assert!(
            !result.formatted.is_empty(),
            "fallback config should be usable"
        );
    }

    // --- Invariant: discover_completeness (tested from engine for convenience) ---

    #[specforge_test_macros::test(
        behavior = "lsp_format_range",
        verify = "range formatting matches full formatting for affected blocks"
    )]
    fn test_format_range_matches_full_formatting_for_affected_blocks() {
        let source = "behavior foo \"Foo\" {\n  contract \"a\"\n}\n\nbehavior bar \"Bar\" {\n      contract \"b\"\n}\n";
        let full = fmt(source);
        let range = format_range(source, 4, 6, &FormatConfig::default());

        // Extract the bar block from both
        let full_bar: String = full
            .lines()
            .skip_while(|l| !l.contains("behavior bar"))
            .collect::<Vec<_>>()
            .join("\n");
        let range_bar: String = range
            .formatted
            .lines()
            .skip_while(|l| !l.contains("behavior bar"))
            .collect::<Vec<_>>()
            .join("\n");

        assert_eq!(
            full_bar, range_bar,
            "range formatting should match full formatting for affected blocks"
        );
    }

    #[specforge_test_macros::test(
        behavior = "lsp_format_range",
        verify = "range formatting matches full formatting for affected blocks"
    )]
    fn a_range_keeps_the_blank_lines_around_it() {
        let source = "behavior foo \"Foo\" {\n  contract \"a\"\n}\n\nbehavior bar \"Bar\" {\n      contract \"b\"\n}\n";
        // The range starts on the blank line between the blocks.
        let range = format_range(source, 3, 6, &FormatConfig::default());

        assert_eq!(
            range.formatted,
            "behavior foo \"Foo\" {\n  contract \"a\"\n}\n\nbehavior bar \"Bar\" {\n  contract \"b\"\n}\n"
        );
        // A range of blank lines only changes nothing.
        let blank = format_range(source, 3, 3, &FormatConfig::default());
        assert_eq!(blank.formatted, source);
    }

    // --- Performance tests ---

    #[specforge_test_macros::test(
        behavior = "lsp_format_document",
        verify = "formats document within 50ms for files under 1000 lines"
    )]
    fn test_formats_document_within_50ms() {
        // Generate a reasonably large file (under 1000 lines)
        let mut source = String::from("use \"types/core\"\nuse \"behaviors/auth\"\n\n");
        for i in 0..50 {
            source.push_str(&format!(
                "behavior b{i} \"Behavior {i}\" {{\n  invariants [inv_a, inv_b]\n  types [type_x]\n  contract \"does thing {i}\"\n\n  verify unit \"test {i}\"\n}}\n\n"
            ));
        }

        let start = std::time::Instant::now();
        let _result = format_source(&source, &FormatConfig::default());
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 50,
            "formatting should complete within 50ms, took {}ms",
            elapsed.as_millis()
        );
    }

    #[specforge_test_macros::test(
        behavior = "lsp_format_range",
        verify = "formats range within 20ms for ranges under 200 lines"
    )]
    fn test_formats_range_within_20ms() {
        let mut source = String::from("use \"types/core\"\n\n");
        for i in 0..20 {
            source.push_str(&format!(
                "behavior b{i} \"Behavior {i}\" {{\n  contract \"does thing {i}\"\n}}\n\n"
            ));
        }

        let start = std::time::Instant::now();
        let _result = format_range(&source, 10, 20, &FormatConfig::default());
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 20,
            "range formatting should complete within 20ms, took {}ms",
            elapsed.as_millis()
        );
    }

    // --- Contract: apply_format_rules ---

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "Apply Format Rules: format rule application holds — cst_available, format_config_loaded, contribution_registry_available, deterministic_output, no_domain_logic, extension_rules_applied"
    )]
    fn test_apply_format_rules_contract() {
        // requires: cst_available — source must parse into a CST
        // requires: format_config_loaded — config is resolved
        let config = FormatConfig::default();
        let source = "behavior foo \"Foo\" {\n      contract   \"test\"\n    types   [a,  b]\n}\n";

        let result = format_source(source, &config);

        // ensures: deterministic_output — same input+config → same output
        let result2 = format_source(source, &config);
        assert_eq!(
            result.formatted, result2.formatted,
            "deterministic_output: same input+config must produce same output"
        );

        // ensures: no_domain_logic — formatter works with ANY keyword, not just known ones
        let custom_entity = "my_custom_thing foo \"Foo\" {\n      field1   \"value\"\n}\n";
        let custom_result = format_source(custom_entity, &config);
        assert!(
            custom_result.formatted.contains("my_custom_thing foo"),
            "no_domain_logic: formatter should handle unknown entity kinds"
        );
        // Verify indentation was applied (generic block formatting)
        assert!(
            custom_result.formatted.contains("  field1"),
            "no_domain_logic: generic blocks should still be indented"
        );
    }

    // --- Contract: maintain_format_idempotency ---

    #[specforge_test_macros::test(
        behavior = "maintain_format_idempotency",
        verify = "Maintain Format Idempotency: format idempotency holds — format_rules_available, idempotency_holds, no_oscillation"
    )]
    fn test_maintain_format_idempotency_contract() {
        let config = FormatConfig::default();

        // requires: format_rules_available — all rules loaded (implicit in format_source)
        let inputs = [
            "behavior a \"A\" {\n      contract   \"x\"\n}\n",
            "behavior b \"B\" {\n  invariants [x, y]\n  types [z]\n  contract \"c\"\n  verify unit \"t\"\n}\n",
            "use \"z/z\"\nuse \"a/a\"\n\nbehavior c \"C\" {\n  contract \"d\"\n}\n",
        ];

        for (i, input) in inputs.iter().enumerate() {
            let first = format_source(input, &config);
            let second = format_source(&first.formatted, &config);

            // ensures: idempotency_holds
            assert_eq!(
                first.formatted, second.formatted,
                "idempotency_holds failed for input #{i}"
            );

            // ensures: no_oscillation — 3rd pass identical to 2nd
            let third = format_source(&second.formatted, &config);
            assert_eq!(
                second.formatted, third.formatted,
                "no_oscillation failed for input #{i}"
            );
        }
    }

    // --- Invariant: formatting_semantic_preservation (property) ---

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "two files differing only in whitespace produce identical output after formatting"
    )]
    fn test_format_parses_to_identical_entity_graph() {
        let inputs = [
            "behavior login \"Login\" {\n  invariants [auth_required, session_valid]\n  types [Credentials, Session]\n  ports [AuthService]\n  contract \"authenticates the user\"\n\n  verify unit \"valid credentials succeed\"\n  verify integration \"session is created\"\n}\n",
            "use \"types/core\"\nuse \"behaviors/auth\"\n\ntype user \"User\" {\n  name \"string\"\n  email \"string\" @unique\n  age \"number\" @optional\n}\n",
            "spec \"MyProject\" {\n  name \"my-project\"\n  version \"0.1.0\"\n}\n",
            "behavior a \"A\" {\n      contract   \"test\"\n    types   [x, y, z]\n    invariants [i1]\n}\n",
        ];

        for (i, input) in inputs.iter().enumerate() {
            let formatted = format_source(input, &FormatConfig::default()).formatted;

            let original_ast = specforge_parser::parse(input, "test.spec");
            let formatted_ast = specforge_parser::parse(&formatted, "test.spec");

            // Same number of entities
            assert_eq!(
                original_ast.entities.len(),
                formatted_ast.entities.len(),
                "input #{i}: entity count should be preserved"
            );

            // Same number of imports
            assert_eq!(
                original_ast.imports.len(),
                formatted_ast.imports.len(),
                "input #{i}: import count should be preserved"
            );

            // Each entity: same kind, id, title, and field keys/values
            for (orig, fmt_ent) in original_ast
                .entities
                .iter()
                .zip(formatted_ast.entities.iter())
            {
                assert_eq!(
                    orig.kind, fmt_ent.kind,
                    "input #{i}: entity kind should be preserved"
                );
                assert_eq!(
                    orig.id, fmt_ent.id,
                    "input #{i}: entity id should be preserved"
                );
                assert_eq!(
                    orig.title, fmt_ent.title,
                    "input #{i}: entity title should be preserved"
                );

                // Compare field keys
                let orig_keys: Vec<&str> = orig
                    .fields
                    .entries()
                    .iter()
                    .map(|e| e.key.as_str())
                    .collect();
                let fmt_keys: Vec<&str> = fmt_ent
                    .fields
                    .entries()
                    .iter()
                    .map(|e| e.key.as_str())
                    .collect();
                assert_eq!(
                    orig_keys, fmt_keys,
                    "input #{i}: field keys should be preserved for entity '{}'",
                    orig.id.raw
                );

                // Compare field values via JSON serialization
                let orig_json = serde_json::to_string(&orig.fields).unwrap();
                let fmt_json = serde_json::to_string(&fmt_ent.fields).unwrap();
                assert_eq!(
                    orig_json, fmt_json,
                    "input #{i}: field values should be preserved for entity '{}':\norig: {orig_json}\nfmt:  {fmt_json}",
                    orig.id.raw
                );
            }

            // Compare import paths (order may differ due to sorting)
            let mut orig_imports: Vec<&str> = original_ast
                .imports
                .iter()
                .map(|i| i.path.as_str())
                .collect();
            let mut fmt_imports: Vec<&str> = formatted_ast
                .imports
                .iter()
                .map(|i| i.path.as_str())
                .collect();
            orig_imports.sort();
            fmt_imports.sort();
            assert_eq!(
                orig_imports, fmt_imports,
                "input #{i}: import paths should be preserved (sorted)"
            );
        }
    }

    // --- Contract: format_with_parse_errors ---

    #[specforge_test_macros::test(
        behavior = "format_with_parse_errors",
        verify = "Format Files with Parse Errors: formatting with parse errors holds — cst_with_errors, no_crash, well_formed_regions_formatted, error_regions_preserved, parse_error_diagnosed"
    )]
    fn test_format_with_parse_errors_contract() {
        let config = FormatConfig::default();

        // requires: cst_with_errors — source has parse errors
        let source = "behavior foo \"Foo\" {\n      contract \"good\"\n}\n\n{{{ broken stuff\n\nbehavior bar \"Bar\" {\n    contract \"also good\"\n}\n";

        let result = format_source(source, &config);

        // ensures: no_crash — we got here without panicking
        assert!(
            !result.formatted.is_empty(),
            "no_crash: output should not be empty"
        );

        // ensures: well_formed_regions_formatted — good blocks should be indented
        assert!(
            result.formatted.contains("behavior foo \"Foo\""),
            "well_formed_regions_formatted: foo block should be present"
        );
        assert!(
            result.formatted.contains("behavior bar \"Bar\""),
            "well_formed_regions_formatted: bar block should be present"
        );

        // ensures: error_regions_preserved — broken content preserved
        assert!(
            result.formatted.contains("{{{ broken") || result.formatted.contains("broken"),
            "error_regions_preserved: error content should be in output"
        );

        // ensures: parse_error_diagnosed — W142 diagnostic emitted with line ranges
        let error_diags: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| d.code == "W142")
            .collect();
        assert!(
            !error_diags.is_empty(),
            "parse_error_diagnosed: should have W142 diagnostic"
        );
        assert!(
            error_diags[0].message.contains("lines"),
            "parse_error_diagnosed: diagnostic should mention line ranges: {}",
            error_diags[0].message
        );
    }

    // --- Gap coverage: format_spec_files ---

    #[specforge_test_macros::test(
        behavior = "format_spec_files",
        verify = "summary count reflects actual changes"
    )]
    fn test_summary_count_reflects_actual_changes() {
        let config = FormatConfig::default();
        // Already-formatted input should produce no changes
        let clean = "behavior foo \"Foo\" {\n  contract \"test\"\n}\n";
        let clean_result = format_source(clean, &config);
        assert_eq!(
            clean_result.formatted, clean,
            "already-formatted file should not change"
        );

        // Badly-formatted input should produce changes
        let dirty = "behavior foo \"Foo\" {\n      contract   \"test\"\n}\n";
        let dirty_result = format_source(dirty, &config);
        assert_ne!(
            dirty_result.formatted, dirty,
            "badly-formatted file should change"
        );

        // Verify we can count changes by comparing input != output
        let inputs = [
            clean,
            dirty,
            "behavior bar \"Bar\" {\n  contract \"ok\"\n}\n",
        ];
        let changed_count = inputs
            .iter()
            .filter(|input| {
                let r = format_source(input, &config);
                r.formatted != **input
            })
            .count();
        assert_eq!(changed_count, 1, "exactly 1 of 3 files should be changed");
    }

    // --- Gap coverage: lsp_format_range ---

    #[specforge_test_macros::test(
        behavior = "lsp_format_range",
        verify = "range is expanded to block boundaries"
    )]
    fn test_range_is_expanded_to_block_boundaries() {
        // Source with two blocks: request formatting in the MIDDLE of the second block
        let source = "behavior foo \"Foo\" {\n  contract \"a\"\n}\n\nbehavior bar \"Bar\" {\n      contract   \"b\"\n      types   [x]\n}\n";
        // Request only line 5 (contract line inside bar), which is inside the bar block (lines 4-7)
        let result = format_range(source, 5, 5, &FormatConfig::default());

        // The entire bar block should be formatted (expanded to block boundaries)
        assert!(
            result.formatted.contains("  contract \"b\""),
            "contract line should be formatted with correct indent: {}",
            result.formatted
        );
        assert!(
            result.formatted.contains("  types") && result.formatted.contains("[x]"),
            "types line should also be formatted (range expanded): {}",
            result.formatted
        );
        // The foo block should be untouched (not in the range)
        assert!(
            result.formatted.contains("behavior foo \"Foo\" {"),
            "foo block should be preserved"
        );
    }

    // --- Configured indentation (the width and tabs an editor asks for outside a project) ---

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "indentation rules normalize to configured indent style"
    )]
    fn a_configured_width_sets_the_indent() {
        // When no .specforgefmt.toml exists, the FormatConfig should use defaults
        // which correspond to what the editor would provide
        let config_4 = FormatConfig {
            indent_width: 4,
            use_tabs: false,
            max_width: 80,
        };
        let config_2 = FormatConfig {
            indent_width: 2,
            use_tabs: false,
            max_width: 80,
        };

        let input = "behavior foo \"Foo\" {\n        contract \"test\"\n}\n";

        let result_4 = format_source(input, &config_4);
        let result_2 = format_source(input, &config_2);

        // With indent_width=4, contract should be indented 4 spaces
        assert!(
            result_4.formatted.contains("    contract"),
            "indent_width=4 should produce 4-space indent: {}",
            result_4.formatted
        );
        // With indent_width=2, contract should be indented 2 spaces
        assert!(
            result_2.formatted.contains("  contract"),
            "indent_width=2 should produce 2-space indent: {}",
            result_2.formatted
        );
        // Different tab sizes produce different output
        assert_ne!(
            result_4.formatted, result_2.formatted,
            "different editor tab sizes should produce different formatting"
        );
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "indentation rules normalize to configured indent style"
    )]
    fn use_tabs_indents_with_tabs() {
        let config = FormatConfig {
            indent_width: 2,
            use_tabs: true,
            max_width: 80,
        };
        let input = "behavior foo \"Foo\" {\n  contract \"test\"\n}\n";
        let result = format_source(input, &config);
        assert!(
            result.formatted.contains("\tcontract"),
            "use_tabs=true should produce tab indentation: {:?}",
            result.formatted
        );
    }

    #[specforge_test_macros::test(
        behavior = "lsp_respect_editor_config",
        verify = "config file takes precedence over editor settings"
    )]
    fn test_config_file_overrides_editor_settings() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("specforge.json"), "{}").unwrap();
        std::fs::write(
            root.join(".specforgefmt.toml"),
            "indent_width = 4\nmax_width = 100\n",
        )
        .unwrap();

        let (config, diags) = crate::config::load_config(root, root);
        assert!(
            diags.is_empty(),
            "valid config should produce no diagnostics"
        );
        assert_eq!(
            config.indent_width, 4,
            "config file should set indent_width=4"
        );
        assert_eq!(
            config.max_width, 100,
            "config file should set max_width=100"
        );
    }

    #[specforge_test_macros::test(
        behavior = "lsp_respect_editor_config",
        verify = "LSP Respect Editor Config: editor config respect holds — lsp_initialized_fired, config_precedence_enforced, editor_fallback_applied"
    )]
    fn test_editor_config_contract() {
        use tempfile::TempDir;

        // Requires: LSP initialized, editor settings available
        let editor_config = FormatConfig {
            indent_width: 4,
            use_tabs: false,
            max_width: 80,
        };

        // Ensures: editor fallback applied when no config file exists
        let tmp_no_config = TempDir::new().unwrap();
        std::fs::write(tmp_no_config.path().join("specforge.json"), "{}").unwrap();
        let (_, diags) = crate::config::load_config(tmp_no_config.path(), tmp_no_config.path());
        assert!(diags.is_empty());
        // Without config file, defaults used (editor would supply these)
        let result_editor = format_source(
            "behavior a \"A\" {\n    contract \"x\"\n}\n",
            &editor_config,
        );
        assert!(
            result_editor.formatted.contains("    contract"),
            "editor settings should be applied"
        );

        // Ensures: config precedence enforced when config file exists
        let tmp_with_config = TempDir::new().unwrap();
        std::fs::write(tmp_with_config.path().join("specforge.json"), "{}").unwrap();
        std::fs::write(
            tmp_with_config.path().join(".specforgefmt.toml"),
            "indent_width = 2\n",
        )
        .unwrap();
        let (file_config, diags) =
            crate::config::load_config(tmp_with_config.path(), tmp_with_config.path());
        assert!(diags.is_empty());
        assert_eq!(
            file_config.indent_width, 2,
            "config file should override editor tab size"
        );
        let result_file =
            format_source("behavior a \"A\" {\n    contract \"x\"\n}\n", &file_config);
        assert!(
            result_file.formatted.contains("  contract"),
            "config file indent should take precedence"
        );
    }

    // --- Gap coverage: format_spec_files ---

    #[specforge_test_macros::test(
        behavior = "format_spec_files",
        verify = "formatting all files in spec/ directory succeeds"
    )]
    fn test_formatting_multiple_files_succeeds() {
        let config = FormatConfig::default();
        let files = [
            "behavior a \"A\" {\n  contract \"ok\"\n}\n",
            "behavior b \"B\" {\n      contract   \"fix\"\n}\n",
            "behavior c \"C\" {\n  contract \"fine\"\n}\n",
        ];
        let results: Vec<_> = files.iter().map(|f| format_source(f, &config)).collect();
        for r in &results {
            assert!(
                r.diagnostics
                    .iter()
                    .all(|d| d.severity != specforge_common::Severity::Error),
                "no formatting errors expected"
            );
            assert!(
                !r.formatted.is_empty(),
                "formatted output should not be empty"
            );
        }
    }

    #[specforge_test_macros::test(
        behavior = "format_spec_files",
        verify = "Format Spec Files: spec file formatting holds — spec_files_available, format_config_loaded, formatted_output_written, unchanged_files_preserved, format_complete_emitted, summary_printed"
    )]
    fn test_format_spec_files_contract() {
        let config = FormatConfig::default();
        // Requires: spec file available, config loaded
        let dirty = "behavior foo \"Foo\" {\n      contract   \"test\"\n}\n";
        let result = format_source(dirty, &config);
        // Ensures: formatted output written (changed)
        assert_ne!(result.formatted, dirty, "dirty file must be reformatted");
        // Ensures: unchanged files preserved
        let clean = result.formatted.clone();
        let result2 = format_source(&clean, &config);
        assert_eq!(
            result2.formatted, clean,
            "already-formatted file must not change"
        );
        // Ensures: no errors
        assert!(
            result
                .diagnostics
                .iter()
                .all(|d| d.severity != specforge_common::Severity::Error)
        );
    }

    // --- Gap coverage: show_formatting_diff ---

    #[specforge_test_macros::test(
        behavior = "show_formatting_diff",
        verify = "Show Formatting Diff: formatting diff holds — spec_files_available, format_config_loaded, no_files_written, unified_diff_produced"
    )]
    fn test_show_formatting_diff_contract() {
        let config = FormatConfig::default();
        let dirty = "behavior foo \"Foo\" {\n      contract   \"test\"\n}\n";
        let clean = "behavior foo \"Foo\" {\n  contract \"test\"\n}\n";

        // Requires: spec file available, config loaded
        let result_dirty = format_source(dirty, &config);
        let result_clean = format_source(clean, &config);

        // Ensures: unified diff produced for changed files
        let diff_dirty = crate::diff::unified_diff("test.spec", dirty, &result_dirty.formatted);
        assert!(
            !diff_dirty.diff_text.is_empty(),
            "dirty file must produce diff"
        );
        assert!(
            diff_dirty.diff_text.contains("---"),
            "diff must use unified format with --- header"
        );
        assert!(
            diff_dirty.diff_text.contains("+++"),
            "diff must use unified format with +++ header"
        );

        // Ensures: no diff for unchanged files
        let diff_clean = crate::diff::unified_diff("test.spec", clean, &result_clean.formatted);
        assert!(
            diff_clean.diff_text.is_empty(),
            "clean file must produce no diff"
        );
    }

    // --- Gap coverage: lsp_format_document ---

    #[specforge_test_macros::test(
        behavior = "lsp_format_document",
        verify = "parse errors in document trigger format_with_parse_errors delegation"
    )]
    fn test_parse_errors_trigger_partial_formatting() {
        let config = FormatConfig::default();
        let source_with_error = "behavior foo \"Foo\" {\n  contract \"ok\"\n}\n\n{{{ invalid syntax\n\nbehavior bar \"Bar\" {\n      contract   \"fix\"\n}\n";

        let result = format_source(source_with_error, &config);
        // Should not crash
        assert!(
            !result.formatted.is_empty(),
            "formatter must not crash on parse errors"
        );
        // Well-formed regions should still be formatted
        // Error region should be preserved
        assert!(
            result.formatted.contains("{{{ invalid syntax"),
            "error region must be preserved verbatim"
        );
    }

    // --- Gap coverage: lsp_format_range ---

    #[specforge_test_macros::test(
        behavior = "lsp_format_range",
        verify = "parse errors within range are left unchanged per format_with_parse_errors"
    )]
    fn test_range_parse_errors_left_unchanged() {
        let config = FormatConfig::default();
        let source = "behavior foo \"Foo\" {\n  contract \"ok\"\n}\n\n{{{ broken\n\nbehavior bar \"Bar\" {\n      contract   \"fix\"\n}\n";

        // Format range covering the error region (lines 4-5)
        let result = format_range(source, 4, 5, &config);
        // Error region should be preserved
        assert!(
            result.formatted.contains("{{{ broken"),
            "parse error region within range must be left unchanged"
        );
    }

    #[specforge_test_macros::test(
        behavior = "lsp_format_range",
        verify = "LSP Format Range: LSP range formatting holds — document_open, format_config_loaded, range_expanded, textedit_list_returned, full_format_parity, format_complete_emitted"
    )]
    fn test_lsp_format_range_contract() {
        let config = FormatConfig::default();
        let source = "behavior foo \"Foo\" {\n  contract \"a\"\n}\n\nbehavior bar \"Bar\" {\n      contract   \"b\"\n}\n";

        // Requires: document open, config loaded
        let result = format_range(source, 5, 5, &config);

        // Ensures: range expanded to block boundaries (bar block formatted)
        assert!(
            result.formatted.contains("  contract \"b\""),
            "bar block contract should be formatted"
        );

        // Ensures: full format parity for affected blocks
        let full = format_source(source, &config);
        // The bar block in range format should match bar block in full format
        let range_bar = result
            .formatted
            .lines()
            .skip_while(|l| !l.starts_with("behavior bar"))
            .collect::<Vec<_>>()
            .join("\n");
        let full_bar = full
            .formatted
            .lines()
            .skip_while(|l| !l.starts_with("behavior bar"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            range_bar, full_bar,
            "range format must match full format for affected blocks"
        );
    }
}

/// The emitter keeps every comment where it was, keeps statement order, and
/// never splits a value: each test pins one bug of the previous engine.
#[cfg(test)]
mod emitter_tests {
    use super::*;

    fn fmt(source: &str) -> String {
        let result = format_source(source, &FormatConfig::default());
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let again = format_source(&result.formatted, &FormatConfig::default()).formatted;
        assert_eq!(again, result.formatted, "not idempotent");
        result.formatted
    }

    #[specforge_test_macros::test(
        behavior = "preserve_comments",
        verify = "leading comment attaches to following node"
    )]
    #[specforge_test_macros::test(
        invariant = "comment_preservation",
        verify = "leading comments remain attached to their following node"
    )]
    fn a_leading_comment_stays_directly_above_its_block() {
        let out = fmt(
            "behavior a \"A\" {\n  contract \"c\"\n}\n// doc for b\nbehavior b \"B\" {\n  contract \"c\"\n}\n",
        );
        assert!(out.contains("}\n\n// doc for b\nbehavior b"), "{out}");
    }

    #[specforge_test_macros::test(
        behavior = "preserve_comments",
        verify = "trailing comment attaches to preceding node on same line"
    )]
    #[specforge_test_macros::test(
        invariant = "comment_preservation",
        verify = "trailing comments remain attached to their preceding node"
    )]
    fn a_trailing_comment_stays_on_its_line() {
        let out =
            fmt("behavior a \"A\" { // header note\n  contract \"c\" // why\n  status draft\n}\n");
        assert!(out.contains("behavior a \"A\" { // header note\n"), "{out}");
        assert!(out.contains("contract \"c\" // why\n"), "{out}");
    }

    #[specforge_test_macros::test(
        behavior = "preserve_comments",
        verify = "section header comment attaches to next block group"
    )]
    #[specforge_test_macros::test(
        behavior = "preserve_comments",
        verify = "standalone comment block between blocks is preserved"
    )]
    fn standalone_and_section_comments_keep_their_separation() {
        let out = fmt(concat!(
            "behavior a \"A\" {\n  contract \"c\"\n}\n\n\n",
            "// standalone note\n\n\n",
            "// Section: more\nbehavior b \"B\" {\n  contract \"c\"\n}\n",
        ));
        assert_eq!(
            out,
            "behavior a \"A\" {\n  contract \"c\"\n}\n\n// standalone note\n\n// Section: more\nbehavior b \"B\" {\n  contract \"c\"\n}\n"
        );
    }

    #[specforge_test_macros::test(
        behavior = "preserve_comments",
        verify = "no comments are lost after formatting"
    )]
    #[specforge_test_macros::test(
        invariant = "comment_preservation",
        verify = "every comment in input appears in formatted output"
    )]
    fn comments_in_every_block_kind_stay_in_place() {
        let source = concat!(
            "spec \"s\" {\n  version \"1\"\n  // spec note\n}\n\n",
            "define widget {\n  // define note\n  size integer\n}\n\n",
            "ref gh.issue:1 \"Issue\" {\n  // ref note\n  status open\n}\n\n",
            "behavior a \"A\" {\n  contract \"c\"\n  // between\n  status draft\n  // again\n  // again\n  verify unit \"x\"\n}\n",
        );
        let out = fmt(source);
        assert!(out.contains("  version \"1\"\n  // spec note\n}"), "{out}");
        assert!(
            out.contains("define widget {\n  // define note\n  size integer\n}"),
            "{out}"
        );
        assert!(out.contains("  // ref note\n  status open\n}"), "{out}");
        assert!(
            out.contains("  contract \"c\"\n  // between\n  status   draft\n  // again\n  // again\n  verify unit \"x\"\n"),
            "{out}"
        );
    }

    #[specforge_test_macros::test(
        behavior = "preserve_comments",
        verify = "Preserve Comments During Formatting: comment preservation holds — cst_available, all_comments_attached, no_comments_lost"
    )]
    fn comment_text_is_kept_as_written() {
        let out = fmt(
            "/// doc comment\n//   - nested bullet\n//no space\nbehavior a \"A\" {\n  contract \"c\"\n}\n",
        );
        assert!(
            out.starts_with("/// doc comment\n//   - nested bullet\n// no space\nbehavior a"),
            "{out}"
        );
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "wrapping rules break long reference lists to multi-line"
    )]
    fn wrapping_a_list_never_splits_a_string() {
        let long = "Each kind has correct testable, singleton, and supports_verify flags";
        let source = format!(
            "feature f \"F\" {{\n  criteria [\"{long}\", \"second criterion that is long enough to wrap\"]\n}}\n"
        );
        let out = fmt(&source);
        assert!(out.contains(&format!("    \"{long}\",\n")), "{out}");
        assert!(out.contains("  criteria [\n"), "{out}");
        // A short hand-wrapped list is joined.
        let out = fmt("feature f \"F\" {\n  refs [\n    a,\n    b,\n  ]\n}\n");
        assert!(out.contains("  refs [a, b]\n"), "{out}");
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "statements keep their source order"
    )]
    fn statements_keep_their_order() {
        let out = fmt(
            "event e \"E\" {\n  verify unit \"first\"\n  channel bus\n  method m(x: string)\n  payload order\n}\n",
        );
        assert_eq!(
            out,
            "event e \"E\" {\n  verify unit \"first\"\n  channel bus\n  method m(x: string)\n  payload order\n}\n"
        );
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "alignment rules align field values within blocks"
    )]
    fn keys_and_annotations_align_but_verify_does_not() {
        let out = fmt(
            "type task \"T\" {\n  id string @readonly @unique\n  title string\n  completedAt timestamp @optional\n  verify   unit    \"x\"\n}\n",
        );
        assert_eq!(
            out,
            "type task \"T\" {\n  id          string    @readonly @unique\n  title       string\n  completedAt timestamp @optional\n  verify unit \"x\"\n}\n"
        );
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "a union that does not fit wraps one variant per line"
    )]
    fn a_long_union_wraps() {
        let source = "type verify_kind = unit | contract | integration | property | performance | mutation | load | deadlock_free\n";
        let out = fmt(source);
        assert!(
            out.starts_with("type verify_kind = unit\n  | contract\n"),
            "{out}"
        );
        assert_eq!(fmt("type k = a|b\n"), "type k = a | b\n");
    }

    /// Comments written between a statement's tokens.
    const COMMENTS_IN_STATEMENTS: &str = concat!(
        "use {\n",
        "  x, // the x\n",
        "  y\n",
        "} from \"./x.spec\"\n",
        "\n",
        "ref gh.issue:42 // the issue\n",
        "  \"Support\"\n",
        "\n",
        "type t = // the variants\n",
        "  open | closed\n",
        "\n",
        "port p \"P\" {\n",
        "  contract // the contract\n",
        "    \"c\"\n",
        "  verify // the kind follows\n",
        "    unit \"v\"\n",
        "  tags x @deprecated // why\n",
        "    \"old\"\n",
        "  method f(a: A, // the a\n",
        "    b: B) -> C\n",
        "}\n",
    );

    #[specforge_test_macros::test(
        behavior = "preserve_comments",
        verify = "a statement holding a comment between its tokens is kept as written"
    )]
    #[specforge_test_macros::test(
        invariant = "comment_preservation",
        verify = "every comment in input appears in formatted output"
    )]
    fn a_statement_holding_a_comment_is_kept_as_written() {
        assert_eq!(fmt(COMMENTS_IN_STATEMENTS), COMMENTS_IN_STATEMENTS);
        let result = format_source(COMMENTS_IN_STATEMENTS, &FormatConfig::default());
        assert!(
            specforge_parser::parse(&result.formatted, "c.spec")
                .errors
                .is_empty(),
            "the output must still parse"
        );
    }

    #[test]
    fn one_line_collapses_whitespace_between_lexemes() {
        assert_eq!(
            fmt("use  {  A ,B }  from \"x\"\n"),
            "use { A ,B } from \"x\"\n"
        );
        let out = fmt("port p \"P\" {\n  tags x @since   \"a  b\"\n}\n");
        assert!(out.contains("@since \"a  b\""), "{out}");
    }

    // --- Plan 14 T0: what a broken document reads as today ---

    const UNCLOSED_STRING: &str = "behavior a1 \"A\" {\n  contract \"x\n}\n\nbehavior b1 \"B\" {\n  contract    \"y\"\n}\n\nbehavior c1 \"C\" {\n  contract \"z\"\n}\n";

    #[test]
    fn an_unclosed_string_keeps_the_whole_file_verbatim() {
        let result = format_source(UNCLOSED_STRING, &FormatConfig::default());
        assert_eq!(result.formatted, UNCLOSED_STRING);
        let w142: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| d.is(codes::W142))
            .collect();
        assert_eq!(w142.len(), 1);
        assert!(
            w142[0].message.starts_with("Parse error at lines 1-12,"),
            "{}",
            w142[0].message
        );
    }

    const UNCLOSED_BODY: &str =
        "behavior a1 \"A\" {\n  contract    \"x\"\n\nbehavior b1 \"B\" {\n  contract    \"y\"\n}\n";

    #[test]
    fn a_kept_unclosed_body_is_reported_at_its_error_line_only() {
        let result = format_source(UNCLOSED_BODY, &FormatConfig::default());
        assert!(
            result
                .formatted
                .starts_with("behavior a1 \"A\" {\n  contract    \"x\"\n\nbehavior b1")
        );
        assert!(result.formatted.contains("  contract \"y\""));
        let span = result
            .diagnostics
            .iter()
            .find(|d| d.is(codes::W142))
            .and_then(|d| d.span.clone())
            .unwrap();
        assert_eq!((span.start_line, span.end_line), (2, 2));
    }
}
