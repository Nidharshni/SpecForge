use std::fmt::Write;

use super::{GroupBy, ModelIntermediate, ModelOptions};
use crate::diagram::markdown_cell;

pub(super) fn render_markdown(model: &ModelIntermediate, options: &ModelOptions) -> String {
    let mut out = String::new();

    // Title
    writeln!(out, "# Logical Data Model").unwrap();
    writeln!(out).unwrap();

    // Preamble
    writeln!(out, "## How to read this model").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "This document describes the **schema** of a SpecForge project — the entity kinds"
    )
    .unwrap();
    writeln!(
        out,
        "(analogous to database tables), their fields (columns), and the relationships"
    )
    .unwrap();
    writeln!(
        out,
        "(foreign keys) between them. It does NOT show actual entity instances. Entity"
    )
    .unwrap();
    writeln!(
        out,
        "kinds and fields are declared by SpecForge extensions. Relationships have"
    )
    .unwrap();
    writeln!(
        out,
        "cardinality: 1:1 (one-to-one), 1:N (one-to-many), N:1 (many-to-one), or"
    )
    .unwrap();
    writeln!(out, "N:M (many-to-many).").unwrap();
    writeln!(out).unwrap();

    // Extension summary table
    if !model.extensions.is_empty() {
        writeln!(out, "## Extensions").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "| Extension | Version | Entity Kinds | Edge Types |").unwrap();
        writeln!(out, "|-----------|---------|-------------|------------|").unwrap();
        for ext in &model.extensions {
            writeln!(
                out,
                "| {} | {} | {} | {} |",
                markdown_cell(&ext.name),
                markdown_cell(&ext.version),
                ext.entity_count,
                ext.edge_count
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    // Entities — grouped or flat
    match options.group_by {
        GroupBy::Extension => render_grouped(model, &mut out),
        GroupBy::None => render_flat(model, &mut out),
    }

    // Summary — use extension edge_count sum (unique edge type labels)
    // rather than relationships.len() which counts expanded source×target pairs
    let total_edges: usize = model.extensions.iter().map(|e| e.edge_count).sum();
    writeln!(
        out,
        "{} entity kinds, {} edge types across {} extensions.",
        model.entities.len(),
        total_edges,
        model.extensions.len()
    )
    .unwrap();

    out
}

fn render_grouped(model: &ModelIntermediate, out: &mut String) {
    for ext in &model.extensions {
        let entities: Vec<_> = model
            .entities
            .iter()
            .filter(|e| e.extension == ext.name)
            .collect();
        if entities.is_empty() {
            continue;
        }

        writeln!(out, "## {}", ext.name).unwrap();
        writeln!(out).unwrap();

        for entity in entities {
            render_entity(entity, model, out);
        }
    }
}

fn render_flat(model: &ModelIntermediate, out: &mut String) {
    for entity in &model.entities {
        render_entity(entity, model, out);
    }
}

fn render_entity(entity: &super::ModelEntity, model: &ModelIntermediate, out: &mut String) {
    writeln!(out, "### {}", entity.name).unwrap();

    if !entity.enhanced_by.is_empty() {
        writeln!(out, "*Enhanced by: {}*", entity.enhanced_by.join(", ")).unwrap();
    }
    writeln!(out).unwrap();

    // Field table
    if !entity.fields.is_empty() {
        writeln!(
            out,
            "| Field | Type | Required | Contribution | Source | Description |"
        )
        .unwrap();
        writeln!(
            out,
            "|-------|------|----------|--------------|--------|-------------|"
        )
        .unwrap();

        for field in &entity.fields {
            let type_str = if let Some(ref target) = field.references {
                format!("{}({})", field.field_type, target)
            } else {
                field.field_type.to_string()
            };

            let required = if field.required { "yes" } else { "no" };
            let contribution = field.contribution.as_deref().unwrap_or("");
            let source = field.contributed_by.as_deref().unwrap_or("");
            let description = field.description.as_deref().unwrap_or("");

            writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} |",
                markdown_cell(&field.name),
                markdown_cell(&type_str),
                required,
                markdown_cell(contribution),
                markdown_cell(source),
                markdown_cell(description)
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    // Relationships for this entity
    let rels: Vec<_> = model
        .relationships
        .iter()
        .filter(|r| r.source == entity.name || r.target == entity.name)
        .collect();

    if !rels.is_empty() {
        writeln!(out, "**Relationships:**").unwrap();
        for rel in rels {
            if rel.source == entity.name {
                writeln!(
                    out,
                    "- {} --({})--> {} [{}]",
                    rel.source, rel.name, rel.target, rel.cardinality
                )
                .unwrap();
            } else {
                writeln!(
                    out,
                    "- {} <--({})-- {} [{}]",
                    entity.name, rel.name, rel.source, rel.cardinality
                )
                .unwrap();
            }
        }
        writeln!(out).unwrap();
    }
}
