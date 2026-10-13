use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_graph::field_form::{FieldJson, write_fields};
use specforge_graph::{Graph, Node};
use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::budget::TokenBudget;
use crate::error::EmitterError;
use crate::schema::{GraphProtocolSchema, SchemaRefBlock, SchemaVersion};

/// The envelope version an export declares: 1.0 for the schemaless graph
/// export, 2.0 once a schema is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
pub(crate) enum ExportVersion {
    #[serde(rename = "1.0")]
    V1,
    #[serde(rename = "2.0")]
    V2,
}

/// The envelope every export is written in: the graph's entities as `N` and
/// its edges, with the format and schema versions, the schema (embedded, or a
/// reference to it) and the `token_budget` block when they apply. Field order
/// is the wire order.
#[derive(Serialize, Shape)]
pub(crate) struct Export<'a, N: Serialize> {
    /// "1.0" for the schemaless graph format, "2.0" once a schema is
    /// attached; context and brief without a schema carry none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_version: Option<ExportVersion>,
    pub schema_version: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<&'a GraphProtocolSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<SchemaRefBlock>,
    pub nodes: Vec<N>,
    pub edges: Vec<JsonEdge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<&'a TokenBudget>,
}

impl<'a, N: Serialize> Export<'a, N> {
    /// `graph`'s edges and `nodes`, in `format_version` with no schema and no
    /// budget block.
    pub(crate) fn plain(
        format_version: Option<ExportVersion>,
        schema_version: &SchemaVersion,
        graph: &Graph,
        nodes: Vec<N>,
    ) -> Self {
        Export {
            format_version,
            schema_version: Cow::Owned(schema_version.to_string()),
            schema: None,
            schema_ref: None,
            nodes,
            edges: sorted_edges(graph),
            token_budget: None,
        }
    }

    pub(crate) fn to_json(&self) -> Result<String, EmitterError> {
        serde_json::to_string(self).map_err(|e| EmitterError::Serialization(e.to_string()))
    }
}

#[derive(Serialize, Shape)]
pub(crate) struct JsonNode {
    id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    file: String,
    line: usize,
    fields: BTreeMap<String, FieldJson>,
}

/// `n` as the graph export writes an entity.
pub(crate) fn graph_node(n: &Node) -> JsonNode {
    JsonNode {
        id: n.id.raw.to_string(),
        kind: n.kind.raw.to_string(),
        title: n.title.clone(),
        file: n.source_span.file.to_string(),
        line: n.source_span.start_line,
        fields: write_fields(&n.fields),
    }
}

#[derive(Serialize, Shape)]
#[shape(define = "Edge")]
pub(crate) struct JsonEdge {
    pub source: String,
    pub target: String,
    pub label: String,
}

pub(crate) fn sorted_edges(graph: &Graph) -> Vec<JsonEdge> {
    let mut edges: Vec<JsonEdge> = graph
        .edges()
        .iter()
        .map(|e| JsonEdge {
            source: e.source.to_string(),
            target: e.target.to_string(),
            label: e.label.to_string(),
        })
        .collect();
    edges.sort_by(|a, b| (&a.source, &a.target, &a.label).cmp(&(&b.source, &b.target, &b.label)));
    edges
}

/// The graph as a plain (format 1.0) export against the empty schema: the
/// test helper for [`emit`] with the default options.
pub fn emit_json(graph: &Graph) -> String {
    crate::emit::emit(graph, &crate::emit::EmitOptions::default())
        .expect("graph serialization cannot fail")
}
