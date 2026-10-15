//! W158: `verify` statements on a kind that accepts none.

use specforge_common::{Diagnostic, codes};

use crate::KindRegistry;
use crate::entity::EntityRecord;

/// One W158 for each entity that declares `verify` statements while its registered kind lacks
/// `supports_verify`. `verify` is reserved syntax whose meaning comes from extensions (ADR 0002): a
/// kind accepts statements only once an extension made it testable (for software kinds,
/// `@specforge/testing`). Statements are not fields (ADR 0094), so this runs for `open_fields` kinds
/// too. An unregistered kind is skipped: it is already E024.
pub(super) fn refused(entities: &[EntityRecord], kinds: &KindRegistry) -> Vec<Diagnostic> {
    entities
        .iter()
        .filter(|entity| !entity.obligations.is_empty())
        .filter(|entity| {
            kinds
                .get(entity.kind.as_str())
                .is_some_and(|entry| !entry.supports_verify)
        })
        .map(|entity| {
            let (kind, id, span) = (entity.kind.as_str(), entity.id.as_str(), &entity.span);
            let mut diagnostic = Diagnostic::new(
                codes::W158,
                format!(
                    "entity '{id}' of kind '{kind}' declares verify statements, which its kind does not accept"
                ),
            )
            .with_span(span.clone());
            diagnostic.suggestion = Some(format!(
                "'{kind}' accepts no verify obligations: enable an extension that makes it testable (for software kinds, `specforge add @specforge/testing`)"
            ));
            diagnostic
        })
        .collect()
}
