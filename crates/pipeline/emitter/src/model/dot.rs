use std::fmt::{self, Write};

use crate::text;

use super::{GroupBy, ModelIntermediate, ModelOptions};
use crate::diagram::{escape_dot, escape_html};

pub(super) fn render_dot(model: &ModelIntermediate, options: &ModelOptions) -> String {
    text::written(|out| {
        writeln!(out, "digraph model {{")?;
        writeln!(out, "  rankdir=LR;")?;
        writeln!(out, "  node [shape=none, fontname=\"Helvetica\"];")?;
        writeln!(out, "  edge [fontname=\"Helvetica\", fontsize=10];")?;

        match options.group_by {
            GroupBy::Extension => render_grouped(model, out)?,
            GroupBy::None => render_flat(model, out)?,
        }

        // Edges
        for rel in &model.relationships {
            writeln!(
                out,
                "  {} -> {} [label=\"{}\\n[{}]\"];",
                rel.source,
                rel.target,
                escape_dot(&rel.name),
                rel.cardinality
            )?;
        }

        writeln!(out, "}}")?;

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

        let ext_color = model.extension_color(&ext.name);
        let cluster_id = ext.name.replace("@specforge/", "").replace('/', "_");

        writeln!(out)?;
        writeln!(out, "  subgraph cluster_{} {{", cluster_id)?;
        writeln!(out, "    label=\"{}\";", escape_dot(&ext.name))?;
        writeln!(out, "    style=dashed;")?;
        writeln!(out, "    color=\"{}\";", ext_color)?;

        for entity in entities {
            render_entity(entity, entity_color(entity, ext_color), out)?;
        }

        writeln!(out, "  }}")?;
    }
    Ok(())
}

fn render_flat(model: &ModelIntermediate, out: &mut String) -> fmt::Result {
    for entity in &model.entities {
        render_entity(
            entity,
            entity_color(entity, model.extension_color(&entity.extension)),
            out,
        )?;
    }
    Ok(())
}

/// Per-kind declared color wins; the extension palette is the fallback (C13-03).
fn entity_color<'a>(entity: &'a super::ModelEntity, fallback: &'a str) -> &'a str {
    entity.dot_color.as_deref().unwrap_or(fallback)
}

fn render_entity(entity: &super::ModelEntity, color: &str, out: &mut String) -> fmt::Result {
    writeln!(out)?;

    let has_contributions = entity
        .fields
        .iter()
        .any(|f| f.contributed_by.is_some() || f.contribution.is_some());
    let colspan = if has_contributions { 5 } else { 3 };

    // HTML-like labels: every text and attribute value is HTML-escaped
    // (a bare `>` would end the label).
    let color = escape_html(color);
    let header_label = escape_html(&if entity.enhanced_by.is_empty() {
        entity.name.clone()
    } else {
        format!("{} (+{})", entity.name, entity.enhanced_by.join(", +"))
    });

    if entity.fields.is_empty() {
        writeln!(out, "    {} [label=<", entity.name)?;
        writeln!(
            out,
            "      <table border=\"1\" cellborder=\"0\" cellspacing=\"0\">"
        )?;
        writeln!(
            out,
            "        <tr><td bgcolor=\"{}\" colspan=\"{}\"><font color=\"white\"><b>{}</b></font></td></tr>",
            color, colspan, header_label
        )?;
        writeln!(out, "      </table>")?;
        writeln!(out, "    >];")?;
    } else {
        writeln!(out, "    {} [label=<", entity.name)?;
        writeln!(
            out,
            "      <table border=\"1\" cellborder=\"0\" cellspacing=\"0\">"
        )?;
        writeln!(
            out,
            "        <tr><td bgcolor=\"{}\" colspan=\"{}\"><font color=\"white\"><b>{}</b></font></td></tr>",
            color, colspan, header_label
        )?;

        for field in &entity.fields {
            let name_str = if field.required || field.is_primary_key {
                format!("<b>{}</b>", escape_html(&field.name))
            } else {
                escape_html(&field.name)
            };

            let marker = escape_html(&if field.is_primary_key {
                "PK".to_string()
            } else if let Some(ref target) = field.references {
                format!("-> {}", target)
            } else {
                String::new()
            });
            let field_type = escape_html(&field.field_type.to_string());

            if has_contributions {
                // Graphviz rejects an empty `<i>` or `<font>`: wrap text only.
                let contribution = field
                    .contribution
                    .as_deref()
                    .filter(|c| !c.is_empty())
                    .map(|c| format!("<i>{}</i>", escape_html(c)))
                    .unwrap_or_default();
                let source = field
                    .contributed_by
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|s| format!("<font color=\"gray\">{}</font>", escape_html(s)))
                    .unwrap_or_default();
                writeln!(
                    out,
                    "        <tr><td align=\"left\">{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    name_str, field_type, marker, contribution, source
                )?;
            } else {
                writeln!(
                    out,
                    "        <tr><td align=\"left\">{}</td><td>{}</td><td>{}</td></tr>",
                    name_str, field_type, marker
                )?;
            }
        }

        writeln!(out, "      </table>")?;
        writeln!(out, "    >];")?;
    }
    Ok(())
}
