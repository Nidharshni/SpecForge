//! What the graph build reads of the loaded declarations (ADR 0032,
//! amended): derived here, from the declarations and the field registry,
//! and nowhere else. The grammar parses values generically; the field
//! registry says what a field's value must be and which fields are edges.

use std::collections::{BTreeSet, HashMap, HashSet};

use specforge_graph::{DerivedFrom, DerivedReference, FieldCoercion, GraphInputs};

use crate::{FieldRegistry, FieldType, KindRegistry};

pub(super) fn derive(kinds: &KindRegistry, fields: &FieldRegistry) -> GraphInputs {
    GraphInputs {
        bidirectional_pairs: bidirectional_pairs(fields),
        single_reference_fields: single_reference_fields(kinds, fields),
        absent_reference_targets: absent_reference_targets(kinds, fields),
        field_coercions: field_coercions(fields),
        derived_references: derived_references(fields),
    }
}

/// Every registered `reference` field, while some kind is registered.
fn single_reference_fields(
    kinds: &KindRegistry,
    fields: &FieldRegistry,
) -> HashSet<(String, String)> {
    if kinds.is_empty() {
        return HashSet::new();
    }
    fields
        .iter()
        .filter(|(_, _, entry)| entry.field_type() == FieldType::Reference)
        .map(|(kind, field, _)| (kind.to_string(), field.to_string()))
        .collect()
}

/// Reference fields whose target kind `kinds` doesn't declare, as
/// (kind, field) -> target kind: the kind's extension isn't enabled
/// (e.g. software's `behavior.features` without @specforge/product).
fn absent_reference_targets(
    kinds: &KindRegistry,
    fields: &FieldRegistry,
) -> HashMap<(String, String), String> {
    fields
        .iter()
        .filter_map(|(kind, field, entry)| {
            let target = entry.declared().target_kind.as_ref()?;
            (!kinds.contains(target))
                .then(|| ((kind.to_string(), field.to_string()), target.clone()))
        })
        .collect()
}

/// Every pair of a reference field and one of its inverse fields
/// ([`FieldRegistry::inverse_fields`]: fields of its target kind), written
/// (lesser, greater), once each, sorted.
fn bidirectional_pairs(fields: &FieldRegistry) -> Vec<(String, String)> {
    let pairs: BTreeSet<(String, String)> = fields
        .iter()
        .filter(|(_, _, entry)| entry.field_type().is_reference())
        .flat_map(|(kind, field, _)| {
            fields
                .inverse_fields(kind, field)
                .into_iter()
                .map(move |inverse| (field, inverse))
        })
        .map(|(field, inverse)| {
            if field <= inverse {
                (field.to_string(), inverse.to_string())
            } else {
                (inverse.to_string(), field.to_string())
            }
        })
        .collect();
    pairs.into_iter().collect()
}

/// How each registered field's value is coerced: a list type to a list,
/// integer, bool, string or enum to that type; a single reference or a
/// block is read as written. The values that still can't be the declared
/// type are `RegistryBuild::check`'s E061.
fn field_coercions(fields: &FieldRegistry) -> HashMap<(String, String), FieldCoercion> {
    fields
        .iter()
        .filter_map(|(kind, field, entry)| {
            let coercion = match entry.field_type() {
                t if t.is_list() => FieldCoercion::List,
                FieldType::Integer => FieldCoercion::Integer,
                FieldType::Bool => FieldCoercion::Bool,
                FieldType::String | FieldType::Enum => FieldCoercion::Text,
                // A single reference or a block is read as written.
                _ => return None,
            };
            Some(((kind.to_string(), field.to_string()), coercion))
        })
        .collect()
}

/// The registered reference fields whose edges the host derives
/// (`derived_from`). A field that names an unknown source or no target
/// kind derives nothing (the declaration check reports it as W021).
/// Sorted, so the build is deterministic.
fn derived_references(fields: &FieldRegistry) -> Vec<DerivedReference> {
    let mut derived: Vec<DerivedReference> = fields
        .iter()
        .filter(|(_, _, entry)| entry.field_type().is_reference())
        .filter_map(|(kind, field, entry)| {
            Some(DerivedReference {
                source_kind: kind.to_string(),
                field: field.to_string(),
                target_kind: entry.declared().target_kind.clone()?,
                from: DerivedFrom::parse(entry.declared().derived_from.as_deref()?)?,
            })
        })
        .collect();
    derived.sort_by(|a, b| (&a.source_kind, &a.field).cmp(&(&b.source_kind, &b.field)));
    derived
}
