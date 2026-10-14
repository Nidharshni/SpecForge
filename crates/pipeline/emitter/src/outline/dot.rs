use std::fmt::Write;

use super::{OutlineDetail, OutlineIntermediate, OutlineOptions};
use crate::diagram::{escape_dot, escape_record_field, extension_id as sanitize_id, theme_color};

pub(super) fn render_dot(outline: &OutlineIntermediate, options: &OutlineOptions) -> String {
    let mut out = String::new();

    writeln!(out, "digraph extensions {{").unwrap();
    writeln!(out, "    rankdir=TB;").unwrap();
    writeln!(
        out,
        "    node [shape=record, style=filled, fontname=\"Helvetica\"];"
    )
    .unwrap();
    writeln!(out, "    edge [fontname=\"Helvetica\", fontsize=10];").unwrap();
    writeln!(out).unwrap();

    // Extension nodes: record labels, each field escaped on its own so the
    // template's separators stay the only record syntax.
    for ext in &outline.extensions {
        let id = sanitize_id(&ext.name);
        let color = theme_color(ext.color.as_deref());
        let label = if options.detail == OutlineDetail::All {
            let kinds: Vec<String> = ext
                .entity_kinds
                .iter()
                .map(|k| escape_record_field(&k.keyword))
                .collect();
            format!(
                "{{ {} | {} | {} entities, {} edges | {} }}",
                escape_record_field(&ext.name),
                escape_record_field(&ext.version),
                ext.entity_kinds.len(),
                ext.edge_types.len(),
                kinds.join(", ")
            )
        } else {
            format!(
                "{{ {} | {} | {} entities, {} edges }}",
                escape_record_field(&ext.name),
                escape_record_field(&ext.version),
                ext.entity_kinds.len(),
                ext.edge_types.len()
            )
        };
        writeln!(
            out,
            "    {} [label=\"{}\", fillcolor=\"{}\", fontcolor=\"white\"];",
            id, label, color
        )
        .unwrap();
    }

    writeln!(out).unwrap();

    // Dependency edges (filtered by visibility mode)
    let visible_deps = super::filter_dependencies(&outline.dependencies, options.deps);
    for dep in &visible_deps {
        let from_id = sanitize_id(&dep.from);
        let to_id = sanitize_id(&dep.to);
        if dep.optional {
            writeln!(
                out,
                "    {} -> {} [label=\"optional {}\", style=dashed];",
                from_id,
                to_id,
                escape_dot(&dep.version)
            )
            .unwrap();
        } else {
            writeln!(
                out,
                "    {} -> {} [label=\"depends {}\"];",
                from_id,
                to_id,
                escape_dot(&dep.version)
            )
            .unwrap();
        }
    }

    // Enhancement edges (dashed)
    for enh in &outline.enhancements {
        let from_id = sanitize_id(&enh.enhancer);
        let to_id = sanitize_id(&enh.owner);
        writeln!(
            out,
            "    {} -> {} [label=\"enhances {} (+{})\", style=dashed, color=\"#999999\"];",
            from_id,
            to_id,
            escape_dot(&enh.target_kind),
            enh.field_count
        )
        .unwrap();
    }

    // Cross-extension edges (dotted red)
    for ce in &outline.cross_edges {
        let from_id = sanitize_id(&ce.owner_extension);
        let to_id = sanitize_id(&ce.target_extension);
        writeln!(
            out,
            "    {} -> {} [label=\"{}\", style=dotted, color=\"#e74c3c\"];",
            from_id,
            to_id,
            escape_dot(&ce.edge_label)
        )
        .unwrap();
    }

    writeln!(out, "}}").unwrap();
    out
}
