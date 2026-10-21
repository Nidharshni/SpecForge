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

/// What `check` refuses of an entity ID (CONTEXT "Entity ID rule"): E014, then E013.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdRefusal {
    /// Outside [`ID_LENGTH`] characters (E014).
    Length,
    /// A structural keyword or the keyword of a kind in `kinds` (E013, [`KindRegistry::reserves`]).
    Reserved,
}

/// Why `check` would refuse `id` as an entity ID in a project whose kinds are `kinds`, or `None`. Every
/// entity is held to it, a spec block's string included; whether `id` is the grammar's identifier is the
/// parser's (`is_identifier`), asked only of an ID written as a token. With no kind registered `check`
/// runs neither part (structural-only), yet the structural keywords are reserved: a caller that writes a
/// starter asks it whatever is enabled.
pub fn refused_id(id: &str, kinds: &KindRegistry) -> Option<IdRefusal> {
    if !ID_LENGTH.contains(&id.chars().count()) {
        Some(IdRefusal::Length)
    } else if kinds.reserves(id) {
        Some(IdRefusal::Reserved)
    } else {
        None
    }
}

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
