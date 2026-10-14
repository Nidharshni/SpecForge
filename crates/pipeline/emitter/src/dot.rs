use specforge_graph::{Graph, Node};
use std::collections::BTreeMap;
use std::fmt::Write;

use crate::diagram::{escape_dot, extension_id};

/// Options controlling whole-graph DOT emission (C13-01): per-kind registry
/// styles (C13-00), a label toggle, kind filtering, and per-extension
/// subgraph clusters for the previous flat hairball.
#[derive(Debug, Clone, Copy)]
pub struct DotOptions<'a> {
    /// Registry for per-kind `dot_shape`/`dot_color`/`dot_fillcolor` and for
    /// extension lookup when clustering.
    pub kind_registry: Option<&'a specforge_registry::KindRegistry>,
    /// Include node title labels (default true; `false` emits bare IDs).
    pub labels: bool,
    /// Wrap nodes in one `subgraph cluster_*` per declaring extension
    /// (needs `kind_registry`; kinds absent from the registry stay top-level).
    pub cluster_by_extension: bool,
    /// Emit only nodes whose kind is listed (`None` = all kinds). Edges whose
    /// endpoints were filtered out are dropped with them.
    pub kind_filter: Option<&'a [String]>,
}

impl Default for DotOptions<'_> {
    fn default() -> Self {
        Self {
            kind_registry: None,
            labels: true,
            cluster_by_extension: false,
            kind_filter: None,
        }
    }
}
pub fn emit_dot(graph: &Graph, options: &DotOptions<'_>) -> String {
    let mut out = String::new();
    writeln!(out, "digraph specforge {{").unwrap();
    writeln!(out, "  rankdir=LR;").unwrap();
    writeln!(out, "  node [shape=box];").unwrap();

    let kind_entry = |kind: &str| match options.kind_registry {
        Some(registry) => registry
            .iter()
            .find(|(name, _)| *name == kind)
            .map(|(_, e)| e),
        None => None,
    };
    let included = |node: &Node| {
        options
            .kind_filter
            .is_none_or(|f| f.iter().any(|k| k == node.kind.raw.as_str()))
    };
    let write_node = |out: &mut String, node: &Node, indent: &str| {
        let mut style = String::new();
        if let Some(entry) = kind_entry(node.kind.raw.as_str()) {
            if let Some(shape) = &entry.declared.dot_shape {
                style.push_str(&format!(" shape=\"{}\"", escape_dot(shape)));
            }
            if let Some(color) = &entry.declared.dot_color {
                style.push_str(&format!(" color=\"{}\"", escape_dot(color)));
            }
            if let Some(fill) = &entry.declared.dot_fillcolor {
                style.push_str(&format!(" fillcolor=\"{}\"", escape_dot(fill)));
            }
        }
        let label = if options.labels {
            match &node.title {
                Some(title) => format!(
                    "{}\\n{}",
                    escape_dot(node.id.raw.as_str()),
                    escape_dot(title)
                ),
                None => escape_dot(node.id.raw.as_str()),
            }
        } else {
            escape_dot(node.id.raw.as_str())
        };
        writeln!(
            out,
            "{indent}\"{}\" [label=\"{}\"{}];",
            escape_dot(node.id.raw.as_str()),
            label,
            style
        )
        .unwrap();
    };

    if options.cluster_by_extension {
        let mut clusters: BTreeMap<&str, Vec<&Node>> = BTreeMap::new();
        let mut top_level: Vec<&Node> = Vec::new();
        for node in graph.nodes() {
            if !included(node) {
                continue;
            }
            match kind_entry(node.kind.raw.as_str()).map(|e| e.source_extension.as_str()) {
                Some(ext) => clusters.entry(ext).or_default().push(node),
                None => top_level.push(node),
            }
        }
        for node in top_level {
            write_node(&mut out, node, "  ");
        }
        for (ext, nodes) in clusters {
            let cluster_id = extension_id(ext);
            writeln!(out).unwrap();
            writeln!(out, "  subgraph cluster_{cluster_id} {{").unwrap();
            writeln!(out, "    label=\"{ext}\";").unwrap();
            for node in nodes {
                write_node(&mut out, node, "    ");
            }
            writeln!(out, "  }}").unwrap();
        }
    } else {
        for node in graph.nodes() {
            if included(node) {
                write_node(&mut out, node, "  ");
            }
        }
    }

    let mut edges: Vec<_> = graph.edges().to_vec();
    edges.sort_by(|a, b| (&a.source, &a.target, &a.label).cmp(&(&b.source, &b.target, &b.label)));

    for edge in &edges {
        let keep = graph.node(edge.source.as_str()).is_none_or(included)
            && graph.node(edge.target.as_str()).is_none_or(included);
        if !keep {
            continue;
        }
        writeln!(
            out,
            "  \"{}\" -> \"{}\" [label=\"{}\"];",
            escape_dot(edge.source.as_str()),
            escape_dot(edge.target.as_str()),
            escape_dot(edge.label.as_str())
        )
        .unwrap();
    }

    writeln!(out, "}}").unwrap();
    out
}

#[cfg(test)]
mod dot_escape_tests {
    use super::*;
    use specforge_common::{SourceSpan, Sym};
    use specforge_graph::{EntityId, EntityKind, FieldMap, Node};

    #[test]
    fn hostile_title_cannot_break_the_dot_statement() {
        let mut graph = Graph::new();
        let mut fields = FieldMap::new();
        fields.push(
            Sym::new("description"),
            specforge_parser::FieldValue::String("}\"; evil | ".to_string()),
        );
        graph.add_node(Node {
            id: EntityId {
                raw: Sym::new("evil\"; hack"),
            },
            kind: EntityKind {
                raw: Sym::new("behavior"),
            },
            title: Some("}\"; hack graph".to_string()),
            fields,
            source_span: SourceSpan {
                file: Sym::new("t.spec"),
                start_line: 1,
                start_col: 1,
                end_line: 1,
                end_col: 1,
            },
            methods: Vec::new(),
        });
        let dot = emit_dot(&graph, &DotOptions::default());
        let node_stmts = dot
            .lines()
            .filter(|l| l.trim_start().starts_with('"'))
            .count();
        assert_eq!(
            node_stmts, 1,
            "hostile strings must not split statements: {dot}"
        );
        assert!(
            dot.contains("\\\";"),
            "embedded quote must be escaped: {dot}"
        );
        assert!(
            dot.contains("\\n}"),
            "embedded newline must be escaped: {dot}"
        );
    }
}
