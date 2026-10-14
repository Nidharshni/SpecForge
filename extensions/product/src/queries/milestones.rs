use super::{of_kind, ratio, sorted_dedup, targets, text};
use crate::evidence::{self, EvidenceState, FeatureEvidence};
use crate::vocabulary::*;
use serde::Serialize;
use specforge_extension_sdk::prelude::{CommandEvidence, CommandGraph};

/// `MilestoneCompletionPayload`: how many of the milestone's features are
/// done, as a count, a ratio in [0, 1] and their ids.
#[derive(Debug, Serialize)]
pub struct MilestoneCompletion {
    pub milestone_id: String,
    pub total_features: usize,
    pub done_count: usize,
    pub completion_ratio: f64,
    pub done_features: Vec<String>,
    /// Whether the command's input carried recorded test evidence
    /// ([`MilestoneCompletion::with_evidence`]); absent when not asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidenceState>,
    /// How many of the features the recorded tests prove, as a count, a
    /// ratio in [0, 1] and their ids, beside the declared `done_count`
    /// (ADR 0039); absent without a scored report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proven_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proven_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proven_features: Option<Vec<String>>,
    /// Each feature's evidence, in the milestone's order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_evidence: Option<Vec<FeatureEvidence>>,
    /// The milestone's status and its features, for the human layout.
    #[serde(skip)]
    pub status: Option<String>,
    #[serde(skip)]
    pub features: Vec<FeatureStatus>,
}

impl MilestoneCompletion {
    /// The completion with what `evidence` (a command's input) proves of
    /// its features: their evidence and the proven count, or only the
    /// evidence state when the input carries no scored report.
    pub fn with_evidence(mut self, graph: &CommandGraph, evidence: &CommandEvidence) -> Self {
        let ids: Vec<String> = self.features.iter().map(|f| f.id.clone()).collect();
        self.evidence = Some(EvidenceState::of(evidence));
        if let Some(features) = evidence::of_features(graph, &ids, evidence) {
            let proven: Vec<String> = features
                .iter()
                .filter(|f| f.proven)
                .map(|f| f.feature_id.clone())
                .collect();
            self.proven_count = Some(proven.len());
            self.proven_ratio = Some(ratio(proven.len(), ids.len()));
            self.proven_features = Some(proven);
            self.feature_evidence = Some(features);
        }
        self
    }
}

#[derive(Debug)]
pub struct FeatureStatus {
    pub id: String,
    pub status: Option<String>,
}

/// The milestone's features (in declaration order) and which are done;
/// `None` when `milestone_id` is not a milestone.
pub fn milestone_completion(
    graph: &CommandGraph,
    milestone_id: &str,
) -> Option<MilestoneCompletion> {
    let node = of_kind(graph, milestone_id, "milestone")?;
    // The referenced features, in the order the milestone lists them.
    let mut referenced = targets(graph, milestone_id, "features");
    let declared = node.list("features");
    referenced.sort_by_key(|id| declared.iter().position(|d| d == id).unwrap_or(usize::MAX));
    let features: Vec<FeatureStatus> = referenced
        .iter()
        .filter_map(|id| graph.node(id))
        .map(|f| FeatureStatus {
            id: f.id.clone(),
            status: text(f, "status"),
        })
        .collect();
    let done_features: Vec<String> = features
        .iter()
        .filter(|f| f.status.as_deref() == Some(FEATURE_DONE))
        .map(|f| f.id.clone())
        .collect();
    Some(MilestoneCompletion {
        milestone_id: milestone_id.to_string(),
        total_features: features.len(),
        done_count: done_features.len(),
        completion_ratio: ratio(done_features.len(), features.len()),
        done_features,
        evidence: None,
        proven_count: None,
        proven_ratio: None,
        proven_features: None,
        feature_evidence: None,
        status: text(node, "status"),
        features,
    })
}

/// `JourneyCoveragePayload`: how many of the journey's features are done,
/// and the ones that are not.
#[derive(Debug, Serialize)]
pub struct JourneyCoverage {
    pub journey_id: String,
    pub total_features: usize,
    pub covered_count: usize,
    pub uncovered_features: Vec<String>,
    /// The journey's persona, for the human layout.
    #[serde(skip)]
    pub persona: Option<String>,
}

/// How many of the journey's features have status `done` (a feature
/// without a status is uncovered); `None` when `journey_id` is not a
/// journey.
pub fn journey_coverage(graph: &CommandGraph, journey_id: &str) -> Option<JourneyCoverage> {
    let node = of_kind(graph, journey_id, "journey")?;
    let features = sorted_dedup(targets(graph, journey_id, "features"));
    let (covered, uncovered): (Vec<String>, Vec<String>) = features
        .into_iter()
        .partition(|f| graph.node(f).and_then(|n| n.text("status")) == Some(FEATURE_DONE));
    Some(JourneyCoverage {
        journey_id: journey_id.to_string(),
        total_features: covered.len() + uncovered.len(),
        covered_count: covered.len(),
        uncovered_features: uncovered,
        persona: text(node, "persona"),
    })
}
