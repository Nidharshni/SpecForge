use super::milestones::{milestone_completion, MilestoneCompletion};
use super::{count_in, of_kind, out, priority_rank, ratio, share, status_is, text};
use crate::vocabulary::*;
use serde::Serialize;
use specforge_extension_sdk::prelude::{CommandGraph, GraphNode};
use std::collections::BTreeMap;

/// `DeliverableCompletionPayload`: how many of the deliverable's milestones
/// are completed, and (on request) each one's own feature completion.
#[derive(Debug, Serialize)]
pub struct DeliverableCompletion {
    pub deliverable_id: String,
    pub milestone_count: usize,
    pub completed_count: usize,
    pub completion_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone_details: Option<Vec<MilestoneCompletion>>,
}

/// The milestones the deliverable is tracked by and how many are
/// `completed` (a milestone without a status is `planned`); the ratio is
/// 0 without milestones. `details` adds each milestone's completion, by
/// id. `None` when `deliverable_id` is not a deliverable.
pub fn deliverable_completion(
    graph: &CommandGraph,
    deliverable_id: &str,
    details: bool,
) -> Option<DeliverableCompletion> {
    of_kind(graph, deliverable_id, "deliverable")?;
    let milestones = out(graph, deliverable_id, "milestones", "milestone");
    let completed_count = milestones
        .iter()
        .filter(|m| status_is(graph, m, MILESTONE_COMPLETED))
        .count();
    Some(DeliverableCompletion {
        deliverable_id: deliverable_id.to_string(),
        milestone_count: milestones.len(),
        completed_count,
        completion_ratio: ratio(completed_count, milestones.len()),
        milestone_details: details.then(|| {
            milestones
                .iter()
                .filter_map(|m| milestone_completion(graph, m))
                .collect()
        }),
    })
}

/// `ReleaseCompletionPayload`: how many of the release's deliverables are
/// shipped.
#[derive(Debug, Serialize)]
pub struct ReleaseCompletion {
    pub release_id: String,
    pub total: usize,
    pub shipped: usize,
    pub completion_ratio: Option<f64>,
}

/// The deliverables the release includes and how many are `shipped`; the
/// ratio is `null` without deliverables. `None` when `release_id` is not a
/// release.
pub fn release_completion(graph: &CommandGraph, release_id: &str) -> Option<ReleaseCompletion> {
    of_kind(graph, release_id, "release")?;
    let deliverables = out(graph, release_id, "deliverables", "deliverable");
    let shipped = deliverables
        .iter()
        .filter(|d| status_is(graph, d, DELIVERABLE_SHIPPED))
        .count();
    Some(ReleaseCompletion {
        release_id: release_id.to_string(),
        total: deliverables.len(),
        shipped,
        completion_ratio: share(shipped, deliverables.len()),
    })
}

/// `DeliverablePriorityPayload`: the highest priority among the
/// deliverable's milestones and journeys that declare one.
#[derive(Debug, Serialize)]
pub struct DeliverablePriority {
    pub deliverable_id: String,
    pub priority: Option<String>,
    pub source_count: usize,
}

/// The highest `Priority` (critical first) among the milestones and
/// journeys the deliverable references that declare one; a constituent
/// without a priority is left out, not counted as medium. `priority` is
/// `null` when none declares one. `None` when `deliverable_id` is not a
/// deliverable.
pub fn deliverable_priority(
    graph: &CommandGraph,
    deliverable_id: &str,
) -> Option<DeliverablePriority> {
    of_kind(graph, deliverable_id, "deliverable")?;
    let ranks: Vec<usize> = out(graph, deliverable_id, "milestones", "milestone")
        .into_iter()
        .chain(out(graph, deliverable_id, "journeys", "journey"))
        .filter_map(|id| priority_rank(graph.node(&id)?.text("priority")?))
        .collect();
    Some(DeliverablePriority {
        deliverable_id: deliverable_id.to_string(),
        priority: ranks
            .iter()
            .min()
            .map(|&r| PRIORITY.values()[r].to_string()),
        source_count: ranks.len(),
    })
}

/// `UnscheduledFeaturesPayload`: the features no milestone delivers.
#[derive(Debug, Serialize)]
pub struct UnscheduledFeatures {
    pub features: Vec<String>,
    pub count: usize,
    pub total_features: usize,
    pub scheduled_count: usize,
    /// Each unscheduled feature's status as written, for the human table.
    #[serde(skip)]
    pub statuses: Vec<Option<String>>,
}

/// The features no milestone lists under `features`, sorted by id.
pub fn unscheduled_features(graph: &CommandGraph) -> UnscheduledFeatures {
    let mut all: Vec<&GraphNode> = graph.nodes_of_kind("feature").collect();
    all.sort_by(|a, b| a.id.cmp(&b.id));
    let total_features = all.len();
    let unscheduled: Vec<&GraphNode> = all
        .into_iter()
        .filter(|f| count_in(graph, &f.id, "features", "milestone") == 0)
        .collect();
    UnscheduledFeatures {
        count: unscheduled.len(),
        scheduled_count: total_features - unscheduled.len(),
        total_features,
        statuses: unscheduled.iter().map(|f| text(f, "status")).collect(),
        features: unscheduled.into_iter().map(|f| f.id.clone()).collect(),
    }
}

/// The kinds with an `owner` field, as `OwnerKindBreakdown` names them.
const OWNED_KINDS: &[(&str, &str)] = &[
    ("feature", "features"),
    ("milestone", "milestones"),
    ("deliverable", "deliverables"),
    ("release", "releases"),
];

/// `OwnerWorkloadEntry`: what one owner owns.
#[derive(Debug, Serialize)]
pub struct OwnerWorkloadEntry {
    pub owner: String,
    pub entity_ids: Vec<String>,
    pub entity_count: usize,
    pub by_kind: OwnerKindBreakdown,
}

/// `OwnerKindBreakdown`.
#[derive(Debug, Default, Serialize)]
pub struct OwnerKindBreakdown {
    pub features: usize,
    pub milestones: usize,
    pub deliverables: usize,
    pub releases: usize,
}

impl OwnerKindBreakdown {
    fn count(&mut self, kind: &str) {
        match kind {
            "feature" => self.features += 1,
            "milestone" => self.milestones += 1,
            "deliverable" => self.deliverables += 1,
            _ => self.releases += 1,
        }
    }
}

/// `OwnerWorkloadPayload` before paging: every owner of a feature,
/// milestone, deliverable or release, most entities first (ties by owner),
/// and how many of those entities have no owner.
#[derive(Debug)]
pub struct OwnerWorkload {
    pub owners: Vec<OwnerWorkloadEntry>,
    pub unowned_count: usize,
    pub total_entities: usize,
}

/// Each owner string once (trimmed; an empty one is no owner), with the
/// ids it owns sorted.
pub fn owner_workload(graph: &CommandGraph) -> OwnerWorkload {
    let mut owners: BTreeMap<String, OwnerWorkloadEntry> = BTreeMap::new();
    let (mut unowned_count, mut total_entities) = (0, 0);
    for (kind, _) in OWNED_KINDS {
        for node in graph.nodes_of_kind(kind) {
            total_entities += 1;
            let Some(owner) = node.text("owner").map(str::trim).filter(|o| !o.is_empty()) else {
                unowned_count += 1;
                continue;
            };
            let entry = owners
                .entry(owner.to_string())
                .or_insert_with(|| OwnerWorkloadEntry {
                    owner: owner.to_string(),
                    entity_ids: Vec::new(),
                    entity_count: 0,
                    by_kind: OwnerKindBreakdown::default(),
                });
            entry.entity_ids.push(node.id.clone());
            entry.entity_count += 1;
            entry.by_kind.count(kind);
        }
    }
    let mut owners: Vec<OwnerWorkloadEntry> = owners
        .into_values()
        .map(|mut e| {
            e.entity_ids.sort();
            e
        })
        .collect();
    owners.sort_by(|a, b| {
        b.entity_count
            .cmp(&a.entity_count)
            .then_with(|| a.owner.cmp(&b.owner))
    });
    OwnerWorkload {
        owners,
        unowned_count,
        total_entities,
    }
}
