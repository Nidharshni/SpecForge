//! E022: a reference to an entity of another kind than its field declares.

use std::collections::HashMap;

use specforge_common::{Diagnostic, codes};

use crate::entity::EntityRecord;
use crate::{FieldRegistry, KindRegistry};

/// Reference list fields whose target entities exist but are of the wrong
/// kind according to the field's declared `target_kind`.
///
/// `entities` is the whole project: a target exists when one of them has
/// its ID. For example, `features [some_behavior_id]` where
/// `some_behavior_id` is a behavior, not a feature, produces E022.
pub(super) fn mistyped(
    entities: &[EntityRecord],
    kinds: &KindRegistry,
    fields: &FieldRegistry,
) -> Vec<Diagnostic> {
    let node_kind_index: HashMap<&str, &str> = entities
        .iter()
        .map(|e| (e.id.as_str(), e.kind.as_str()))
        .collect();
    let mut diagnostics = Vec::new();

    for entity in entities {
        let (entity_kind, entity_id, span) =
            (entity.kind.as_str(), entity.id.as_str(), &entity.span);
        // Skip entities whose kind is not registered (already E024)
        if !kinds.contains(entity_kind) {
            continue;
        }

        for (field_name, target_ids) in &entity.references {
            // Look up the field's target_kind constraint
            let expected_kind = match fields.get(entity_kind, field_name) {
                Some(entry) => match &entry.declared().target_kind {
                    Some(tk) => tk.as_str(),
                    None => continue, // No constraint — any kind is valid
                },
                None => continue, // Unknown field — already W020
            };

            for target_id in target_ids {
                // Only check targets that exist in the graph (missing = E003)
                if let Some(&actual_kind) = node_kind_index.get(target_id.as_str())
                    && actual_kind != expected_kind
                {
                    diagnostics.push(
                        Diagnostic::new(
                            codes::E022,
                            format!(
                                "reference '{}' in field '{}' of {} '{}' targets a {}, but this field expects {}",
                                target_id,
                                field_name,
                                entity_kind,
                                entity_id,
                                actual_kind,
                                expected_kind
                            ),
                        )
                        .with_span(span.clone()),
                    );
                }
            }
        }
    }

    diagnostics
}
