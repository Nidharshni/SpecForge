use super::traceability::{deliverables_of_feature, journeys_of_channel, journeys_of_persona};
use super::{into, kind_of, of_kind, sorted_dedup, targets};
use serde::Serialize;
use specforge_extension_sdk::prelude::CommandGraph;

/// `FeatureImpactPayload`: what deferring or removing the feature touches.
#[derive(Debug, Serialize)]
pub struct FeatureImpact {
    pub feature_id: String,
    pub affected_journeys: Vec<String>,
    pub affected_milestones: Vec<String>,
    pub affected_deliverables: Vec<String>,
    pub affected_modules: Vec<String>,
    pub dependent_features: Vec<String>,
    pub total_affected_entities: usize,
}

/// The journeys, milestones and modules that list the feature, the
/// deliverables holding those journeys or modules, and the features that
/// depend on it, directly or through one another (only `depends_on`: a
/// feature listing it under `features` relates to it). Each sorted by id,
/// once; `None` when `feature_id` is not a feature.
pub fn feature_impact(graph: &CommandGraph, feature_id: &str) -> Option<FeatureImpact> {
    of_kind(graph, feature_id, "feature")?;
    let (via_journeys, via_modules) = deliverables_of_feature(graph, feature_id);
    let mut dependents: Vec<String> = Vec::new();
    let mut frontier = vec![feature_id.to_string()];
    while let Some(next) = frontier.pop() {
        for dependent in into(graph, &next, "depends_on", "feature") {
            if dependent != feature_id && !dependents.contains(&dependent) {
                dependents.push(dependent.clone());
                frontier.push(dependent);
            }
        }
    }
    let mut impact = FeatureImpact {
        feature_id: feature_id.to_string(),
        affected_journeys: into(graph, feature_id, "features", "journey"),
        affected_milestones: into(graph, feature_id, "features", "milestone"),
        affected_deliverables: sorted_dedup([via_journeys, via_modules].concat()),
        affected_modules: into(graph, feature_id, "features", "module"),
        dependent_features: sorted_dedup(dependents),
        total_affected_entities: 0,
    };
    // The kinds are disjoint, so the union is the sum.
    impact.total_affected_entities = impact.affected_journeys.len()
        + impact.affected_milestones.len()
        + impact.affected_deliverables.len()
        + impact.affected_modules.len()
        + impact.dependent_features.len();
    Some(impact)
}

/// `FeatureDependentPayload`: the features that declare `depends_on` the
/// feature, sorted by id.
#[derive(Debug, Serialize)]
pub struct FeatureDependents {
    pub feature_id: String,
    pub dependents: Vec<String>,
    pub count: usize,
}

/// The features that declare `depends_on` the feature; `None` when
/// `feature_id` is not a feature.
pub fn feature_dependents(graph: &CommandGraph, feature_id: &str) -> Option<FeatureDependents> {
    of_kind(graph, feature_id, "feature")?;
    let dependents = into(graph, feature_id, "depends_on", "feature");
    Some(FeatureDependents {
        feature_id: feature_id.to_string(),
        count: dependents.len(),
        dependents,
    })
}

/// `PersonaFeaturePayload`: the features of every journey the persona
/// undertakes, and those journeys.
#[derive(Debug, Serialize)]
pub struct PersonaFeatures {
    pub persona_id: String,
    pub features: Vec<String>,
    pub via_journey_ids: Vec<String>,
    pub count: usize,
}

/// The features of every journey the persona undertakes, sorted; `None`
/// when `persona_id` is not a persona.
pub fn persona_features(graph: &CommandGraph, persona_id: &str) -> Option<PersonaFeatures> {
    of_kind(graph, persona_id, "persona")?;
    let journeys = journeys_of_persona(graph, persona_id);
    let (features, via_journey_ids) = through_journeys(graph, journeys);
    Some(PersonaFeatures {
        persona_id: persona_id.to_string(),
        count: features.len(),
        features,
        via_journey_ids,
    })
}

/// `ChannelFeaturePayload`: the features of every journey that uses the
/// channel, and those journeys.
#[derive(Debug, Serialize)]
pub struct ChannelFeatures {
    pub channel_id: String,
    pub features: Vec<String>,
    pub via_journey_ids: Vec<String>,
    pub count: usize,
}

/// The features of every journey that uses the channel, sorted; `None`
/// when `channel_id` is not a channel.
pub fn channel_features(graph: &CommandGraph, channel_id: &str) -> Option<ChannelFeatures> {
    of_kind(graph, channel_id, "channel")?;
    let journeys = journeys_of_channel(graph, channel_id);
    let (features, via_journey_ids) = through_journeys(graph, journeys);
    Some(ChannelFeatures {
        channel_id: channel_id.to_string(),
        count: features.len(),
        features,
        via_journey_ids,
    })
}

/// The features `journeys` exercise, and the journeys, each sorted and
/// listed once.
pub(super) fn through_journeys(
    graph: &CommandGraph,
    journeys: Vec<String>,
) -> (Vec<String>, Vec<String>) {
    let journeys = sorted_dedup(journeys);
    let features = journeys
        .iter()
        .flat_map(|j| targets(graph, j, "features"))
        .filter(|f| kind_of(graph, f) == Some("feature"))
        .collect();
    (sorted_dedup(features), journeys)
}
