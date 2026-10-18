use super::{as_written, comment_text, holds_comment, lines_keeping_strings, node_text, one_line};
use crate::config::FormatConfig;
use specforge_graph::FieldForms;
use specforge_parser::Written;
use tree_sitter::Node;
use tree_sitter_specforge::{field, kind};

/// The declared kind a block's fields are looked up under, with the forms
/// that say how each field's value is written (ADR 0147): only the depth-1
/// body of an entity block has one.
type Forms<'a> = Option<(&'a str, &'a FieldForms)>;

/// Format one top-level block.
pub(super) fn format_block(
    node: Node,
    source: &str,
    config: &FormatConfig,
    forms: &FieldForms,
) -> Vec<String> {
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
                Some(inner) => format_block(inner, source, config, forms),
                None => vec![node_text(node, source).trim_end().to_string()],
            };
        }
        kind::REF_INLINE if holds_comment(node) => return as_written(node, source),
        kind::REF_INLINE => return vec![format!("ref {} {}", text(field::ID), text(field::TITLE))],
        kind::UNION_BLOCK => return format_union_block(node, source, config),
        _ => return lines_keeping_strings(node, source),
    };
    let mut lines = vec![format!("{header} {{")];
    let kind_text = text(field::KIND);
    let declared = (node.kind() == kind::ENTITY_BLOCK).then_some((kind_text.as_str(), forms));
    format_body(node, source, config, 1, declared, &mut lines);
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
        return lines_keeping_strings(node, source);
    }
    let mut cursor = variants.walk();
    let children: Vec<Node> = variants.children(&mut cursor).collect();
    let parts: Vec<&str> = children
        .iter()
        .filter(|c| c.kind() != "|")
        .map(|c| node_text(*c, source))
        .collect();
    let title = node
        .child_by_field_name(field::TITLE)
        .map(|n| format!(" {}", node_text(n, source)))
        .unwrap_or_default();
    let head = format!("{} {}{title} = ", text(field::KIND), text(field::NAME));
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
    forms: Forms,
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
            kind::FIELD => field_member(*child, source, config, depth, forms),
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
                    value[0].clone()
                };
                lines.push(format!("{indent}{key}{}{first}", pad(&key)));
                lines.extend(value[1..].iter().cloned());
                // Annotations of a multi-line value follow its last line, not
                // its first: inside a string they would be its text.
                if value.len() > 1
                    && !annotations.is_empty()
                    && let Some(last) = lines.last_mut()
                {
                    last.push(' ');
                    last.push_str(&anns);
                }
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

fn field_member(
    node: Node,
    source: &str,
    config: &FormatConfig,
    depth: usize,
    forms: Forms,
) -> Member {
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
            format_body(value, source, config, depth + 1, None, &mut body);
            Member::Nested {
                key,
                lines: body,
                annotations,
            }
        }
        kind::LIST => Member::Field {
            value: list_value(value, source, config, depth, &key, forms),
            key,
            annotations,
        },
        kind::EXPR_GROUP => {
            // `expr {` follows the key; each expression line is indented one
            // level inside it; `}` closes at the field's level
            // (formatting_consistency).
            let indent = config.indent_str();
            let text = node_text(value, source);
            let mut lines = text.lines();
            let mut value = vec![lines.next().unwrap_or("").trim_end().to_string()];
            let body: Vec<&str> = lines.collect();
            let (inner, close) = body.split_at(body.len().saturating_sub(1));
            value.extend(
                inner
                    .iter()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    .map(|l| format!("{}{l}", indent.repeat(depth + 1))),
            );
            value.extend(
                close
                    .iter()
                    .map(|l| format!("{}{}", indent.repeat(depth), l.trim())),
            );
            Member::Field {
                key,
                value,
                annotations,
            }
        }
        _ => {
            // Multi-line values (triple-quoted strings, ...) are opaque: the
            // first line follows the key, the rest stays as written. A
            // string's lines are its value, so its trailing whitespace stays.
            let value = match in_form(value, source, &key, forms) {
                Some(text) => vec![text],
                None => lines_keeping_strings(value, source),
            };
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
    key: &str,
    forms: Forms,
) -> Vec<String> {
    let key_len = key.len();
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    if children.iter().any(|c| c.kind() == kind::COMMENT) {
        return lines_keeping_strings(node, source);
    }
    let items: Vec<String> = children
        .iter()
        .filter(|c| !matches!(c.kind(), "[" | "]" | ","))
        .map(|c| in_form(*c, source, key, forms).unwrap_or_else(|| one_line(*c, source)))
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

/// `node`, a string or a bare word, written in its field's form when the
/// declared type writes it the other way (`FieldForms::rewrite`, the rule
/// `check` reports as W159); `None` leaves it as written.
fn in_form(node: Node, source: &str, key: &str, forms: Forms) -> Option<String> {
    let (kind, forms) = forms?;
    let text = node_text(node, source);
    let (written, word) = match node.kind() {
        kind::STRING => (Written::Quoted, text.strip_prefix('"')?.strip_suffix('"')?),
        kind::IDENTIFIER => (Written::Bare, text),
        _ => return None,
    };
    Some(match forms.rewrite(kind, key, written, word)? {
        Written::Bare => word.to_string(),
        _ => format!("\"{word}\""),
    })
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
