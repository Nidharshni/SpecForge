//! E013 and E014: the identifier contract of entity IDs.

use std::collections::BTreeSet;

use specforge_common::{Diagnostic, DiagnosticData, codes, structural};

use crate::KindRegistry;
use crate::entity::EntityRecord;

/// Reserved words that cannot be used as entity identifiers (E013): the
/// structural grammar keywords plus every extension-declared entity kind.
/// An ID equal to a keyword makes `refs [behavior]`-style entries ambiguous
/// with the block introducer itself.
fn reserved_words(kinds: &KindRegistry) -> BTreeSet<String> {
    let mut reserved: BTreeSet<String> =
        structural::KEYWORDS.iter().map(|s| s.to_string()).collect();
    for keyword in kinds.keywords() {
        reserved.insert(keyword.to_string());
    }
    reserved
}

/// E013: entity IDs that collide with reserved words. Documented in
/// entity-model.md ("Reserved Words") long before it was enforced.
pub(super) fn reserved(entities: &[EntityRecord], kinds: &KindRegistry) -> Vec<Diagnostic> {
    let reserved = reserved_words(kinds);
    let mut diagnostics = Vec::new();
    for entity in entities {
        let (id, span) = (entity.id.as_str(), &entity.span);
        if !reserved.contains(id) {
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

/// E014: identifier length contract (2-60 chars) — the documented naming
/// convention (the grammar terminal accepts 1+).
pub(super) fn length(entities: &[EntityRecord]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for entity in entities {
        let (id, span) = (entity.id.as_str(), &entity.span);
        let len = id.chars().count();
        if (2..=60).contains(&len) {
            continue;
        }
        diagnostics.push(
            Diagnostic::new(
                codes::E014,
                format!(
                    "entity ID '{}' violates the identifier length contract (2-60 chars, got {len}) at {}",
                    id, span.file
                ),
            )
            .with_span(span.clone())
            .with_suggestion(
                "pick a descriptive identifier between 2 and 60 characters".to_string(),
            ),
        );
    }
    diagnostics
}
