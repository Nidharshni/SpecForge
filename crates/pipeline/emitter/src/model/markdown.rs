use std::fmt::{self, Write};

use crate::text;

use super::{GroupBy, ModelIntermediate, ModelOptions};
use crate::diagram::markdown_cell;

pub(super) fn render_markdown(model: &ModelIntermediate, options: &ModelOptions) -> String {
    text::written(|out| {
        // Title
        writeln!(out, "# Logical Data Model")?;
        writeln!(out)?;

        // Preamble
        writeln!(out, "## How to read this model")?;
        writeln!(out)?;
        writeln!(
            out,
            "This document describes the **schema** of a SpecForge project — the entity kinds"
        )?;
        writeln!(
            out,
            "(analogous to database tables), their fields (columns), and the relationships"
        )?;
        writeln!(
            out,
            "(foreign keys) between them. It does NOT show actual entity instances. Entity"
        )?;
        writeln!(
            out,
            "kinds and fields are declared by SpecForge extensions. Relationships have"
        )?;
        writeln!(
            out,
            "cardinality: 1:1 (one-to-one), 1:N (one-to-many), N:1 (many-to-one), or"
        )?;
        writeln!(out, "N:M (many-to-many).")?;
        writeln!(out)?;

        // Extension summary table
        if !model.extensions.is_empty() {
            writeln!(out, "## Extensions")?;
            writeln!(out)?;
            writeln!(out, "| Extension | Version | Entity Kinds | Edge Types |")?;
            writeln!(out, "|-----------|---------|-------------|------------|")?;
            for ext in &model.extensions {
                writeln!(
                    out,
                    "| {} | {} | {} | {} |",
                    markdown_cell(&ext.name),
                    markdown_cell(&ext.version),
                    ext.entity_count,
                    ext.edge_count
                )?;
            }
            writeln!(out)?;
        }

        // Entities — grouped or flat
        match options.group_by {
            GroupBy::Extension => render_grouped(model, out)?,
            GroupBy::None => render_flat(model, out)?,
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
        )?;

        Ok(())
    })
}

fn render_grouped(model: &ModelIntermediate, out: &mut String) -> fmt::Result {
    for ext in &model.extensions {
        let entities: Vec<_> = model
            .entities
            .iter()
            .filter(|e| e.extension == ext.name)
            .collect();
        if entities.is_empty() {
            continue;
        }

        writeln!(out, "## {}", ext.name)?;
        writeln!(out)?;

        for entity in entities {
            render_entity(entity, model, out)?;
        }
    }
    Ok(())
}

fn render_flat(model: &ModelIntermediate, out: &mut String) -> fmt::Result {
    for entity in &model.entities {
        render_entity(entity, model, out)?;
    }
    Ok(())
}

fn render_entity(
    entity: &super::ModelEntity,
    model: &ModelIntermediate,
    out: &mut String,
) -> fmt::Result {
    writeln!(out, "### {}", entity.name)?;

    if !entity.enhanced_by.is_empty() {
        writeln!(out, "*Enhanced by: {}*", entity.enhanced_by.join(", "))?;
    }
    writeln!(out)?;

    // Field table
    if !entity.fields.is_empty() {
        writeln!(
            out,
            "| Field | Type | Required | Contribution | Source | Description |"
        )?;
        writeln!(
            out,
            "|-------|------|----------|--------------|--------|-------------|"
        )?;

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
            )?;
        }
        writeln!(out)?;
    }

    // Relationships for this entity
    let rels: Vec<_> = model
        .relationships
        .iter()
        .filter(|r| r.source == entity.name || r.target == entity.name)
        .collect();

    if !rels.is_empty() {
        writeln!(out, "**Relationships:**")?;
        for rel in rels {
            if rel.source == entity.name {
                writeln!(
                    out,
                    "- {} --({})--> {} [{}]",
                    rel.source, rel.name, rel.target, rel.cardinality
                )?;
            } else {
                writeln!(
                    out,
                    "- {} <--({})-- {} [{}]",
                    entity.name, rel.name, rel.source, rel.cardinality
                )?;
            }
        }
        writeln!(out)?;
    }
    Ok(())
}
