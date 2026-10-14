//! W012: a `ref` nothing references.

use specforge_common::{Diagnostic, codes, structural};

use crate::entity::EntityRecord;

/// W012 for every `ref` entity with no incoming edge. Only `ref`s are
/// checked: `spec` is the project's root container, which nothing
/// references and which is never unreferenced. Extension kinds that want
/// unreferenced detection declare a `no_incoming_edges` rule.
pub(super) fn unreferenced(entities: &[EntityRecord]) -> Vec<Diagnostic> {
    entities
        .iter()
        .filter(|record| record.kind == structural::REF && record.incoming.total == 0)
        .map(|record| {
            Diagnostic::new(
                codes::W012,
                format!(
                    "unreferenced {} '{}' has no incoming edges",
                    record.kind, record.id
                ),
            )
            .with_span(record.span.clone())
        })
        .collect()
}
