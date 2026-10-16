//! Which entities match a text: one ranking for LSP completion, LSP
//! workspace symbols and MCP search.
//!
//! All comparisons are lowercase. A match falls in the first tier it
//! reaches: the text is the id or title ([`Tier::Exact`]), starts it
//! ([`Tier::Prefix`]) or is inside it ([`Tier::Substring`]); for a search
//! over text too, inside a string field ([`Tier::FieldText`]); else the
//! id or title is within the fuzzy threshold ([`Tier::Fuzzy`]: Jaro-Winkler
//! similarity of at least [`FUZZY_THRESHOLD`]). Within a tier the more
//! similar comes first, then the id. Filters (kinds, a field's value, what
//! an entity references) apply before ranking; the limit after.

use specforge_common::Sym;
use specforge_graph::{Graph, Node};
use specforge_parser::FieldValue;
use specforge_registry::snapshot::{field_items, field_text};

/// The least Jaro-Winkler similarity, over the lowercase id or title, at
/// which a text that matches no other way still finds an entity.
pub const FUZZY_THRESHOLD: f64 = 0.80;

/// Whether a similarity is within the fuzzy threshold.
pub fn within_fuzzy_threshold(similarity: f64) -> bool {
    similarity >= FUZZY_THRESHOLD
}

/// What a text is matched against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchScope {
    /// The id and the title: completion and workspace symbols, as an
    /// editor filters by name.
    Names,
    /// The id, the title, then the string fields (a contract, a
    /// guarantee): MCP search, as an agent searches what entities say.
    NamesAndText,
}

/// A lookup: the text, its scope, and the filters every match passes.
#[derive(Clone, Copy, Debug)]
pub struct EntityQuery<'q> {
    pub text: &'q str,
    pub scope: MatchScope,
    /// Only entities of these kinds; empty: every kind.
    pub kinds: &'q [&'q str],
    /// Only entities whose field (the first) holds the value (the
    /// second): its text by the field-text rule (ADR 0019), item by item,
    /// lowercase.
    pub field_contains: Option<(&'q str, &'q str)>,
    /// Only entities that reference this entity (a reference to it in
    /// one of their fields).
    pub referencing: Option<&'q str>,
    /// At most this many matches, the best first.
    pub limit: Option<usize>,
}

impl<'q> EntityQuery<'q> {
    /// `text` over `scope`, unfiltered and unlimited.
    pub fn new(text: &'q str, scope: MatchScope) -> Self {
        EntityQuery {
            text,
            scope,
            kinds: &[],
            field_contains: None,
            referencing: None,
            limit: None,
        }
    }
}

/// How well an entity matches, best first.
#[derive(PartialOrd, Ord, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Tier {
    Exact,
    Prefix,
    Substring,
    FieldText,
    Fuzzy,
}

/// What the text matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchedOn {
    Id,
    Title,
    /// A string field (its name).
    Field(Sym),
    /// The text is empty: every entity matches.
    Everything,
}

impl MatchedOn {
    /// The `match_field` MCP search reports: `"id"`, `"title"`, the
    /// field's name; none for an empty text.
    pub fn field_name(&self) -> Option<&str> {
        match self {
            MatchedOn::Id => Some("id"),
            MatchedOn::Title => Some("title"),
            MatchedOn::Field(field) => Some(field.as_str()),
            MatchedOn::Everything => None,
        }
    }
}

/// One entity a text finds.
#[derive(Clone, Debug)]
pub struct EntityMatch<'g> {
    pub node: &'g Node,
    pub tier: Tier,
    pub on: MatchedOn,
    /// The Jaro-Winkler similarity of the text to the id or the title,
    /// whichever is closer (1.0 for an empty text).
    pub similarity: f64,
    /// Monotone with the order: 1.0, 0.9, 0.8 and 0.7 for the first four
    /// tiers, 0.6 × similarity for a fuzzy match.
    pub score: f64,
}

/// The entities of `graph` that `query` finds, best first.
pub fn find_entities<'g>(graph: &'g Graph, query: &EntityQuery) -> Vec<EntityMatch<'g>> {
    let referrers: Option<std::collections::BTreeSet<Sym>> = query
        .referencing
        .map(|target| graph.edges_to(target).iter().map(|e| e.source).collect());
    let text = query.text.to_lowercase();
    let mut matches: Vec<EntityMatch<'g>> = graph
        .nodes()
        .into_iter()
        .filter(|n| query.kinds.is_empty() || query.kinds.contains(&n.kind.raw.as_str()))
        .filter(|n| referrers.as_ref().is_none_or(|ids| ids.contains(&n.id.raw)))
        .filter(|n| {
            query
                .field_contains
                .is_none_or(|(field, value)| field_holds(n, field, value))
        })
        .filter_map(|n| rank(n, &text, query.scope))
        .collect();
    matches.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then(b.similarity.total_cmp(&a.similarity))
            .then_with(|| a.node.id.raw.cmp(&b.node.id.raw))
    });
    if let Some(limit) = query.limit {
        matches.truncate(limit);
    }
    matches
}

/// How `node` matches the lowercase `text`, if it does.
fn rank<'g>(node: &'g Node, text: &str, scope: MatchScope) -> Option<EntityMatch<'g>> {
    let found = |tier, on, similarity: f64| {
        let score = match tier {
            Tier::Exact => 1.0,
            Tier::Prefix => 0.9,
            Tier::Substring => 0.8,
            Tier::FieldText => 0.7,
            Tier::Fuzzy => 0.6 * similarity,
        };
        Some(EntityMatch {
            node,
            tier,
            on,
            similarity,
            score,
        })
    };
    if text.is_empty() {
        return found(Tier::Exact, MatchedOn::Everything, 1.0);
    }
    let id = node.id.raw.as_str().to_lowercase();
    let title = node.title.as_deref().map(str::to_lowercase);
    let id_similarity = strsim::jaro_winkler(text, &id);
    let title_similarity = title
        .as_deref()
        .map_or(0.0, |t| strsim::jaro_winkler(text, t));
    let similarity = id_similarity.max(title_similarity);
    let names = [
        (MatchedOn::Id, Some(id.as_str())),
        (MatchedOn::Title, title.as_deref()),
    ];
    for (tier, test) in [
        (
            Tier::Exact,
            (|name: &str, text: &str| name == text) as fn(&str, &str) -> bool,
        ),
        (Tier::Prefix, |name, text| name.starts_with(text)),
        (Tier::Substring, |name, text| name.contains(text)),
    ] {
        if let Some((on, _)) = names
            .iter()
            .find(|(_, name)| name.is_some_and(|name| test(name, text)))
        {
            return found(tier, *on, similarity);
        }
    }
    if scope == MatchScope::NamesAndText
        && let Some(field) = node.fields.entries().iter().find(|e| {
            matches!(&e.value, FieldValue::String(value) if value.to_lowercase().contains(text))
        })
    {
        return found(Tier::FieldText, MatchedOn::Field(field.key), similarity);
    }
    if within_fuzzy_threshold(similarity) {
        let on = if id_similarity >= title_similarity {
            MatchedOn::Id
        } else {
            MatchedOn::Title
        };
        return found(Tier::Fuzzy, on, similarity);
    }
    None
}

/// Whether `node`'s `field` holds `value`, ignoring case: one of the
/// field's texts contains it, by the one field-text rule (ADR 0019): a
/// list's items (a reference list's ids, a verify list's statements, a
/// block's keys, each expression in its written form), else the scalar's
/// text.
fn field_holds(node: &Node, field: &str, value: &str) -> bool {
    let value = value.to_lowercase();
    node.fields.get(field).is_some_and(|v| {
        field_items(v)
            .unwrap_or_else(|| vec![field_text(v)])
            .iter()
            .any(|text| text.to_lowercase().contains(&value))
    })
}

/// The text of `node`'s string field `field` around the first place it
/// holds `text` (ignoring case): at most `width` characters on each side,
/// elided with `…`. What MCP search shows for a field-text match.
pub fn snippet(node: &Node, field: Sym, text: &str, width: usize) -> Option<String> {
    let value = node.fields.entries().iter().find_map(|e| match &e.value {
        FieldValue::String(value) if e.key == field => Some(value),
        _ => None,
    })?;
    let lower = value.to_lowercase();
    let at = lower.find(&text.to_lowercase())?;
    // Lowercasing can change byte lengths; fall back to the whole value.
    if lower.len() != value.len() {
        return Some(value.clone());
    }
    let chars: Vec<(usize, char)> = value.char_indices().collect();
    let start_char = chars.iter().position(|(i, _)| *i >= at).unwrap_or(0);
    let end_byte = at + text.len();
    let end_char = chars
        .iter()
        .position(|(i, _)| *i >= end_byte)
        .unwrap_or(chars.len());
    let from = start_char.saturating_sub(width);
    let to = (end_char + width).min(chars.len());
    let mut out = String::new();
    if from > 0 {
        out.push('…');
    }
    out.extend(chars[from..to].iter().map(|(_, c)| c));
    if to < chars.len() {
        out.push('…');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fuzzy_threshold_is_inclusive() {
        assert!(!within_fuzzy_threshold(0.79));
        assert!(!within_fuzzy_threshold(0.799_999));
        assert!(within_fuzzy_threshold(0.80));
        assert!(within_fuzzy_threshold(0.874));
    }

    #[test]
    fn tiers_order_best_first() {
        assert!(Tier::Exact < Tier::Prefix);
        assert!(Tier::Prefix < Tier::Substring);
        assert!(Tier::Substring < Tier::FieldText);
        assert!(Tier::FieldText < Tier::Fuzzy);
    }
}
