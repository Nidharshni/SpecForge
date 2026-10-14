use std::fmt::{self, Write};

use crate::text;

use super::{Cardinality, GroupBy, ModelIntermediate, ModelOptions};
use crate::diagram::{escape_mermaid, mermaid_name};

pub(super) fn render_mermaid(model: &ModelIntermediate, options: &ModelOptions) -> String {
    text::written(|out| {
        writeln!(out, "erDiagram")?;

        match options.group_by {
            GroupBy::Extension => render_grouped(model, out)?,
            GroupBy::None => render_flat(model, out)?,
        }

        // Relationships
        if !model.relationships.is_empty() {
            writeln!(out)?;
            writeln!(out, "    %% Relationships")?;
            writeln!(out)?;
            for rel in &model.relationships {
                let notation = cardinality_notation(rel.cardinality);
                writeln!(
                    out,
                    "    {} {} {} : \"{}\"",
                    mermaid_name(&rel.source),
                    notation,
                    mermaid_name(&rel.target),
                    escape_mermaid(&rel.name)
                )?;
            }
        }

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

        writeln!(out)?;
        writeln!(out, "    %% {}", escape_mermaid(&ext.name))?;
        writeln!(out)?;

        for entity in entities {
            render_entity(entity, out)?;
        }
    }
    Ok(())
}

fn render_flat(model: &ModelIntermediate, out: &mut String) -> fmt::Result {
    for entity in &model.entities {
        writeln!(out)?;
        render_entity(entity, out)?;
    }
    Ok(())
}

fn render_entity(entity: &super::ModelEntity, out: &mut String) -> fmt::Result {
    if !entity.enhanced_by.is_empty() {
        writeln!(
            out,
            "    %% Enhanced by: {}",
            escape_mermaid(&entity.enhanced_by.join(", "))
        )?;
    }

    if entity.fields.is_empty() {
        // No fields: just declare entity name
        writeln!(out, "    {} {{", mermaid_name(&entity.name))?;
        writeln!(out, "    }}")?;
    } else {
        writeln!(out, "    {} {{", mermaid_name(&entity.name))?;
        for field in &entity.fields {
            let type_str = field.field_type.to_string();
            let mut markers = Vec::new();

            if field.is_primary_key {
                markers.push("PK");
            }
            if field.references.is_some() {
                markers.push("FK");
            }

            let marker_str = if markers.is_empty() {
                String::new()
            } else {
                format!(" {}", markers.join(","))
            };

            // Build description: contribution + source + original description
            let mut desc_parts: Vec<String> = Vec::new();
            if let Some(ref contrib) = field.contribution {
                desc_parts.push(contrib.clone());
            }
            if let Some(ref source) = field.contributed_by {
                desc_parts.push(format!("from {}", source));
            }
            if let Some(ref desc) = field.description {
                desc_parts.push(desc.clone());
            } else if let Some(ref vals) = field.enum_values {
                desc_parts.push(vals.join(", "));
            }

            // The description sits in a quoted string: its quote, markup and
            // line breaks are escaped before it enters the string.
            let desc_str = if desc_parts.is_empty() {
                String::new()
            } else {
                format!(" \"{}\"", escape_mermaid(&desc_parts.join(" | ")))
            };

            writeln!(
                out,
                "        {} {}{}{}",
                type_str,
                mermaid_name(&field.name),
                marker_str,
                desc_str
            )?;
        }
        writeln!(out, "    }}")?;
    }
    Ok(())
}

fn cardinality_notation(c: Cardinality) -> &'static str {
    match c {
        Cardinality::OneToOne => "||--||",
        Cardinality::OneToMany => "||--o{",
        Cardinality::ManyToOne => "}o--||",
        Cardinality::ManyToMany => "}o--o{",
    }
}
