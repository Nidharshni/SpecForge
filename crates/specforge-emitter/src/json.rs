use serde::Serialize;
use serde_json::Value;
use specforge_common::shape::Shape;
use specforge_graph::{FieldMap, FieldValue, Graph, Node};
use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::budget::TokenBudget;
use crate::error::EmitterError;
use crate::schema::{GraphProtocolSchema, SchemaRefBlock};

/// V1 export envelope version. V1 is a frozen legacy shape; new consumers
/// should use the V2 schema-embedded export.
pub const SCHEMA_VERSION: &str = "0.1.0";

/// The envelope every export is written in: the graph's entities as `N` and
/// its edges, with the format and schema versions, the schema (embedded, or a
/// reference to it) and the `token_budget` block when they apply. Field order
/// is the wire order.
#[derive(Serialize, Shape)]
pub(crate) struct Export<'a, N: Serialize> {
    /// "1.0" for the schemaless graph format, "2.0" once a schema is
    /// attached; context and brief without a schema carry none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_version: Option<&'static str>,
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
        format_version: Option<&'static str>,
        graph: &Graph,
        nodes: Vec<N>,
    ) -> Self {
        Export {
            format_version,
            schema_version: Cow::Borrowed(SCHEMA_VERSION),
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
    fields: BTreeMap<String, Value>,
}

/// `n` as the graph export writes an entity.
pub(crate) fn graph_node(n: &Node) -> JsonNode {
    JsonNode {
        id: n.id.raw.to_string(),
        kind: n.kind.raw.to_string(),
        title: n.title.clone(),
        file: n.source_span.file.to_string(),
        line: n.source_span.start_line,
        fields: field_map_to_json(&n.fields),
    }
}

#[derive(Serialize, Shape)]
#[shape(define = "Edge")]
pub(crate) struct JsonEdge {
    pub source: String,
    pub target: String,
    pub label: String,
}

/// Every field of an entity as plain JSON, keyed by field name.
pub fn field_map_to_json(fields: &FieldMap) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    for entry in fields.entries() {
        map.insert(entry.key.to_string(), field_value_to_json(&entry.value));
    }
    map
}

/// One field value as plain JSON: text as a string, lists as arrays.
pub fn field_value_to_json(value: &FieldValue) -> Value {
    match value {
        FieldValue::String(s) => Value::String(s.clone()),
        FieldValue::Identifier(s) => Value::String(s.clone()),
        FieldValue::TypeUnion(types) => Value::String(types.join(" | ")),
        FieldValue::Expression(exprs) => Value::Array(
            exprs
                .iter()
                .map(|e| serde_json::to_value(e).unwrap_or(Value::String(e.to_string())))
                .collect(),
        ),
        FieldValue::Integer(n) => Value::Number((*n).into()),
        FieldValue::Boolean(b) => Value::Bool(*b),
        FieldValue::Date(s) => Value::String(s.clone()),
        FieldValue::ReferenceList(refs) => {
            Value::Array(refs.iter().map(|r| Value::String(r.id.clone())).collect())
        }
        FieldValue::VariantList(variants) => Value::Array(
            variants
                .iter()
                .map(|v: &String| Value::String(v.clone()))
                .collect(),
        ),
        FieldValue::StringList(list) => Value::Array(
            list.iter()
                .map(|s: &String| Value::String(s.clone()))
                .collect(),
        ),
        FieldValue::VerifyList(stmts) => Value::Array(
            stmts
                .iter()
                .map(|v| {
                    serde_json::json!({
                        "kind": v.kind,
                        "description": v.description,
                    })
                })
                .collect(),
        ),
        FieldValue::Block(inner) => Value::Object(field_map_to_json(inner).into_iter().collect()),
        FieldValue::MixedList(items) => {
            Value::Array(items.iter().map(field_value_to_json).collect())
        }
    }
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

pub fn emit_json(graph: &Graph) -> String {
    let nodes = graph.nodes().into_iter().map(graph_node).collect();
    Export::plain(Some("1.0"), graph, nodes)
        .to_json()
        .expect("graph serialization cannot fail")
}

/// The obligations as the exports write them (`[{kind, description}]`), or
/// `None` when the entity declares none.
pub(crate) fn obligations_json(node: &Node) -> Option<Value> {
    let stmts = specforge_graph::obligations(node);
    (!stmts.is_empty()).then(|| {
        Value::Array(
            stmts
                .iter()
                .map(|s| serde_json::json!({"kind": s.kind, "description": s.description}))
                .collect(),
        )
    })
}
