//! What the graph build reads of the loaded declarations (ADR 0032,
//! amended): derived here, from the declarations and the field registry,
//! and nowhere else. The grammar parses values generically; the field
//! registry says what a field's value must be and which fields are edges.

use std::collections::{BTreeMap, BTreeSet};

use specforge_graph::{DerivedFrom, DerivedReference, FieldForms, GraphInputs, Target};

use crate::{FieldRegistry, KindRegistry};

pub(super) fn derive(
    fields: &FieldRegistry,
    forms: &FieldForms,
    owner_of: impl Fn(&str) -> Option<String>,
) -> GraphInputs {
    GraphInputs {
        bidirectional_pairs: bidirectional_pairs(fields),
        target_owners: target_owners(forms, owner_of),
        field_forms: forms.clone(),
        derived_references: derived_references(fields),
    }
}

/// For each target kind no loaded extension declares, the extension that owns it (`owner_of`, the
/// lookup E024 reads), so I004 and E022 name the extension E024 names (ADR 0117). A kind nothing
/// owns has no entry.
fn target_owners(
    forms: &FieldForms,
    owner_of: impl Fn(&str) -> Option<String>,
) -> BTreeMap<String, String> {
    forms
        .targets()
        .into_iter()
        .filter_map(|(_, target)| match target {
            Target::Absent(kind) => Some((kind.to_string(), owner_of(kind)?)),
            Target::Loaded(_) => None,
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
    let declared = kinds.iter().map(|(name, _)| name.clone()).collect();
    FieldForms::new(forms, member_kinds, declared)
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
