use std::fmt::Write;

use super::{OutlineDetail, OutlineIntermediate, OutlineOptions};
use crate::diagram::markdown_cell;

pub(super) fn render_markdown(outline: &OutlineIntermediate, options: &OutlineOptions) -> String {
    let mut out = String::new();

    writeln!(out, "# Extension Architecture").unwrap();
    writeln!(out).unwrap();

    // Overview table
    if !outline.extensions.is_empty() {
        writeln!(out, "## Overview").unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "| Extension | Version | Entities | Edges | Rules | Enhances |"
        )
        .unwrap();
        writeln!(
            out,
            "|-----------|---------|----------|-------|-------|----------|"
        )
        .unwrap();
        for ext in &outline.extensions {
            let enhances_summary = summarize_enhancements(outline, &ext.name);
            writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} |",
                markdown_cell(&ext.name),
                markdown_cell(&ext.version),
                ext.entity_kinds.len(),
                ext.edge_types.len(),
                ext.validation_rules.len(),
                if enhances_summary.is_empty() {
                    "\u{2014}".to_string()
                } else {
                    markdown_cell(&enhances_summary).into_owned()
                }
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    // Dependencies (filtered by visibility mode)
    let visible_deps = super::filter_dependencies(&outline.dependencies, options.deps);
    if !visible_deps.is_empty() {
        writeln!(out, "## Dependencies").unwrap();
        writeln!(out).unwrap();
        for dep in &visible_deps {
            let mut suffix = String::new();
            if dep.optional {
                suffix.push_str(", optional");
            }
            if dep.kind == super::DependencyKind::Effective {
                suffix.push_str(", transitive");
            } else if dep.kind == super::DependencyKind::Transitive {
                suffix.push_str(", transitive, unused");
            }
            writeln!(
                out,
                "- {} \u{2192} {} ({}{})",
                dep.from, dep.to, dep.version, suffix
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    // Enhancements
    if !outline.enhancements.is_empty() {
        writeln!(out, "## Enhancements").unwrap();
        writeln!(out).unwrap();
        for enh in &outline.enhancements {
            writeln!(
                out,
                "- {} enhances {}/{}: +{} fields ({})",
                enh.enhancer,
                enh.owner,
                enh.target_kind,
                enh.field_count,
                enh.field_names.join(", ")
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    // Cross-extension edges
    if !outline.cross_edges.is_empty() {
        writeln!(out, "## Cross-Extension Edges").unwrap();
        writeln!(out).unwrap();
        for ce in &outline.cross_edges {
            writeln!(
                out,
                "- {} ({}.{} \u{2192} {}.{})",
                ce.edge_label,
                ce.owner_extension,
                ce.source_kind,
                ce.target_extension,
                ce.target_kind
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    if options.detail == OutlineDetail::None {
        // Still show summary even at none level
        render_summary(&mut out, outline);
        return out;
    }

    // Per-extension detail
    writeln!(out, "## Extensions").unwrap();
    writeln!(out).unwrap();

    for ext in &outline.extensions {
        writeln!(
            out,
            "### {} ({} entities, {} edges, {} rules)",
            ext.name,
            ext.entity_kinds.len(),
            ext.edge_types.len(),
            ext.validation_rules.len()
        )
        .unwrap();
        writeln!(out).unwrap();

        // Entity kinds
        if !ext.entity_kinds.is_empty() {
            let keywords: Vec<&str> = ext
                .entity_kinds
                .iter()
                .map(|k| k.keyword.as_str())
                .collect();
            writeln!(out, "**Entity kinds**: {}", keywords.join(", ")).unwrap();
            writeln!(out).unwrap();
        }

        // Contributes
        if !ext.contributes.is_empty() {
            writeln!(out, "**Contributes**: {}", ext.contributes.join(", ")).unwrap();
            writeln!(out).unwrap();
        }

        // Verify kinds
        if !ext.verify_kinds.is_empty() {
            writeln!(out, "**Verify kinds**: {}", ext.verify_kinds.join(", ")).unwrap();
            writeln!(out).unwrap();
        }

        // Validation rules with codes
        if !ext.validation_rules.is_empty() {
            let codes: Vec<&str> = ext
                .validation_rules
                .iter()
                .map(|r| r.code.as_str())
                .collect();
            let errors = ext
                .validation_rules
                .iter()
                .filter(|r| r.severity == "error")
                .count();
            let warnings = ext
                .validation_rules
                .iter()
                .filter(|r| r.severity == "warning")
                .count();
            let infos = ext
                .validation_rules
                .iter()
                .filter(|r| r.severity == "info")
                .count();
            let mut breakdown = Vec::new();
            if errors > 0 {
                breakdown.push(format!("{} errors", errors));
            }
            if warnings > 0 {
                breakdown.push(format!("{} warnings", warnings));
            }
            if infos > 0 {
                breakdown.push(format!("{} info", infos));
            }
            writeln!(
                out,
                "**Validation rules**: {} ({} total)",
                codes.join(", "),
                ext.validation_rules.len()
            )
            .unwrap();
            if !breakdown.is_empty() {
                writeln!(out, "  Severity breakdown: {}", breakdown.join(", ")).unwrap();
            }
            writeln!(out).unwrap();
        }

        // Surface counts
        if ext.surface_counts.cli_commands > 0
            || ext.surface_counts.mcp_tools > 0
            || ext.surface_counts.mcp_resources > 0
        {
            writeln!(
                out,
                "**Surfaces**: {} CLI commands, {} MCP tools, {} MCP resources",
                ext.surface_counts.cli_commands,
                ext.surface_counts.mcp_tools,
                ext.surface_counts.mcp_resources
            )
            .unwrap();
            writeln!(out).unwrap();
        }

        // Shared fields
        if !ext.shared_fields.is_empty() {
            let names: Vec<&str> = ext.shared_fields.iter().map(|f| f.name.as_str()).collect();
            writeln!(
                out,
                "**Shared fields** (applied to all entities): {}",
                names.join(", ")
            )
            .unwrap();
            writeln!(out).unwrap();
        }

        // Enhancement details for this extension
        let ext_enhancements: Vec<_> = outline
            .enhancements
            .iter()
            .filter(|e| e.enhancer == ext.name)
            .collect();
        if !ext_enhancements.is_empty() {
            writeln!(out, "**Enhances**:").unwrap();
            for enh in ext_enhancements {
                writeln!(
                    out,
                    "- {}/{}: +{} fields ({})",
                    enh.owner,
                    enh.target_kind,
                    enh.field_count,
                    enh.field_names.join(", ")
                )
                .unwrap();
            }
            writeln!(out).unwrap();
        }

        // Full field detail (--fields=all)
        if options.detail == OutlineDetail::All {
            for kind in &ext.entity_kinds {
                if kind.fields.is_empty() && kind.enhanced_by.is_empty() {
                    continue;
                }
                writeln!(out, "#### {} ({})", kind.keyword, kind.name).unwrap();
                writeln!(out).unwrap();
                if !kind.fields.is_empty() {
                    writeln!(out, "| Field | Type | Required | Source | Edge | Target |").unwrap();
                    writeln!(out, "|-------|------|----------|--------|------|--------|").unwrap();
                    for f in &kind.fields {
                        writeln!(
                            out,
                            "| {} | {} | {} | {} | {} | {} |",
                            markdown_cell(&f.name),
                            markdown_cell(&f.field_type),
                            if f.required { "yes" } else { "no" },
                            markdown_cell(&f.source_extension),
                            markdown_cell(f.edge.as_deref().unwrap_or("\u{2014}")),
                            markdown_cell(f.target_kind.as_deref().unwrap_or("\u{2014}")),
                        )
                        .unwrap();
                    }
                    writeln!(out).unwrap();
                }
                if !kind.enhanced_by.is_empty() {
                    for attr in &kind.enhanced_by {
                        writeln!(
                            out,
                            "*Enhanced by {}*: +{} fields ({})",
                            attr.source_extension,
                            attr.field_count,
                            attr.field_names.join(", ")
                        )
                        .unwrap();
                    }
                    writeln!(out).unwrap();
                }
            }
        }
    }

    render_summary(&mut out, outline);
    out
}

fn render_summary(out: &mut String, outline: &OutlineIntermediate) {
    let total_entities: usize = outline
        .extensions
        .iter()
        .map(|e| e.entity_kinds.len())
        .sum();
    let total_edges: usize = outline.extensions.iter().map(|e| e.edge_types.len()).sum();
    let total_rules: usize = outline
        .extensions
        .iter()
        .map(|e| e.validation_rules.len())
        .sum();

    writeln!(out, "## Summary").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "- **{}** extensions, **{}** entity kinds, **{}** edge types",
        outline.extensions.len(),
        total_entities,
        total_edges
    )
    .unwrap();
    writeln!(
        out,
        "- **{}** validation rules, **{}** cross-extension edges, **{}** enhancements",
        total_rules,
        outline.cross_edges.len(),
        outline.enhancements.len()
    )
    .unwrap();
    writeln!(out).unwrap();
}

fn summarize_enhancements(outline: &OutlineIntermediate, ext_name: &str) -> String {
    let enhs: Vec<_> = outline
        .enhancements
        .iter()
        .filter(|e| e.enhancer == ext_name)
        .collect();
    if enhs.is_empty() {
        return String::new();
    }
    enhs.iter()
        .map(|e| format!("{} (+{})", e.target_kind, e.field_count))
        .collect::<Vec<_>>()
        .join(", ")
}
