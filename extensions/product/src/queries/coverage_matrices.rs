use super::features::through_journeys;
use super::traceability::{deliverables_of_feature, journeys_of_channel, journeys_of_persona};
use super::{ratio, sorted_dedup};
use serde::Serialize;
use specforge_extension_sdk::prelude::CommandGraph;

/// What one persona's or channel's journeys reach (`PersonaCoverageEntry`
/// and `ChannelCoverageEntry` beside the id).
#[derive(Debug, Serialize)]
pub struct Reach {
    pub reachable_features: Vec<String>,
    pub unreachable_features: Vec<String>,
    pub coverage_ratio: f64,
    pub journey_count: usize,
}

/// `PersonaCoverageEntry`.
#[derive(Debug, Serialize)]
pub struct PersonaCoverageEntry {
    pub persona_id: String,
    #[serde(flatten)]
    pub reach: Reach,
}

/// `ChannelCoverageEntry`.
#[derive(Debug, Serialize)]
pub struct ChannelCoverageEntry {
    pub channel_id: String,
    #[serde(flatten)]
    pub reach: Reach,
}

/// A coverage matrix before paging: one entry per persona or channel, by
/// id, over `total_features`; `overall_coverage` is the mean of every
/// entry's ratio (`null` without entries).
#[derive(Debug)]
pub struct CoverageMatrix<T> {
    pub entries: Vec<T>,
    pub total_features: usize,
    pub overall_coverage: Option<f64>,
}

/// `PersonaCoverageMatrixPayload` before paging: the features each
/// persona reaches through the journeys that target it.
pub fn persona_coverage_matrix(graph: &CommandGraph) -> CoverageMatrix<PersonaCoverageEntry> {
    coverage_matrix(
        graph,
        "persona",
        journeys_of_persona,
        |persona_id, reach| PersonaCoverageEntry { persona_id, reach },
    )
}

/// `ChannelCoverageMatrixPayload` before paging: the features each
/// channel reaches through the journeys that use it.
pub fn channel_coverage_matrix(graph: &CommandGraph) -> CoverageMatrix<ChannelCoverageEntry> {
    coverage_matrix(
        graph,
        "channel",
        journeys_of_channel,
        |channel_id, reach| ChannelCoverageEntry { channel_id, reach },
    )
}

/// Each `kind` entity's [`Reach`] over every feature, through the journeys
/// `journeys` gives it (each feature once, however many journeys reach
/// it); a ratio is 0 without journeys, or without features.
fn coverage_matrix<T>(
    graph: &CommandGraph,
    kind: &str,
    journeys: fn(&CommandGraph, &str) -> Vec<String>,
    entry: impl Fn(String, Reach) -> T,
) -> CoverageMatrix<T> {
    let features = sorted_dedup(
        graph
            .nodes_of_kind("feature")
            .map(|f| f.id.clone())
            .collect(),
    );
    let mut ids: Vec<String> = graph.nodes_of_kind(kind).map(|n| n.id.clone()).collect();
    ids = sorted_dedup(ids);
    let mut ratios = 0.0;
    let entries: Vec<T> = ids
        .into_iter()
        .map(|id| {
            let (reachable, via) = through_journeys(graph, journeys(graph, &id));
            let unreachable: Vec<String> = features
                .iter()
                .filter(|f| reachable.binary_search(f).is_err())
                .cloned()
                .collect();
            let coverage_ratio = ratio(reachable.len(), features.len());
            ratios += coverage_ratio;
            entry(
                id,
                Reach {
                    reachable_features: reachable,
                    unreachable_features: unreachable,
                    coverage_ratio,
                    journey_count: via.len(),
                },
            )
        })
        .collect();
    CoverageMatrix {
        overall_coverage: (!entries.is_empty()).then(|| ratios / entries.len() as f64),
        total_features: features.len(),
        entries,
    }
}

/// `FeatureOverlapEntry`.
#[derive(Debug, Serialize)]
pub struct FeatureOverlapEntry {
    pub feature_id: String,
    pub deliverable_ids: Vec<String>,
    pub deliverable_count: usize,
}

/// `FeatureOverlapPayload` before paging.
#[derive(Debug)]
pub struct FeatureOverlap {
    pub overlapping_features: Vec<FeatureOverlapEntry>,
    pub total_features: usize,
}

/// The features two or more deliverables reach, through a journey or a
/// module (a deliverable reaching one both ways counts once), with those
/// deliverables sorted; the most shared first, ties by id.
pub fn feature_overlap(graph: &CommandGraph) -> FeatureOverlap {
    let features = sorted_dedup(
        graph
            .nodes_of_kind("feature")
            .map(|f| f.id.clone())
            .collect(),
    );
    let mut overlapping: Vec<FeatureOverlapEntry> = features
        .iter()
        .filter_map(|f| {
            let (via_journeys, via_modules) = deliverables_of_feature(graph, f);
            let deliverable_ids = sorted_dedup([via_journeys, via_modules].concat());
            (deliverable_ids.len() >= 2).then(|| FeatureOverlapEntry {
                feature_id: f.clone(),
                deliverable_count: deliverable_ids.len(),
                deliverable_ids,
            })
        })
        .collect();
    overlapping.sort_by(|a, b| {
        b.deliverable_count
            .cmp(&a.deliverable_count)
            .then_with(|| a.feature_id.cmp(&b.feature_id))
    });
    FeatureOverlap {
        overlapping_features: overlapping,
        total_features: features.len(),
    }
}
