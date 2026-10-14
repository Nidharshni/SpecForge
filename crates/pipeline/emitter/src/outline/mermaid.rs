use std::fmt::Write;

use super::{OutlineDetail, OutlineIntermediate, OutlineOptions};
use crate::diagram::{escape_mermaid, extension_id as sanitize_id};

/// Color palette for extension cards.
/// Each entry: (subgraph_fill, subgraph_stroke, inner_stroke, text_color)
const PALETTE: &[(&str, &str, &str, &str)] = &[
    ("#e8f5e9", "#2e7d32", "#66bb6a", "#1b5e20"), // green
    ("#e3f2fd", "#1565c0", "#42a5f5", "#0d47a1"), // blue
    ("#fffde7", "#f9a825", "#ffca28", "#e65100"), // amber
    ("#f3e5f5", "#7b1fa2", "#ab47bc", "#4a148c"), // purple
    ("#fce4ec", "#c62828", "#ef5350", "#b71c1c"), // red
    ("#e0f2f1", "#00695c", "#26a69a", "#004d40"), // teal
];

pub(super) fn render_mermaid(outline: &OutlineIntermediate, options: &OutlineOptions) -> String {
    let mut out = String::new();
    let mut edge_count: usize = 0;

    writeln!(out, "flowchart TB").unwrap();

    // Extension subgraph cards
    for (i, ext) in outline.extensions.iter().enumerate() {
        let id = sanitize_id(&ext.name);
        let card_id = format!("{}_card", id);
        let content = build_card_content(ext, outline, options);

        writeln!(out).unwrap();
        writeln!(
            out,
            "    subgraph {}[\"  {} v{}  \"]",
            id,
            escape_mermaid(&ext.name),
            escape_mermaid(&ext.version)
        )
        .unwrap();
        writeln!(out, "        {}[\"{}\"]", card_id, content).unwrap();
        writeln!(out, "    end").unwrap();

        // classDef for inner card node
        let (_, _, inner_stroke, _) = palette(i);
        writeln!(
            out,
            "    classDef cls_{} fill:#fff,stroke:{},stroke-width:1.5px,color:#333",
            id, inner_stroke
        )
        .unwrap();
        writeln!(out, "    class {} cls_{}", card_id, id).unwrap();
    }

    writeln!(out).unwrap();

    // Filter dependencies by visibility mode
    let visible_deps = super::filter_dependencies(&outline.dependencies, options.deps);

    // Required dependency edges (solid)
    for dep in visible_deps.iter().filter(|d| !d.optional) {
        let from_id = sanitize_id(&dep.from);
        let to_id = sanitize_id(&dep.to);
        writeln!(out, "    {} -->|\"depends on\"| {}", from_id, to_id).unwrap();
        edge_count += 1;
    }

    // Optional dependency edges (dashed)
    for dep in visible_deps.iter().filter(|d| d.optional) {
        let from_id = sanitize_id(&dep.from);
        let to_id = sanitize_id(&dep.to);
        writeln!(out, "    {} -.->|\"optional dep\"| {}", from_id, to_id).unwrap();
        edge_count += 1;
    }

    // Enhancement edges (dotted)
    for enh in &outline.enhancements {
        let from_id = sanitize_id(&enh.enhancer);
        let to_id = sanitize_id(&enh.owner);
        writeln!(
            out,
            "    {} -.->|\"enhances {}\"| {}",
            from_id,
            escape_mermaid(&enh.target_kind),
            to_id
        )
        .unwrap();
        edge_count += 1;
    }

    // Position orphan extensions (no edges) with invisible links
    let connected: std::collections::HashSet<String> = visible_deps
        .iter()
        .flat_map(|d| [d.from.clone(), d.to.clone()])
        .chain(
            outline
                .enhancements
                .iter()
                .flat_map(|e| [e.enhancer.clone(), e.owner.clone()]),
        )
        .collect();
    let orphans: Vec<_> = outline
        .extensions
        .iter()
        .filter(|e| !connected.contains(&e.name))
        .collect();
    if !orphans.is_empty()
        && let Some(first_connected) = outline
            .extensions
            .iter()
            .find(|e| connected.contains(&e.name))
    {
        for orphan in &orphans {
            let orphan_id = sanitize_id(&orphan.name);
            let anchor_id = sanitize_id(&first_connected.name);
            writeln!(out, "    {} ~~~ {}", orphan_id, anchor_id).unwrap();
            edge_count += 1;
        }
    }

    writeln!(out).unwrap();

    // Subgraph style directives (backgrounds)
    for (i, ext) in outline.extensions.iter().enumerate() {
        let id = sanitize_id(&ext.name);
        let (fill, stroke, _, text) = palette(i);
        writeln!(
            out,
            "    style {} fill:{},stroke:{},stroke-width:2px,color:{}",
            id, fill, stroke, text
        )
        .unwrap();
    }

    // linkStyle for edge colors
    let mut link_idx = 0;
    // Required deps
    for dep in visible_deps.iter().filter(|d| !d.optional) {
        let color = extension_stroke(&dep.from, &outline.extensions);
        writeln!(
            out,
            "    linkStyle {} stroke:{},stroke-width:2px",
            link_idx, color
        )
        .unwrap();
        link_idx += 1;
    }
    // Optional deps
    for dep in visible_deps.iter().filter(|d| d.optional) {
        let color = extension_stroke(&dep.from, &outline.extensions);
        writeln!(
            out,
            "    linkStyle {} stroke:{},stroke-width:2px,stroke-dasharray:5",
            link_idx, color
        )
        .unwrap();
        link_idx += 1;
    }
    // Enhancements
    for enh in &outline.enhancements {
        let color = extension_stroke(&enh.enhancer, &outline.extensions);
        writeln!(
            out,
            "    linkStyle {} stroke:{},stroke-width:1px,stroke-dasharray:3",
            link_idx, color
        )
        .unwrap();
        link_idx += 1;
    }
    // Invisible links
    while link_idx < edge_count {
        writeln!(out, "    linkStyle {} stroke:none", link_idx).unwrap();
        link_idx += 1;
    }

    out
}

fn build_card_content(
    ext: &super::OutlineExtension,
    outline: &OutlineIntermediate,
    options: &OutlineOptions,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    let divider = "\u{2500}".repeat(25); // ─────────────────────────

    // Stats line (always shown, bold)
    parts.push(format!(
        "<b>{} entities \u{00b7} {} edges \u{00b7} {} rules</b>",
        ext.entity_kinds.len(),
        ext.edge_types.len(),
        ext.validation_rules.len()
    ));

    if options.detail == OutlineDetail::None {
        return parts.join("<br>");
    }

    // Keywords section
    let keywords: Vec<String> = ext
        .entity_kinds
        .iter()
        .map(|k| escape_mermaid(&k.keyword))
        .collect();
    if !keywords.is_empty() {
        parts.push(divider.clone());
        // Balance keywords into rows of 3
        for chunk in keywords.chunks(3) {
            parts.push(chunk.join(" \u{00b7} "));
        }
    }

    // Extras section (italic)
    let mut extras: Vec<String> = Vec::new();

    // Surface counts
    let sc = &ext.surface_counts;
    if sc.cli_commands > 0 || sc.mcp_tools > 0 || sc.mcp_resources > 0 {
        let mut surface_parts = Vec::new();
        if sc.cli_commands > 0 {
            surface_parts.push(format!("{} CLI cmds", sc.cli_commands));
        }
        if sc.mcp_tools > 0 {
            surface_parts.push(format!("{} MCP tools", sc.mcp_tools));
        }
        if sc.mcp_resources > 0 {
            surface_parts.push(format!("{} MCP resources", sc.mcp_resources));
        }
        extras.push(surface_parts.join(" \u{00b7} "));
    }

    // Shared fields
    if !ext.shared_fields.is_empty() {
        let names: Vec<String> = ext
            .shared_fields
            .iter()
            .map(|f| escape_mermaid(&f.name))
            .collect();
        extras.push(format!("shared: {}", names.join(", ")));
    }

    // Enhancement summary
    let ext_enhancements: Vec<_> = outline
        .enhancements
        .iter()
        .filter(|e| e.enhancer == ext.name)
        .collect();
    if !ext_enhancements.is_empty() {
        let summary: Vec<String> = ext_enhancements
            .iter()
            .map(|e| format!("{} +{}", escape_mermaid(&e.target_kind), e.field_count))
            .collect();
        extras.push(format!("enhances: {}", summary.join(" \u{00b7} ")));
    }

    if !extras.is_empty() {
        parts.push(divider);
        for extra in &extras {
            parts.push(format!("<i>{}</i>", extra));
        }
    }

    parts.join("<br>")
}

fn palette(index: usize) -> (&'static str, &'static str, &'static str, &'static str) {
    let entry = PALETTE[index % PALETTE.len()];
    (entry.0, entry.1, entry.2, entry.3)
}

fn extension_stroke(name: &str, extensions: &[super::OutlineExtension]) -> &'static str {
    let idx = extensions.iter().position(|e| e.name == name).unwrap_or(0);
    palette(idx).1
}
