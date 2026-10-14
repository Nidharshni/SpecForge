//! W020: a field no extension declares on the entity's kind.

use specforge_common::{Diagnostic, codes};

use super::index::KeywordExtensionIndex;
use crate::entity::EntityRecord;
use crate::{FieldRegistry, KindRegistry};

/// W020 for each field name not declared on a registered kind. `title` is
/// structural and always valid; every other name, `expression` included, is
/// valid where an extension declares it (the prove pass reads the fields
/// extensions give a proof role, ADR 0009). `verify` is reserved syntax
/// whose meaning comes from extensions (ADR 0002): it is valid only on
/// kinds an extension made testable (`supports_verify`), e.g. via
/// @specforge/testing. Entities with unregistered kinds are skipped to avoid
/// cascading diagnostics.
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
            let accepted = match field_name {
                "title" => true,
                "verify" => entry.supports_verify,
                _ => fields.contains(kind, field_name),
            };
            if accepted {
                continue;
            }
            let suggestion = if field_name == "verify" {
                Some(format!(
                    "'{kind}' accepts no verify obligations: enable an extension that makes it testable (for software kinds, `specforge add @specforge/testing`)"
                ))
            } else {
                KeywordExtensionIndex::bundled_fields()
                    .lookup(&format!("{kind}.{field_name}"))
                    .map(|ext| {
                        format!(
                            "{ext} declares '{field_name}' on '{kind}': install it with: specforge add {ext}"
                        )
                    })
            };
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
