//! One graph node as its record, and the edge type of one edge (ADR 0019).

use std::collections::{BTreeMap, HashMap, HashSet};

use specforge_graph::{Graph, Node, ValueShape};
use specforge_parser::{FieldValue, UNION_VARIANTS_FIELD};

use super::text::{field_items, field_text};
use crate::compilation::RegistryBuild;
use crate::entity::{
    EdgeCounts, EntityRecord, Exemption, FieldRecord, MethodRecord, ObligationRecord, ParamRecord,
};
use crate::registries::{FieldRegistry, KindRegistry};

/// The edge type of the edge the field `label` writes from the entity
/// `source`, when that field writes one (`EdgeRegistry::written_by`, on the
/// kind `source` is). The one place a graph edge is typed.
pub(super) fn edge_type_of(
    kind_of: &HashMap<&str, &str>,
    registries: &RegistryBuild,
    source: &str,
    label: &str,
) -> Option<String> {
    let kind = kind_of.get(source)?;
    let entry = registries.edges().written_by(kind, label)?;
    Some(entry.label().to_string())
}

/// One node as its record: what it writes (as field text), its
/// obligations, edge counts by edge type, methods, and what exempts it; and,
/// beside it, the ids it references (unique, in written order, resolved or
/// not): only the custom validators' context reads them (E022 is the
/// linker's, ADR 0117).
pub(super) fn record(
    graph: &Graph,
    kind_of: &HashMap<&str, &str>,
    node: &Node,
    registries: &RegistryBuild,
) -> (EntityRecord, Vec<String>) {
    let id = node.id.raw.as_str();
    // Edges in one direction, in total and by edge type.
    let counts = |types: Vec<Option<String>>| {
        let mut by_type = BTreeMap::new();
        for edge_type in types.iter().flatten() {
            *by_type.entry(edge_type.clone()).or_insert(0) += 1;
        }
        EdgeCounts {
            total: types.len(),
            by_type,
        }
    };
    let incoming = counts(
        graph
            .edges_to(id)
            .iter()
            .map(|e| edge_type_of(kind_of, registries, e.source.as_str(), e.label.as_str()))
            .collect(),
    );
    let outgoing = counts(
        graph
            .edges_from(id)
            .iter()
            .map(|e| edge_type_of(kind_of, registries, id, e.label.as_str()))
            .collect(),
    );
    let entries = node.fields.entries();
    let mut seen = HashSet::new();
    let referenced: Vec<String> = entries
        .iter()
        .flat_map(|entry| {
            registries
                .field_forms()
                .reading(node.kind.raw.as_str(), entry.key.as_str())
                .references(entry)
        })
        .filter(|reference| seen.insert(reference.id))
        .map(|reference| reference.id.to_string())
        .collect();
    let record = EntityRecord {
        id: id.to_string(),
        kind: node.kind.raw.to_string(),
        span: node.source_span.clone(),
        fields: entries
            .iter()
            .map(|entry| FieldRecord {
                key: entry.key.to_string(),
                text: field_text(&entry.value),
                annotations: entry
                    .annotations
                    .iter()
                    .map(|a| a.name.to_string())
                    .collect(),
                items: field_items(&entry.value),
                shape: ValueShape::of(&entry.value),
                value_span: entry.value_span.clone(),
                rewrites: registries.field_forms().rewrites(
                    node.kind.raw.as_str(),
                    entry,
                    &node.source_span,
                ),
            })
            .collect(),
        obligations: node
            .obligations
            .iter()
            .map(|statement| ObligationRecord {
                kind: statement.kind.clone(),
                text: statement.description.clone(),
            })
            .collect(),
        incoming,
        outgoing,
        exemption: exemption(node, registries.kinds(), registries.fields()),
        methods: node
            .methods
            .iter()
            .map(|m| MethodRecord {
                name: m.name.clone(),
                params: m
                    .params
                    .iter()
                    .map(|p| ParamRecord {
                        name: p.name.clone(),
                        ty: p.ty.clone(),
                    })
                    .collect(),
                returns: m.returns.clone(),
            })
            .collect(),
    };
    (record, referenced)
}

/// What exempts an entity from obligations of its own, whatever it declares
/// (ADR 0004, D2-b; ADR 0019), in this order: a union (`type X = A | B`,
/// which has no body to hold them); a set field its kind's registry entry
/// declares `exempts_obligations` (as `@specforge/formal` declares
/// `abstract true`); a kind that accepts no `verify` statements
/// (`supports_verify` unset), which has nowhere to declare them. Decided
/// from the entity's structure and the registries, never from field names:
/// a struct member that only happens to be named `abstract` exempts
/// nothing. A kind no extension declares is not known to refuse `verify`,
/// so it is not exempt for that.
fn exemption(node: &Node, kinds: &KindRegistry, fields: &FieldRegistry) -> Option<Exemption> {
    let kind = node.kind.raw.as_str();
    let entries = node.fields.entries();
    // The union syntax is structural: its body is the variant list, under
    // the parser's own key (a user's `values [a, b]` is a list, and exempts
    // nothing).
    let union = entries.iter().any(|entry| {
        matches!(&entry.value, FieldValue::VariantList(variants)
            if entry.key.as_str() == UNION_VARIANTS_FIELD && !variants.is_empty())
    });
    if union {
        return Some(Exemption::Union);
    }
    let flag = entries.iter().find(|entry| {
        is_set(&entry.value)
            && fields
                .get(kind, entry.key.as_str())
                .is_some_and(|f| f.declared().exempts_obligations)
    });
    if let Some(entry) = flag {
        return Some(Exemption::Flag {
            field: entry.key.to_string(),
        });
    }
    kinds
        .get(kind)
        .is_some_and(|entry| !entry.supports_verify)
        .then_some(Exemption::NoVerify)
}

/// A field value that turns an exempting flag on: `true`, or any value
/// that is not empty.
fn is_set(value: &FieldValue) -> bool {
    match value {
        FieldValue::Boolean(b) => *b,
        FieldValue::String(s) | FieldValue::Identifier(s) => !s.is_empty(),
        FieldValue::List(items) => !items.is_empty(),
        _ => false,
    }
}
