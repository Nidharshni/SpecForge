//! E024: an entity of a kind no loaded extension declares; and W151, the
//! notice that nothing checks the kinds at all.

use std::collections::BTreeSet;

use specforge_common::{Diagnostic, codes, structural};

use crate::entity::EntityRecord;
use crate::{CandidateVocabulary, KindRegistry};

/// E024 for every entity whose kind is neither structural (`spec`, `ref`,
/// `use`, `define`) nor registered, with the extension that would declare
/// it as the suggestion when the candidate vocabulary knows one.
pub(super) fn unknown(
    entities: &[EntityRecord],
    kinds: &KindRegistry,
    candidates: &CandidateVocabulary,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for entity in entities {
        let (keyword, id, span) = (entity.kind.as_str(), entity.id.as_str(), &entity.span);
        if structural::is_structural(keyword) || kinds.contains(keyword) {
            continue;
        }

        let suggestion = Some(match candidates.kind_owner(keyword) {
            Some(ext) => format!("install it with: specforge add {ext}"),
            None => {
                format!("search for an extension that provides it with: specforge search {keyword}")
            }
        });

        let mut diagnostic = Diagnostic::new(
            codes::E024,
            format!(
                "unknown entity kind '{}' for entity '{}' at {}",
                keyword, id, span.file
            ),
        )
        .with_span(span.clone());
        diagnostic.suggestion = suggestion;
        diagnostics.push(diagnostic);
    }

    diagnostics
}

/// The most kinds W151 names before it says there are more.
const KINDS_NAMED: usize = 5;

/// W151: no loaded extension declares an entity kind (`loaded`: some
/// extension is loaded, so I002 does not say so), yet the project writes
/// entities that are not structural; the kind, field and identifier checks
/// do not run on them. One diagnostic names how many and of which kinds
/// (sorted, unique, at most five), with the extension that declares the first
/// kind as the suggestion when the candidate vocabulary knows one. It has
/// no span: it is about the project, like I002.
pub(super) fn unchecked(
    entities: &[EntityRecord],
    loaded: bool,
    candidates: &CandidateVocabulary,
) -> Vec<Diagnostic> {
    if !loaded {
        return Vec::new();
    }
    let unchecked: Vec<&EntityRecord> = entities
        .iter()
        .filter(|record| !structural::is_structural(&record.kind))
        .collect();
    let kinds: BTreeSet<&str> = unchecked
        .iter()
        .map(|record| record.kind.as_str())
        .collect();
    let Some(first) = kinds.iter().next().copied() else {
        return Vec::new();
    };
    let named: Vec<&str> = kinds.iter().take(KINDS_NAMED).copied().collect();
    let mut listed = named.join(", ");
    if kinds.len() > KINDS_NAMED {
        listed.push_str(", …");
    }
    let (count, noun, verb) = match unchecked.len() {
        1 => (1, "entity", "is"),
        n => (n, "entities", "are"),
    };
    let suggestion = match candidates.kind_owner(first) {
        Some(ext) => format!("install it with: specforge add {ext}"),
        None => format!(
            "enable the extension that declares them; search with: specforge search {first}"
        ),
    };
    vec![
        Diagnostic::new(
            codes::W151,
            format!(
                "the loaded extensions declare no entity kind: {count} {noun} (kinds: {listed}) {verb} not checked against kinds, fields or identifiers"
            ),
        )
        .with_suggestion(suggestion),
    ]
}
