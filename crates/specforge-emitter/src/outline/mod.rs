//! The extension outline: one card per loaded extension (its kinds, edge
//! types, rules and contributions), the dependencies between extensions,
//! the enhancements and the cross-extension edges. One call, [`export`].
//! How the outline is built and drawn is this module's own (ADR 0007).

use specforge_protocol_types::ExtensionDeclaration;

mod build;
mod dot;
mod json;
mod markdown;
mod mermaid;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutlineFormat {
    #[default]
    Markdown,
    Mermaid,
    Dot,
    Json,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutlineDetail {
    None,
    #[default]
    Keys,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DependencyKind {
    /// Explicitly declared in peerDependencies
    Direct,
    /// Transitive AND the extension references kinds from the target
    Effective,
    /// Pure transitive — no direct kind references
    Transitive,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyDepth {
    /// Only explicitly declared peer_dependencies
    #[default]
    Direct,
    /// Direct + transitive deps that reference kinds from the target extension
    Effective,
    /// All transitive deps, including unused
    Full,
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct OutlineOptions {
    pub format: OutlineFormat,
    pub detail: OutlineDetail,
    pub deps: DependencyDepth,
}

// ---------------------------------------------------------------------------
// Core IR types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineIntermediate {
    pub extensions: Vec<OutlineExtension>,
    pub dependencies: Vec<OutlineDependency>,
    pub enhancements: Vec<OutlineEnhancement>,
    pub cross_edges: Vec<OutlineCrossEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineExtension {
    pub name: String,
    pub version: String,
    pub entity_kinds: Vec<OutlineEntityKind>,
    pub edge_types: Vec<OutlineEdgeType>,
    pub validation_rules: Vec<OutlineValidationRule>,
    /// The describe categories it declares, then `providers` when its handshake says so.
    pub contributes: Vec<String>,
    pub verify_kinds: Vec<String>,
    pub surface_counts: OutlineSurfaceCounts,
    pub shared_fields: Vec<OutlineSharedField>,
    pub collector_count: usize,
    /// The colour the extension declares for diagrams (`theme_color`).
    /// Not serialized.
    #[serde(skip)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineEntityKind {
    pub name: String,
    pub keyword: String,
    pub testable: bool,
    pub field_count: usize,
    pub fields: Vec<OutlineField>,
    pub enhanced_by: Vec<OutlineFieldAttribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineField {
    pub name: String,
    pub field_type: String,
    pub required: bool,
    pub source_extension: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineFieldAttribution {
    pub source_extension: String,
    pub field_count: usize,
    pub field_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineEdgeType {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineValidationRule {
    pub code: String,
    pub severity: String,
    pub check: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct OutlineSurfaceCounts {
    pub cli_commands: usize,
    pub mcp_tools: usize,
    pub mcp_resources: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineSharedField {
    pub name: String,
    pub field_type: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineDependency {
    pub from: String,
    pub to: String,
    pub version: String,
    pub optional: bool,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineEnhancement {
    pub enhancer: String,
    pub owner: String,
    pub target_kind: String,
    pub field_count: usize,
    pub field_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineCrossEdge {
    pub edge_label: String,
    pub owner_extension: String,
    pub source_kind: String,
    pub target_kind: String,
    pub target_extension: String,
}

pub(crate) fn filter_dependencies(
    deps: &[OutlineDependency],
    depth: DependencyDepth,
) -> Vec<&OutlineDependency> {
    match depth {
        DependencyDepth::Direct => deps
            .iter()
            .filter(|d| d.kind == DependencyKind::Direct)
            .collect(),
        DependencyDepth::Effective => deps
            .iter()
            .filter(|d| d.kind != DependencyKind::Transitive)
            .collect(),
        DependencyDepth::Full => deps.iter().collect(),
    }
}

/// The extension outline of `declarations`, as `options` asks.
///
/// Each extension is one card, detailed to `options.detail`. The output
/// shows the dependencies `options.deps` selects (in every format, JSON
/// included), the enhancements and the cross-extension edges. It is drawn
/// in `options.format`, each extension in its declared `theme_color`.
pub fn export(declarations: &[ExtensionDeclaration], options: &OutlineOptions) -> String {
    let outline = OutlineIntermediate::of(declarations);
    match options.format {
        OutlineFormat::Markdown => markdown::render_markdown(&outline, options),
        OutlineFormat::Mermaid => mermaid::render_mermaid(&outline, options),
        OutlineFormat::Dot => dot::render_dot(&outline, options),
        OutlineFormat::Json => json::render_json(&outline, options),
    }
}
