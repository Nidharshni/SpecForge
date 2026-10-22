//! The review document (ADR 0152): the graph export as one Markdown document
//! a person reads without tooling. It reads only field declarations: a
//! `normative` field is quoted, a field whose value names entities is a Links
//! row, a prose-shaped value is a paragraph or list, the rest a Fields row.
//! The selection chooses the sections; links, titles and referrers come from
//! the whole graph.

mod escape;

use std::collections::BTreeSet;

use specforge_common::Sym;
use specforge_graph::{FieldForms, FieldMap, FieldValue, Graph, Members, Node, Selected, Written};
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
    /// The field's annotations as written (` @a @b`), empty for a referrer's edge label.
    noted: String,
    id: &'g str,
    entity: Option<&'g Node>,
}

/// `selected` as a document. `graph` names what the sections link to.
pub(crate) fn render(graph: &Graph, selected: &Graph, options: &EmitOptions<'_>) -> String {
    let mut blocks = vec!["# Specification".to_string(), selection(selected, options)];
    for node in order(selected, options.selected.and_then(Selected::root)) {
        blocks.push(section(graph, node, options));
    }
    blocks.join("\n\n")
}

/// Every entity of `selected`, the scoped one first, then by kind and id.
fn order(selected: &Graph, scope: Option<Sym>) -> Vec<&Node> {
    let mut nodes = selected.nodes();
    nodes.sort_by_key(|n| {
        (
            Some(n.id.raw) != scope,
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
    if let Some(chosen) = options.selected
        && let Some(root) = chosen.root()
    {
        line.push_str(&format!(" around `{root}`"));
        if let Some(depth) = chosen.depth() {
            line.push_str(&format!(" (depth {depth})"));
        }
    }
    let kinds: BTreeSet<&str> = options
        .selected
        .into_iter()
        .flat_map(|chosen| chosen.kinds())
        .map(|kind| kind.as_str())
        .collect();
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

    let members = Members::of(node);
    let links = links(graph, node, options.registries.field_forms(), &members);
    let linked: BTreeSet<&str> = links.iter().map(|r| r.field).collect();
    let mut fields = Vec::new();
    let mut prose = Vec::new();
    for entry in node.fields.entries() {
        let key = entry.key.as_str();
        let normative = options
            .registries
            .fields()
            .get(kind, key)
            .is_some_and(|f| f.declared().normative);
        if normative {
            let text = field_text(&entry.value);
            blocks.push(format!(
                "**{key}**{}\n\n{}",
                inline(&noted(key, &members)),
                escape::quote(&inline(&text))
            ));
        } else if linked.contains(key) {
            continue;
        } else if let Some(text) = prose_of(&entry.value, key, &members) {
            prose.push(format!(
                "**{key}**{}\n\n{text}",
                inline(&noted(key, &members))
            ));
        } else {
            let value = format!("{}{}", field_text(&entry.value), noted(key, &members));
            fields.push(vec![key.to_string(), cell(&value)]);
        }
    }
    if !fields.is_empty() {
        blocks.push(format!(
            "**Fields**\n\n{}",
            table(&["Field", "Value"], fields)
        ));
    }
    blocks.extend(prose);
    if !members.methods.is_empty() {
        let lines: Vec<String> = members
            .methods
            .iter()
            .map(|m| format!("- `{}`", m.signature()))
            .collect();
        blocks.push(format!("**Methods**\n\n{}", lines.join("\n")));
    }
    if !links.is_empty() {
        blocks.push(format!("**Links**\n\n{}", entity_table(&links)));
    }
    let mut referrers: Vec<Row> = graph
        .edges_to(id)
        .into_iter()
        .map(|e| Row {
            field: e.label.as_str(),
            noted: String::new(),
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
/// ones), in edge order.
fn links<'g>(
    graph: &'g Graph,
    node: &'g Node,
    forms: &FieldForms,
    members: &Members,
) -> Vec<Row<'g>> {
    let edges = graph.edges_from(node.id.raw.as_str());
    let linked = |field: &str, target: &str| {
        edges
            .iter()
            .any(|e| e.label.as_str() == field && e.target.as_str() == target)
    };
    let mut rows = Vec::new();
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
                noted: noted(field, members),
                id: reference.id,
                entity,
            });
        }
    }
    for edge in &edges {
        let (field, target) = (edge.label.as_str(), edge.target.as_str());
        if !rows.iter().any(|r| r.field == field && r.id == target) {
            rows.push(Row {
                field,
                noted: String::new(),
                id: target,
                entity: graph.node(target),
            });
        }
    }
    rows
}

/// A prose-shaped value as a paragraph or list: a multi-line string, a list
/// of quoted strings, a block. `None` for any other value.
fn prose_of(value: &FieldValue, key: &str, members: &Members) -> Option<String> {
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
            block_lines(block, key, 0, members, &mut lines);
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
            vec![
                format!("{}{}", r.field, cell(&r.noted)),
                format!("`{}`", r.id),
                kind,
                title,
            ]
        })
        .collect();
    table(&["Field", "Entity", "Kind", "Title"], rows)
}

/// The annotations written after the field at `path`, each with a space before it; empty when it has none.
fn noted(path: &str, members: &Members) -> String {
    members.annotations.get(path).map_or_else(String::new, |a| {
        a.iter().map(|a| format!(" {}", a.spelled())).collect()
    })
}

/// A block as `- key: text` lines, nested two spaces per level.
fn block_lines(
    block: &FieldMap,
    path: &str,
    depth: usize,
    members: &Members,
    out: &mut Vec<String>,
) {
    let pad = "  ".repeat(depth);
    for entry in block.entries() {
        let path = format!("{path}.{}", entry.key);
        let note = inline(&noted(&path, members)).into_owned();
        match &entry.value {
            FieldValue::Block(inner) => {
                out.push(format!("{pad}- {}:{note}", entry.key));
                block_lines(inner, &path, depth + 1, members, out);
            }
            value => out.push(format!(
                "{pad}- {}: {}{note}",
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
