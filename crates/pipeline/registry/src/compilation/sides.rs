use std::collections::BTreeMap;

use specforge_common::{Diagnostic, codes};

use crate::FieldRegistry;

/// W162 (ADR 0150): a relation declared from both of its ends. (a) A
/// reference field whose `inverse_of` names a field its target kind
/// registers; (b) two different kinds that each register a reference field
/// targeting the other, reported once per kind pair, lesser kind first.
/// The fields still register; nothing else changes.
pub(super) fn both_sides(fields: &FieldRegistry) -> Vec<Diagnostic> {
    // (kind, field) -> (target kind, extension), reference fields only.
    let mut refs: BTreeMap<(&str, &str), (&str, &str)> = BTreeMap::new();
    for (kind, name, entry) in fields.iter() {
        if let (true, Some(target)) = (
            entry.field_type().is_reference(),
            entry.declared().target_kind.as_deref(),
        ) {
            refs.insert((kind, name), (target, entry.source_extension()));
        }
    }

    let mut diagnostics = Vec::new();
    for (&(kind, name), &(target, extension)) in &refs {
        let Some(inverse) = fields
            .get(kind, name)
            .and_then(|entry| entry.declared().inverse_of.as_deref())
        else {
            continue;
        };
        if fields.contains(target, inverse) {
            diagnostics.push(
                Diagnostic::new(
                    codes::W162,
                    format!(
                        "field '{kind}.{name}' ({extension}) names '{inverse}' its reverse, but {target} declares a field '{inverse}': a relation is written from one side; drop inverse_of or name a word {target} does not use"
                    ),
                )
                .with_suggestion(format!("remove inverse_of(\"{inverse}\") from '{kind}.{name}'")),
            );
        }
    }

    // (b) the first field of each ordered pair, then each unordered pair once.
    let mut first: BTreeMap<(&str, &str), (&str, &str)> = BTreeMap::new();
    for (&(kind, name), &(target, extension)) in &refs {
        if kind != target {
            first.entry((kind, target)).or_insert((name, extension));
        }
    }
    for (&(kind, target), &(name, extension)) in &first {
        if kind < target
            && let Some(&(back, back_extension)) = first.get(&(target, kind))
        {
            diagnostics.push(
                Diagnostic::new(
                    codes::W162,
                    format!(
                        "kinds '{kind}' and '{target}' link each other both ways ('{kind}.{name}' from {extension}, '{target}.{back}' from {back_extension}): a relation is written from one side; remove one field, the graph derives it"
                    ),
                )
                .with_suggestion(format!("remove '{kind}.{name}' or '{target}.{back}'")),
            );
        }
    }
    diagnostics.sort_by(|a, b| a.message.cmp(&b.message));
    diagnostics
}
