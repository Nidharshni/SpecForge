//! E013 and E014: the identifier contract of entity IDs.

use specforge_common::{Diagnostic, DiagnosticData, codes};

use crate::KindRegistry;
use crate::entity::EntityRecord;

/// E013: entity IDs that collide with reserved words. Documented in
/// entity-model.md ("Reserved Words") long before it was enforced.
pub(super) fn reserved(entities: &[EntityRecord], kinds: &KindRegistry) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for entity in entities {
        let (id, span) = (entity.id.as_str(), &entity.span);
        if !kinds.reserves(id) {
            continue;
        }
        diagnostics.push(
            Diagnostic::new(
                codes::E013,
                format!(
                    "entity ID '{}' collides with a reserved keyword at {}",
                    id, span.file
                ),
            )
            .with_span(span.clone())
            .with_suggestion(format!(
                "rename the entity (e.g. `{id}_rule`, `{id}_spec`) — reserved words cannot be identifiers"
            ))
            .with_data(DiagnosticData::ShadowedKeyword {
                keyword: id.to_string(),
            }),
        );
    }
    diagnostics
}

/// E014: the shortest and longest entity IDs (the grammar terminal accepts 1+).
pub const ID_LENGTH: std::ops::RangeInclusive<usize> = 2..=60;

/// E014: identifier length contract ([`ID_LENGTH`] chars) — the documented naming
/// convention.
pub(super) fn length(entities: &[EntityRecord]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for entity in entities {
        let (id, span) = (entity.id.as_str(), &entity.span);
        let len = id.chars().count();
        if ID_LENGTH.contains(&len) {
            continue;
        }
        diagnostics.push(
            Diagnostic::new(
                codes::E014,
                format!(
                    "entity ID '{}' violates the identifier length contract ({}-{} chars, got {len}) at {}",
                    id,
                    ID_LENGTH.start(),
                    ID_LENGTH.end(),
                    span.file
                ),
            )
            .with_span(span.clone())
            .with_suggestion(format!(
                "pick a descriptive identifier between {} and {} characters",
                ID_LENGTH.start(),
                ID_LENGTH.end()
            )),
        );
    }
    diagnostics
}
