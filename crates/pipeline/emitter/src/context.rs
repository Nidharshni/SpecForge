use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_coverage::Status;
use specforge_graph::Obligation;
use specforge_graph::field_form::{FieldJson, write};
use specforge_graph::{FieldValue, Node};
use specforge_registry::FieldRegistry;
use std::collections::BTreeMap;

#[derive(Serialize, Shape)]
pub(crate) struct ContextNode {
    id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    /// The fields an extension declares `headline` (a contract, a status),
    /// by name.
    #[serde(flatten)]
    headline: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verify: Option<Vec<Obligation>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    fields: BTreeMap<String, FieldJson>,
    /// The entity's coverage status, when the export was given verdicts
    /// (`EmitOptions::coverage`): what `specforge query --include-coverage`
    /// shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    coverage_status: Option<Status>,
}

/// Keys a context node has of its own, which no headline field may take.
const NODE_KEYS: [&str; 6] = ["id", "kind", "title", "verify", "fields", "coverage_status"];

/// The fields of `node` its extension declares `headline`, as text, for the
/// context node's top level.
pub(crate) fn headline_fields(node: &Node, registry: &FieldRegistry) -> BTreeMap<String, String> {
    node.fields
        .entries()
        .iter()
        .filter(|entry| is_headline(node, entry.key.as_str(), registry))
        .filter(|entry| !NODE_KEYS.contains(&entry.key.as_str()))
        .filter_map(|entry| match &entry.value {
            FieldValue::String(s) | FieldValue::Identifier(s) => {
                Some((entry.key.to_string(), s.clone()))
            }
            _ => None,
        })
        .collect()
}

/// The text that states what `node` promises, as an agent reads it first:
/// its first string field the extension declares both `headline` and
/// `normative` (a behavior's or an event's `contract`; a `status` is
/// headline but not normative). `None` when its kind declares none.
pub fn headline_statement<'n>(node: &'n Node, registry: &FieldRegistry) -> Option<&'n str> {
    node.fields.entries().iter().find_map(|entry| {
        let declared = registry
            .get(node.kind.raw.as_str(), entry.key.as_str())
            .is_some_and(|f| f.declared().headline && f.declared().normative);
        match &entry.value {
            FieldValue::String(s) if declared => Some(s.as_str()),
            _ => None,
        }
    })
}

fn is_headline(node: &Node, field: &str, registry: &FieldRegistry) -> bool {
    registry
        .get(node.kind.raw.as_str(), field)
        .is_some_and(|f| f.declared().headline)
}

/// The fields of `node` its extension declares normative (the text that
/// states what the entity promises), except those a context node already
/// carries at its top level (its headline fields and obligations).
pub(crate) fn normative_fields(
    node: &Node,
    registry: &FieldRegistry,
) -> BTreeMap<String, FieldJson> {
    node.fields
        .entries()
        .iter()
        .filter(|entry| !is_headline(node, entry.key.as_str(), registry))
        .filter(|entry| {
            registry
                .get(node.kind.raw.as_str(), entry.key.as_str())
                .is_some_and(|field| field.declared().normative)
        })
        .map(|entry| (entry.key.to_string(), write(&entry.value)))
        .collect()
}

/// `n` as the context export writes an entity: its title and obligations, the
/// headline and normative fields `registry` declares.
pub(crate) fn context_node(
    n: &Node,
    registry: &FieldRegistry,
    coverage_status: Option<Status>,
) -> ContextNode {
    ContextNode {
        id: n.id.raw.to_string(),
        kind: n.kind.raw.to_string(),
        title: n.title.clone(),
        headline: headline_fields(n, registry),
        verify: {
            let statements = &n.obligations;
            (!statements.is_empty()).then(|| statements.iter().map(Obligation::of).collect())
        },
        fields: normative_fields(n, registry),
        coverage_status,
    }
}
