//! Product queries: pure reads of the graph the host passes a command.
//!
//! Each one answers a planning question over this extension's own kinds and
//! the fields they declare (`status`, `priority`, `features`, ...). The
//! host knows none of them; it runs the `cmd__product_*` exports in
//! `commands.rs`, which render these results.

use crate::vocabulary::{self, *};
use specforge_extension_sdk::prelude::{CommandError, CommandGraph, GraphNode};

/// The kinds this extension declares, in the order `health` reports them.
pub const PRODUCT_KINDS: &[&str] = &[
    "feature",
    "journey",
    "deliverable",
    "milestone",
    "module",
    "term",
    "persona",
    "channel",
    "release",
];

mod coverage_matrices;
mod dates_and_effort;
mod dependency_graphs;
mod features;
mod lists;
mod milestones;
mod project_wide;
mod rollups;
mod term_analytics;
mod traceability;

pub use coverage_matrices::{
    channel_coverage_matrix, feature_overlap, persona_coverage_matrix, CoverageMatrix, Reach,
};
pub use dates_and_effort::{
    effort_weight, milestone_timeline, milestone_velocity, parse_ymd, weighted_milestone_completion,
};
pub use dependency_graphs::{
    critical_path, deliverable_dependents, feature_ordering, module_coupling,
    module_dependency_depth,
};
pub use features::{channel_features, feature_dependents, feature_impact, persona_features};
pub use lists::{
    list, paginate, sortable, ChannelListEntry, DeliverableListEntry, FeatureListEntry,
    JourneyListEntry, ListEntry, ListFilter, ListKind, MilestoneListEntry, ModuleListEntry, Page,
    PersonaListEntry, ReleaseListEntry, TermListEntry, SORT_ORDER,
};
// Read by the tests (`tests.rs`) and by nothing the build compiles.
#[cfg(test)]
pub use lists::{
    FilterArg, CHANNELS, DELIVERABLES, FEATURES, JOURNEYS, MILESTONES, MODULES, PERSONAS, RELEASES,
    TERMS,
};
pub use milestones::{journey_coverage, milestone_completion};
pub use project_wide::{bulk_status, project_health, HealthPayload};
pub use rollups::{
    deliverable_completion, deliverable_priority, owner_workload, release_completion,
    unscheduled_features,
};
pub use term_analytics::{term_clusters, term_density, term_graph};
pub use traceability::{
    deliverable_personas, deliverable_traceability, feature_deliverables, persona_channels,
};

// ── Helpers ────────────────────────────────────────────────────────────────

fn text(node: &GraphNode, field: &str) -> Option<String> {
    node.text(field).map(str::to_string)
}

fn of_kind<'a>(graph: &'a CommandGraph, id: &str, kind: &str) -> Option<&'a GraphNode> {
    graph.node(id).filter(|n| n.kind == kind)
}

fn kind_of<'a>(graph: &'a CommandGraph, id: &str) -> Option<&'a str> {
    graph.node(id).map(|n| n.kind.as_str())
}

/// The targets of `id`'s references declared in `label`.
fn targets(graph: &CommandGraph, id: &str, label: &str) -> Vec<String> {
    graph
        .edges_from(id)
        .iter()
        .filter(|e| e.label == label)
        .map(|e| e.target.clone())
        .collect()
}

/// The `kind` entities `id` references in `label`, sorted, once each.
fn out(graph: &CommandGraph, id: &str, label: &str, kind: &str) -> Vec<String> {
    sorted_dedup(
        targets(graph, id, label)
            .into_iter()
            .filter(|t| kind_of(graph, t) == Some(kind))
            .collect(),
    )
}

/// The `source_kind` entities that reference `id` in `label`, sorted,
/// once each.
fn into(graph: &CommandGraph, id: &str, label: &str, source_kind: &str) -> Vec<String> {
    sorted_dedup(
        graph
            .edges_to(id)
            .iter()
            .filter(|e| e.label == label && kind_of(graph, &e.source) == Some(source_kind))
            .map(|e| e.source.clone())
            .collect(),
    )
}

/// How many of `id`'s references are declared in `label`.
fn count_out(graph: &CommandGraph, id: &str, label: &str) -> usize {
    graph
        .edges_from(id)
        .iter()
        .filter(|e| e.label == label)
        .count()
}

/// How many `source_kind` entities reference `id` in `label`.
fn count_in(graph: &CommandGraph, id: &str, label: &str, source_kind: &str) -> usize {
    graph
        .edges_to(id)
        .iter()
        .filter(|e| e.label == label && kind_of(graph, &e.source) == Some(source_kind))
        .count()
}

/// `ids` sorted, each once.
fn sorted_dedup(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids.dedup();
    ids
}

/// `part / whole` in [0, 1]; 0 when `whole` is 0.
fn ratio(part: usize, whole: usize) -> f64 {
    if whole > 0 {
        part as f64 / whole as f64
    } else {
        0.0
    }
}

/// `part / whole` in [0, 1]; `None` when `whole` is 0.
fn share(part: usize, whole: usize) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

/// `node`'s lifecycle status: as written, or its kind's absent status
/// (feature `proposed`, deliverable `draft`, milestone and release `planned`,
/// persona and channel `active`); `None` for a kind without one.
fn status(node: &GraphNode) -> Option<&str> {
    node.text("status")
        .or_else(|| vocabulary::lifecycle(&node.kind)?.absent())
}

/// Whether the entity `id` has the lifecycle status `wanted`.
fn status_is(graph: &CommandGraph, id: &str, wanted: &str) -> bool {
    graph.node(id).and_then(status) == Some(wanted)
}

/// `medium`'s place in `Priority`: a feature without a priority ranks there.
const MEDIUM: usize = PRIORITY.index("medium");

/// `priority`'s place in `Priority`, 0 for critical; `None` outside it.
fn priority_rank(priority: &str) -> Option<usize> {
    PRIORITY.position(priority)
}

// ── Errors ─────────────────────────────────────────────────────────────────

/// The `ENTITY_NOT_FOUND` exit code.
pub const NOT_FOUND_EXIT: i32 = 1;

/// The `INVALID_INPUT` exit code: the one the host gives the usage errors
/// it catches itself.
pub const INVALID_INPUT_EXIT: i32 = 2;

/// `ProductSurfaceError` for an id that names no `kind`: the message names
/// both, and the suggestion is the nearest id of that kind, if any is
/// within edit distance 2.
pub fn not_found(graph: &CommandGraph, kind: &str, id: &str) -> CommandError {
    CommandError {
        entity_id: Some(id.to_string()),
        suggestion: suggest(graph, kind, id),
        ..CommandError::new("ENTITY_NOT_FOUND", format!("{kind} '{id}' not found"))
    }
}

/// `ProductSurfaceError` for an arg the command cannot use.
pub fn invalid_input(message: impl Into<String>) -> CommandError {
    CommandError::new("INVALID_INPUT", message)
}

/// The id of a `kind` entity nearest `id`, within Levenshtein distance 2;
/// the first by id among equally near ones.
pub fn suggest(graph: &CommandGraph, kind: &str, id: &str) -> Option<String> {
    graph
        .nodes_of_kind(kind)
        .map(|n| (levenshtein(&n.id, id), &n.id))
        .filter(|(distance, _)| *distance <= 2)
        .min()
        .map(|(_, near)| near.clone())
}

/// The edit distance between `a` and `b`, by character.
fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(ca != *cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

fn pct(part: usize, whole: usize) -> f64 {
    if whole > 0 {
        (part as f64 / whole as f64) * 100.0
    } else {
        0.0
    }
}
