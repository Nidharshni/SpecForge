//! What the graph build reads of the loaded declarations (ADR 0032,
//! amended): derived here, from the declarations and the field registry,
//! and nowhere else. The grammar parses values generically; the field
//! registry says what a field's value must be and which fields are edges.

use std::collections::{BTreeSet, HashMap};

use specforge_graph::{DerivedFrom, DerivedReference, FieldForms, GraphInputs};

use crate::{FieldRegistry, KindRegistry};

pub(super) fn derive(
    kinds: &KindRegistry,
    fields: &FieldRegistry,
    forms: &FieldForms,
) -> GraphInputs {
    GraphInputs {
        bidirectional_pairs: bidirectional_pairs(fields),
        absent_reference_targets: absent_reference_targets(kinds, fields),
        field_forms: forms.clone(),
        derived_references: derived_references(fields),
    }
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

/// Every registered field's form, and the kinds that take any field
/// (`open_fields`): how the graph build stores a written value as the
/// field's declared type, and what each reader takes it to reference. The
/// values that still can't be the declared type are `RegistryBuild::check`'s
/// E061.
pub(super) fn field_forms(kinds: &KindRegistry, fields: &FieldRegistry) -> FieldForms {
    let forms = fields
        .iter()
        .map(|(kind, field, entry)| ((kind.to_string(), field.to_string()), entry.form().clone()))
        .collect();
    let member_kinds = kinds
        .iter()
        .filter(|(_, entry)| entry.declared.open_fields)
        .map(|(name, _)| name.clone())
        .collect();
    FieldForms::new(forms, member_kinds)
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
