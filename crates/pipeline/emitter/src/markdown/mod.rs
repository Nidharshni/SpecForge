//! The review document (ADR 0152): the graph export as one Markdown document
//! a person reads without tooling. It reads only field declarations: a
//! `normative` field is quoted, a field whose value names entities is a Links
//! row, a prose-shaped value is a paragraph or list, the rest a Fields row.
//! The selection chooses the sections; links, titles and referrers come from
//! the whole graph.

mod escape;

use std::collections::BTreeSet;

use specforge_graph::{FieldForms, FieldMap, FieldValue, Graph, Node, Written};
use specforge_registry::snapshot::field_text;

use crate::emit::EmitOptions;
pub use escape::quote;
use escape::{cell, dedent, inline};

/// The `—` of a cell with nothing to say.
const NONE: &str = "—";

/// One row of a Links or Referenced-by table: the field, the entity named,
/// and the entity when it resolves.
struct Row<'g> {
    field: &'g str,
    id: &'g str,
    entity: Option<&'g Node>,
}

/// `selected` as a document. `graph` names what the sections link to.
pub(crate) fn render(graph: &Graph, selected: &Graph, options: &EmitOptions<'_>) -> String {
    let mut blocks = vec!["# Specification".to_string(), selection(selected, options)];
    for node in order(selected, options.scope) {
        blocks.push(section(graph, node, options));
    }
    blocks.join("\n\n")
}

/// Every entity of `selected`, the scoped one first, then by kind and id.
fn order<'g>(selected: &'g Graph, scope: Option<&str>) -> Vec<&'g Node> {
    let mut nodes = selected.nodes();
    nodes.sort_by_key(|n| {
        (
            Some(n.id.raw.as_str()) != scope,
            n.kind.raw.as_str(),
            n.id.raw.as_str(),
        )
    });
    nodes
}

/// The line under the title: how many entities, and the request that chose them.
fn selection(selected: &Graph, options: &EmitOptions<'_>) -> String {
    let count = selected.node_count();
    let mut line = format!("{count} {}", if count == 1 { "entity" } else { "entities" });
    if let Some(scope) = options.scope {
        line.push_str(&format!(" around `{scope}`"));
        if let Some(depth) = options.depth {
            line.push_str(&format!(" (depth {depth})"));
        }
    }
    let kinds: BTreeSet<&str> = options.kind_filter.iter().copied().collect();
    if !kinds.is_empty() {
        let names: Vec<String> = kinds.iter().map(|k| inline(k).into_owned()).collect();
        let noun = if names.len() == 1 { "kind" } else { "kinds" };
        line.push_str(&format!(", of {noun} {}", names.join(", ")));
    }
    line.push('.');
    line
}

/// One entity: its blocks, separated by one blank line, none written empty.
fn section(graph: &Graph, node: &Node, options: &EmitOptions<'_>) -> String {
    let id = node.id.raw.as_str();
    let kind = node.kind.raw.as_str();
    let mut heading = format!("## {kind} `{id}`");
    if let Some(title) = &node.title {
        heading.push_str(&format!(" — {}", cell(title)));
    }
    let span = &node.source_span;
    let mut blocks = vec![heading, format!("`{}:{}`", span.file, span.start_line)];

    let links = links(graph, node, options.field_forms);
    let linked: BTreeSet<&str> = links.iter().map(|r| r.field).collect();
    let mut fields = Vec::new();
    let mut prose = Vec::new();
    for entry in node.fields.entries() {
        let key = entry.key.as_str();
        let normative = options
            .field_registry
            .and_then(|r| r.get(kind, key))
            .is_some_and(|f| f.declared().normative);
        if normative {
            let text = field_text(&entry.value);
            blocks.push(format!("**{key}**\n\n{}", escape::quote(&inline(&text))));
        } else if linked.contains(key) {
            continue;
        } else if let Some(text) = prose_of(&entry.value) {
            prose.push(format!("**{key}**\n\n{text}"));
        } else {
            fields.push(vec![key.to_string(), cell(&field_text(&entry.value))]);
        }
    }
    if !fields.is_empty() {
        blocks.push(format!(
            "**Fields**\n\n{}",
            table(&["Field", "Value"], fields)
        ));
    }
    blocks.extend(prose);
    if !links.is_empty() {
        blocks.push(format!("**Links**\n\n{}", entity_table(&links)));
    }
    let mut referrers: Vec<Row> = graph
        .edges_to(id)
        .into_iter()
        .map(|e| Row {
            field: e.label.as_str(),
            id: e.source.as_str(),
            entity: graph.node(e.source.as_str()),
        })
        .collect();
    referrers.sort_by_key(|r| (r.entity.map(|n| n.kind.raw.as_str()), r.id, r.field));
    if !referrers.is_empty() {
        blocks.push(format!("**Referenced by**\n\n{}", entity_table(&referrers)));
    }
    if !node.obligations.is_empty() {
        let rows = node.obligations.iter().enumerate().map(|(i, o)| {
            let kind = if o.kind.is_empty() {
                NONE.to_string()
            } else {
                cell(&o.kind)
            };
            vec![(i + 1).to_string(), kind, cell(&o.description)]
        });
        blocks.push(format!(
            "**Verify**\n\n{}",
            table(&["#", "Kind", "Obligation"], rows.collect())
        ));
    }
    blocks.join("\n\n")
}

/// `node`'s references: each written reference in field and written order
/// (linked, or unresolved), then each edge from it not listed yet (derived
/// ones; all of them without `forms`), in edge order.
fn links<'g>(graph: &'g Graph, node: &'g Node, forms: Option<&FieldForms>) -> Vec<Row<'g>> {
    let edges = graph.edges_from(node.id.raw.as_str());
    let linked = |field: &str, target: &str| {
        edges
            .iter()
            .any(|e| e.label.as_str() == field && e.target.as_str() == target)
    };
    let mut rows = Vec::new();
    if let Some(forms) = forms {
        for entry in node.fields.entries() {
            let field = entry.key.as_str();
            for reference in forms
                .reading(node.kind.raw.as_str(), field)
                .references(entry)
            {
                let entity = linked(field, reference.id)
                    .then(|| graph.node(reference.id))
                    .flatten();
                rows.push(Row {
                    field,
                    id: reference.id,
                    entity,
                });
            }
        }
    }
    for edge in &edges {
        let (field, target) = (edge.label.as_str(), edge.target.as_str());
        if !rows.iter().any(|r| r.field == field && r.id == target) {
            rows.push(Row {
                field,
                id: target,
                entity: graph.node(target),
            });
        }
    }
    rows
}

/// A prose-shaped value as a paragraph or list: a multi-line string, a list
/// of quoted strings, a block. `None` for any other value.
fn prose_of(value: &FieldValue) -> Option<String> {
    match value {
        FieldValue::String(s) if s.contains('\n') => Some(dedent(&inline(s)).join("\n")),
        FieldValue::List(items)
            if !items.is_empty() && items.iter().all(|i| i.written == Written::Quoted) =>
        {
            let lines: Vec<String> = items
                .iter()
                .map(|i| format!("- {}", inline(&i.text).replace('\n', " ")))
                .collect();
            Some(lines.join("\n"))
        }
        FieldValue::Block(block) => {
            let mut lines = Vec::new();
            block_lines(block, 0, &mut lines);
            Some(lines.join("\n"))
        }
        _ => None,
    }
}

/// A `| Field | Entity | Kind | Title |` table of `rows`.
fn entity_table(rows: &[Row<'_>]) -> String {
    let rows = rows
        .iter()
        .map(|r| {
            let (kind, title) = match r.entity {
                Some(n) => (
                    n.kind.raw.as_str().to_string(),
                    n.title.as_deref().map_or(NONE.to_string(), cell),
                ),
                None => (NONE.to_string(), "*unresolved*".to_string()),
            };
            vec![r.field.to_string(), format!("`{}`", r.id), kind, title]
        })
        .collect();
    table(&["Field", "Entity", "Kind", "Title"], rows)
}

/// A block as `- key: text` lines, nested two spaces per level.
fn block_lines(block: &FieldMap, depth: usize, out: &mut Vec<String>) {
    let pad = "  ".repeat(depth);
    for entry in block.entries() {
        match &entry.value {
            FieldValue::Block(inner) => {
                out.push(format!("{pad}- {}:", entry.key));
                block_lines(inner, depth + 1, out);
            }
            value => out.push(format!(
                "{pad}- {}: {}",
                entry.key,
                inline(&field_text(value)).replace('\n', " ")
            )),
        }
    }
}

/// A table: the header, its rule, then one line per row.
fn table(header: &[&str], rows: Vec<Vec<String>>) -> String {
    let line = |cells: Vec<&str>| format!("| {} |", cells.join(" | "));
    let mut lines = vec![
        line(header.to_vec()),
        format!("|{}", "---|".repeat(header.len())),
    ];
    lines.extend(
        rows.iter()
            .map(|r| line(r.iter().map(String::as_str).collect())),
    );
    lines.join("\n")
}
