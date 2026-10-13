//! The logical data model: the entity kinds a Graph Protocol schema
//! declares, drawn as tables (their fields as columns, a synthetic `id` as
//! the key) and the edge types between them as relationships with a
//! cardinality. One call, [`export`]. How the model is built, selected and
//! drawn is this module's own (ADR 0007).

mod build;
mod cardinality;
mod dbml;
mod dot;
mod filter;
mod json;
mod markdown;
mod mermaid;

use std::fmt;

use serde::{Deserialize, Serialize};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::FieldType;

use crate::schema::GraphProtocolSchema;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelFormat {
    #[default]
    Markdown,
    Mermaid,
    Dot,
    Json,
    Dbml,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupBy {
    #[default]
    Extension,
    None,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldLevel {
    None,
    #[default]
    Keys,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Cardinality {
    #[serde(rename = "1:1")]
    OneToOne,
    #[serde(rename = "1:N")]
    OneToMany,
    #[serde(rename = "N:1")]
    ManyToOne,
    #[serde(rename = "N:M")]
    ManyToMany,
}

impl fmt::Display for Cardinality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Cardinality::OneToOne => write!(f, "1:1"),
            Cardinality::OneToMany => write!(f, "1:N"),
            Cardinality::ManyToOne => write!(f, "N:1"),
            Cardinality::ManyToMany => write!(f, "N:M"),
        }
    }
}

// Options
// ---------------------------------------------------------------------------

/// What the model export draws, and how.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelOptions {
    pub format: ModelFormat,
    /// Under a header per extension, or one flat list. JSON lists every
    /// entity with its extension either way.
    pub group_by: GroupBy,
    /// Which fields each entity lists.
    pub fields: FieldLevel,
    /// Only the kinds this extension declares (`None`: every extension's).
    pub extension: Option<String>,
    /// Only these kinds (empty: every kind).
    pub kinds: Vec<String>,
    /// Only the kinds near a root kind (`None`: every kind).
    pub root: Option<ModelRoot>,
}

/// The kinds within `depth` hops of `kind`, over the model's relationships
/// in either direction, `kind` included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRoot {
    pub kind: String,
    /// `None`: every kind connected to `kind`.
    pub depth: Option<usize>,
}

// ---------------------------------------------------------------------------
// Core IR types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelIntermediate {
    /// The version of the Graph Protocol schema the model draws, as
    /// `specforge export` and `specforge schema` state it for the same project.
    pub schema_version: String,
    pub extensions: Vec<ModelExtension>,
    pub entities: Vec<ModelEntity>,
    pub relationships: Vec<ModelRelationship>,
    /// Maps edge label -> declaring extension name, whole whatever the selection: an ownership map,
    /// not a selection. [`ModelIntermediate::recount`] counts an extension's drawn edge types from
    /// it. Not serialized to output.
    #[serde(skip)]
    pub edge_type_owners: Vec<(String, String)>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]

pub(crate) struct ModelEntity {
    pub name: String,
    pub extension: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Registry-declared DOT color; renderers fall back to the extension
    /// palette when absent (C13-03).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dot_color: Option<String>,
    pub fields: Vec<ModelField>,
    /// Extensions that contribute fields to this entity via entity enhancements.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enhanced_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelField {
    pub name: String,
    pub field_type: FieldType,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    pub is_primary_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references: Option<String>,
    /// Internal: edge label from SchemaField.edge, used for cardinality inference.
    /// Skipped in serialization.
    #[serde(skip)]
    pub edge_label: Option<String>,
    /// Extension that contributed this field, set only when different from the entity's
    /// owning extension (i.e., the field comes from an entity enhancement).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributed_by: Option<String>,
    /// Contribution info: "EdgeLabel -> target_kind" for reference fields with edges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contribution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelRelationship {
    pub name: String,
    pub source: String,
    pub target: String,
    pub cardinality: Cardinality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelExtension {
    pub name: String,
    pub version: String,
    pub entity_count: usize,
    pub edge_count: usize,
    /// The colour the extension declares for diagrams (`theme_color`). The
    /// schema does not carry it: [`ModelIntermediate::with_theme_colors`]
    /// sets it from the declarations. Not serialized.
    #[serde(skip)]
    pub color: Option<String>,
}

impl ModelIntermediate {
    /// Each extension's declared `theme_color`, from its handshake.
    fn with_theme_colors(mut self, declarations: &[ExtensionDeclaration]) -> Self {
        for ext in &mut self.extensions {
            ext.color = declarations
                .iter()
                .find(|d| d.name() == ext.name)
                .and_then(|d| d.handshake.theme_color.clone());
        }
        self
    }

    /// The colour `extension` is drawn in.
    pub(crate) fn extension_color(&self, extension: &str) -> &str {
        crate::diagram::theme_color(
            self.extensions
                .iter()
                .find(|e| e.name == extension)
                .and_then(|e| e.color.as_deref()),
        )
    }
}

/// The logical data model of `schema`, as `options` asks.
///
/// Every kind of `schema` is one entity, whose first field is a synthetic
/// `id` key. Every edge type joins each of its source kinds to each of its
/// target kinds, with the cardinality the source's field declares (N:M when
/// none does). The export then keeps only the kinds that `options.extension`,
/// `options.kinds` and `options.root` all select. It keeps a relationship
/// when both its kinds are kept, and recounts each extension's entities and
/// edge types. Each kept entity lists the fields `options.fields` names. The
/// model is drawn in `options.format`, grouped by `options.group_by`, and each
/// extension takes the `theme_color` its declaration in `declarations` gives
/// (grey when none does).
///
/// Total: a name `schema` does not have selects nothing. Refusing or
/// reporting such a name is the operation's job (`specforge_ops::model::model`).
pub fn export(
    schema: &GraphProtocolSchema,
    declarations: &[ExtensionDeclaration],
    options: &ModelOptions,
) -> String {
    let model = ModelIntermediate::of(schema)
        .with_theme_colors(declarations)
        .selected(options)
        .with_fields(options.fields);
    match options.format {
        ModelFormat::Markdown => markdown::render_markdown(&model, options),
        ModelFormat::Mermaid => mermaid::render_mermaid(&model, options),
        ModelFormat::Dot => dot::render_dot(&model, options),
        ModelFormat::Json => json::render_json(&model),
        ModelFormat::Dbml => dbml::render_dbml(&model, options),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinality_display_strings() {
        assert_eq!(Cardinality::OneToOne.to_string(), "1:1");
        assert_eq!(Cardinality::OneToMany.to_string(), "1:N");
        assert_eq!(Cardinality::ManyToOne.to_string(), "N:1");
        assert_eq!(Cardinality::ManyToMany.to_string(), "N:M");
    }
}
