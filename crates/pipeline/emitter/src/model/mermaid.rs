use std::fmt::Write;

use super::{Cardinality, GroupBy, ModelIntermediate, ModelOptions};
use crate::diagram::{escape_mermaid, mermaid_name};

pub(super) fn render_mermaid(model: &ModelIntermediate, options: &ModelOptions) -> String {
    let mut out = String::new();

    writeln!(out, "erDiagram").unwrap();

    match options.group_by {
        GroupBy::Extension => render_grouped(model, &mut out),
        GroupBy::None => render_flat(model, &mut out),
    }

    // Relationships
    if !model.relationships.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "    %% Relationships").unwrap();
        writeln!(out).unwrap();
        for rel in &model.relationships {
            let notation = cardinality_notation(rel.cardinality);
            writeln!(
                out,
                "    {} {} {} : \"{}\"",
                mermaid_name(&rel.source),
                notation,
                mermaid_name(&rel.target),
                escape_mermaid(&rel.name)
            )
            .unwrap();
        }
    }

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

        writeln!(out).unwrap();
        writeln!(out, "    %% {}", escape_mermaid(&ext.name)).unwrap();
        writeln!(out).unwrap();

        for entity in entities {
            render_entity(entity, out);
        }
    }
}

fn render_flat(model: &ModelIntermediate, out: &mut String) {
    for entity in &model.entities {
        writeln!(out).unwrap();
        render_entity(entity, out);
    }
}

fn render_entity(entity: &super::ModelEntity, out: &mut String) {
    if !entity.enhanced_by.is_empty() {
        writeln!(
            out,
            "    %% Enhanced by: {}",
            escape_mermaid(&entity.enhanced_by.join(", "))
        )
        .unwrap();
    }

    if entity.fields.is_empty() {
        // No fields: just declare entity name
        writeln!(out, "    {} {{", mermaid_name(&entity.name)).unwrap();
        writeln!(out, "    }}").unwrap();
    } else {
        writeln!(out, "    {} {{", mermaid_name(&entity.name)).unwrap();
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
            )
            .unwrap();
        }
        writeln!(out, "    }}").unwrap();
    }
}

fn cardinality_notation(c: Cardinality) -> &'static str {
    match c {
        Cardinality::OneToOne => "||--||",
        Cardinality::OneToMany => "||--o{",
        Cardinality::ManyToOne => "}o--||",
        Cardinality::ManyToMany => "}o--o{",
    }
}
