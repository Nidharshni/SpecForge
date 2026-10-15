//! W020: a field no extension declares on the entity's kind.

use specforge_common::{Diagnostic, codes};

use super::index::KeywordExtensionIndex;
use crate::entity::EntityRecord;
use crate::{FieldRegistry, KindRegistry};

/// W020 for each field name not declared on a registered kind. `title` is
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
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

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

        for field_name in entity.field_keys() {
            if field_name == "title" || fields.contains(kind, field_name) {
                continue;
            }
            let suggestion = KeywordExtensionIndex::bundled_fields()
                    .lookup(&format!("{kind}.{field_name}"))
                    .map(|ext| {
                        format!(
                            "{ext} declares '{field_name}' on '{kind}': install it with: specforge add {ext}"
                        )
                    })
;
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
