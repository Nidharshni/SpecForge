//! W020 and E079: a field no extension declares on the entity's kind.

use std::collections::{BTreeSet, HashMap};

use specforge_common::{Diagnostic, codes};

use crate::entity::{EntityRecord, FieldRecord};
use crate::{CandidateVocabulary, FieldRegistry, KindRegistry};

/// W020 for each field name not declared on a registered kind, or E079 when
/// the name is the other side of a relation another kind owns (ADR 0150): an
/// `inverse_of` names it, or the ids it lists are all of one other kind that
/// owns a reference field into this kind. `title` is
/// structural and always valid; every other name, `expression` included, is
/// valid where an extension declares it (the prove pass reads the fields
/// extensions give a proof role, ADR 0009). A field named `verify` is
/// checked like any field: `verify` statements are not fields (ADR 0094; a
/// kind that accepts none is W158). Entities with unregistered kinds are
/// skipped to avoid cascading diagnostics.
pub(super) fn unknown(
    entities: &[EntityRecord],
    kinds: &KindRegistry,
    fields: &FieldRegistry,
    candidates: &CandidateVocabulary,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let kind_of: HashMap<&str, &str> = entities
        .iter()
        .map(|entity| (entity.id.as_str(), entity.kind.as_str()))
        .collect();

    for entity in entities {
        let (kind, id, span) = (entity.kind.as_str(), entity.id.as_str(), &entity.span);
        // Skip entities with unregistered kinds — already E024
        let Some(entry) = kinds.get(kind) else {
            continue;
        };

        // Skip entities with open_fields — any field name is valid (e.g., type struct fields, port methods)
        if entry.declared.open_fields {
            continue;
        }

        for field in &entity.fields {
            let field_name = field.key.as_str();
            if field_name == "title" || fields.contains(kind, field_name) {
                continue;
            }
            let listed = listed_ids(field);
            let value_kinds: BTreeSet<&str> = listed
                .iter()
                .filter_map(|listed| kind_of.get(listed.as_str()).copied())
                .collect();
            let value_kind = match value_kinds.len() {
                1 => value_kinds.first().copied(),
                _ => None,
            };
            let owners = fields.owning_fields(kind, field_name, value_kind);
            if !owners.is_empty() {
                let mut diagnostic = Diagnostic::new(
                    codes::E079,
                    format!(
                        "field '{field_name}' on {kind} '{id}' is the other side of {}: a relation is written on its owning side only",
                        join(owners.iter().map(|o| format!("{}.{}", o.kind_name(), o.name())))
                    ),
                )
                .with_span(field.value_span.clone().unwrap_or_else(|| span.clone()));
                diagnostic.suggestion = Some(format!(
                    "remove '{field_name}' here and write {}",
                    join(owners.iter().map(|owner| {
                        let owner_kind = owner.kind_name();
                        let on: Vec<String> = listed
                            .iter()
                            .filter(|l| kind_of.get(l.as_str()) == Some(&owner_kind))
                            .map(|l| format!("'{l}'"))
                            .collect();
                        let on = if on.is_empty() {
                            format!("each {owner_kind} it lists")
                        } else {
                            format!("{owner_kind} {}", on.join(", "))
                        };
                        format!("'{} [{id}]' on {on}", owner.name())
                    }))
                ));
                diagnostics.push(diagnostic);
                continue;
            }
            let suggestion = candidates.field_owner(kind, field_name).map(|ext| {
                format!(
                    "{ext} declares '{field_name}' on '{kind}': install it with: specforge add {ext}"
                )
            });
            let mut diagnostic = Diagnostic::new(
                codes::W020,
                format!(
                    "unrecognized field '{}' on entity '{}' of kind '{}' at {}",
                    field_name, id, kind, span.file
                ),
            )
            .with_span(span.clone());
            diagnostic.suggestion = suggestion;
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}

/// The ids a field's value lists: a list's items, or a single value's text.
fn listed_ids(field: &FieldRecord) -> Vec<String> {
    match &field.items {
        Some(items) => items.clone(),
        None => vec![field.text.trim().to_string()],
    }
}

fn join(parts: impl Iterator<Item = String>) -> String {
    parts.collect::<Vec<_>>().join(" or ")
}
