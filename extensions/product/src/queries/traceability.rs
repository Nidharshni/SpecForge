use super::{into, of_kind, out, sorted_dedup};
use serde::Serialize;
use specforge_extension_sdk::prelude::CommandGraph;

/// `DeliverableTraceabilityPayload`: every feature the deliverable reaches
/// through its journeys or its modules, and how many each path reaches.
#[derive(Debug, Serialize)]
pub struct DeliverableTraceability {
    pub deliverable_id: String,
    pub transitive_features: Vec<String>,
    pub journey_path_count: usize,
    pub module_path_count: usize,
}

/// The features `deliverable -> journeys -> features` and `deliverable ->
/// modules -> features` reach, once each; `None` when `deliverable_id` is
/// not a deliverable.
pub fn deliverable_traceability(
    graph: &CommandGraph,
    deliverable_id: &str,
) -> Option<DeliverableTraceability> {
    of_kind(graph, deliverable_id, "deliverable")?;
    let (via_journeys, via_modules) = features_of_deliverable(graph, deliverable_id);
    Some(DeliverableTraceability {
        deliverable_id: deliverable_id.to_string(),
        journey_path_count: via_journeys.len(),
        module_path_count: via_modules.len(),
        transitive_features: sorted_dedup([via_journeys, via_modules].concat()),
    })
}

/// `FeatureDeliverablePayload`: every deliverable that holds the feature
/// through a journey or a module, and how many each path reaches.
#[derive(Debug, Serialize)]
pub struct FeatureDeliverables {
    pub feature_id: String,
    pub deliverables: Vec<String>,
    pub via_journey_count: usize,
    pub via_module_count: usize,
}

/// The deliverables the reverse paths `feature <- journey <- deliverable`
/// and `feature <- module <- deliverable` reach, once each, sorted; `None`
/// when `feature_id` is not a feature.
pub fn feature_deliverables(graph: &CommandGraph, feature_id: &str) -> Option<FeatureDeliverables> {
    of_kind(graph, feature_id, "feature")?;
    let (via_journeys, via_modules) = deliverables_of_feature(graph, feature_id);
    Some(FeatureDeliverables {
        feature_id: feature_id.to_string(),
        via_journey_count: via_journeys.len(),
        via_module_count: via_modules.len(),
        deliverables: sorted_dedup([via_journeys, via_modules].concat()),
    })
}

/// `PersonaChannelPayload`: the channels of every journey the persona
/// undertakes.
#[derive(Debug, Serialize)]
pub struct PersonaChannels {
    pub persona_id: String,
    pub channels: Vec<String>,
    pub count: usize,
}

/// The channels `persona <- journey -> channels` reaches, sorted, once
/// each; `None` when `persona_id` is not a persona.
pub fn persona_channels(graph: &CommandGraph, persona_id: &str) -> Option<PersonaChannels> {
    of_kind(graph, persona_id, "persona")?;
    let channels = sorted_dedup(
        journeys_of_persona(graph, persona_id)
            .iter()
            .flat_map(|j| out(graph, j, "channels", "channel"))
            .collect(),
    );
    Some(PersonaChannels {
        persona_id: persona_id.to_string(),
        count: channels.len(),
        channels,
    })
}

/// `DeliverablePersonaPayload`: the personas the deliverable's journeys
/// target, and those journeys.
#[derive(Debug, Serialize)]
pub struct DeliverablePersonas {
    pub deliverable_id: String,
    pub personas: Vec<String>,
    pub via_journey_ids: Vec<String>,
    pub count: usize,
}

/// The personas `deliverable -> journeys -> persona` reaches, sorted, once
/// each, and the journeys on those paths (a journey without a persona is on
/// none); `None` when `deliverable_id` is not a deliverable.
pub fn deliverable_personas(
    graph: &CommandGraph,
    deliverable_id: &str,
) -> Option<DeliverablePersonas> {
    of_kind(graph, deliverable_id, "deliverable")?;
    let mut personas = Vec::new();
    let mut via_journey_ids = Vec::new();
    for journey in out(graph, deliverable_id, "journeys", "journey") {
        let targeted = out(graph, &journey, "persona", "persona");
        if !targeted.is_empty() {
            personas.extend(targeted);
            via_journey_ids.push(journey);
        }
    }
    let personas = sorted_dedup(personas);
    Some(DeliverablePersonas {
        deliverable_id: deliverable_id.to_string(),
        count: personas.len(),
        personas,
        via_journey_ids,
    })
}

/// The journeys that target `persona`: by the reference, or by the field
/// naming it.
pub(super) fn journeys_of_persona(graph: &CommandGraph, persona: &str) -> Vec<String> {
    let by_field = graph
        .nodes_of_kind("journey")
        .filter(|j| j.text("persona") == Some(persona))
        .map(|j| j.id.clone());
    sorted_dedup(
        into(graph, persona, "persona", "journey")
            .into_iter()
            .chain(by_field)
            .collect(),
    )
}

/// The journeys that use `channel`.
pub(super) fn journeys_of_channel(graph: &CommandGraph, channel: &str) -> Vec<String> {
    into(graph, channel, "channels", "journey")
}

/// The features `deliverable` reaches through its journeys, and through
/// its modules, each sorted, once each.
fn features_of_deliverable(graph: &CommandGraph, deliverable: &str) -> (Vec<String>, Vec<String>) {
    let through = |label: &str, kind: &str| {
        sorted_dedup(
            out(graph, deliverable, label, kind)
                .iter()
                .flat_map(|hop| out(graph, hop, "features", "feature"))
                .collect(),
        )
    };
    (through("journeys", "journey"), through("modules", "module"))
}

/// The deliverables that reach `feature` through a journey, and through a
/// module, each sorted, once each.
pub fn deliverables_of_feature(graph: &CommandGraph, feature: &str) -> (Vec<String>, Vec<String>) {
    let through = |kind: &str, label: &str| {
        sorted_dedup(
            into(graph, feature, "features", kind)
                .iter()
                .flat_map(|hop| into(graph, hop, label, "deliverable"))
                .collect(),
        )
    };
    (through("journey", "journeys"), through("module", "modules"))
}
