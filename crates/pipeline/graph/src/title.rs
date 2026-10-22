//! Whether a block owes a title and has none: the one answer E076, the migration's titles step and its judge, and
//! the typed create and retitle ask (ADR 0148, ADR 0225).

use crate::build::is_define_block;
use crate::defaults::is_defaults_block;
use specforge_common::{Diagnostic, codes};
use specforge_parser::Entity;

/// Whether `title` names nothing: absent, or only whitespace. A blank title is no title.
pub fn is_untitled(title: Option<&str>) -> bool {
    title.is_none_or(|t| t.trim().is_empty())
}

/// Whether `entity` owes a title it does not have (E076): a block that becomes a node, untitled.
pub fn owes_title(entity: &Entity) -> bool {
    becomes_node(entity) && is_untitled(entity.title.as_deref())
}

/// Whether a block becomes a node: every one but `define` (W143) and `defaults` (E078).
fn becomes_node(entity: &Entity) -> bool {
    !is_define_block(entity) && !is_defaults_block(entity)
}

/// E076: a block with no title, or a blank one.
pub(crate) fn untitled_block_error(entity: &Entity) -> Diagnostic {
    Diagnostic::new(
        codes::E076,
        format!("{} '{}' has no title", entity.kind.raw, entity.id.raw),
    )
    .with_span(entity.span.clone())
    .with_suggestion(format!(
        "write one after its id: {} {} \"…\", or run `specforge migrate`, which takes it from the comment above the block",
        entity.kind.raw, entity.id.raw
    ))
}
