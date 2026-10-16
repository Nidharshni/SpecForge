//! The exploration: where to start reading a project's graph, a read view
//! over the project view (ADR 0015, "Prompt read views"). The explore
//! prompt and `specforge explore` render it.

use std::cmp::Reverse;
use std::collections::BTreeSet;

use serde::Serialize;
use specforge_common::DiagnosticList;
use specforge_common::shape::Shape;
use specforge_graph::Graph;

use crate::view::{EntityField, ProjectView};
use crate::{Noticed, Refusal};

/// How many starting points an exploration names.
pub const STARTING_POINTS: usize = 5;
/// How many of the most connected entities an exploration names.
pub const MOST_CONNECTED: usize = 10;

/// What to explore.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExplorationRequest<'a> {
    /// Explore from this entity: the selection is what it reaches.
    pub entity_id: Option<&'a str>,
    /// Only entities of this kind. A kind the project does not know selects
    /// nothing and is an I020 notice (`KnownKinds::unknown_in`).
    pub kind: Option<&'a str>,
    /// Hops from `entity_id`; unbounded when `None`; ignored without
    /// `entity_id`.
    pub depth: Option<usize>,
}

/// The path from the exploration's entity to one entity it reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipPath {
    pub to: String,
    /// The labels of the edges on the path, from the entity on
    /// (`Graph::reach_path`).
    pub labels: Vec<String>,
}

/// What an exploration answers. Every list is about the selected entities:
/// those `entity_id` reaches within `depth` (every entity without it), of
/// `kind` when given. Degrees count every edge to or from another entity of
/// the project, selected or not.
#[derive(Debug, Clone, PartialEq)]
pub struct Exploration {
    /// The entity explored from.
    pub from: Option<String>,
    /// The selected entities, in id order.
    pub selected: Vec<String>,
    /// From `from` to each selected entity it reaches (`from` aside),
    /// nearest first.
    pub paths: Vec<RelationshipPath>,
    /// The selected connected entities with the highest lead, ties by id,
    /// at most [`STARTING_POINTS`].
    pub starting_points: Vec<String>,
    /// The selected connected entities with the most edges, ties by id, at
    /// most [`MOST_CONNECTED`].
    pub most_connected: Vec<String>,
    /// The selected unconnected entities, in id order.
    pub unconnected: Vec<String>,
}

impl Noticed<Exploration> {
    /// The exploration as the explore prompt's payload and `specforge
    /// explore --format json` write it, its notices (I020) in `notices`.
    pub fn document(&self) -> ExplorationDocument {
        let exploration = self.value();
        let from = exploration.from.clone().unwrap_or_default();
        ExplorationDocument {
            matching_entities: exploration.selected.clone(),
            relationship_paths: exploration
                .paths
                .iter()
                .map(|path| PathDocument {
                    from_entity: from.clone(),
                    to_entity: path.to.clone(),
                    edge_types: path.labels.clone(),
                    path_length: path.labels.len(),
                })
                .collect(),
            starting_points: exploration.starting_points.clone(),
            high_connectivity: exploration.most_connected.clone(),
            unconnected: exploration.unconnected.clone(),
            notices: DiagnosticList(self.notices().to_vec()),
        }
    }
}

/// The exploration as `specforge explore --format json` and the explore
/// prompt print it (`McpExplorePromptResult`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct ExplorationDocument {
    pub matching_entities: Vec<String>,
    pub relationship_paths: Vec<PathDocument>,
    pub starting_points: Vec<String>,
    pub high_connectivity: Vec<String>,
    pub unconnected: Vec<String>,
    /// I020 for a kind the project does not know.
    pub notices: DiagnosticList,
}

/// One relationship path of an exploration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct PathDocument {
    pub from_entity: String,
    pub to_entity: String,
    pub edge_types: Vec<String>,
    pub path_length: usize,
}

/// The exploration `request` asks for. An `entity_id` the graph lacks is
/// `view::not_found` (E003, did-you-mean).
pub fn explore(
    view: &ProjectView,
    request: &ExplorationRequest,
) -> Result<Noticed<Exploration>, Refusal<EntityField>> {
    let graph = view.graph();
    let notices = request
        .kind
        .map(|kind| view.kinds().unknown_in(&[kind]))
        .unwrap_or_default();

    let reached = request
        .entity_id
        .map(|id| view.neighbourhood(id, request.depth))
        .transpose()?;
    let of_kind = |id: &str| {
        request
            .kind
            .is_none_or(|kind| graph.node(id).is_some_and(|node| node.kind.raw == kind))
    };
    let selection: BTreeSet<&'static str> = match &reached {
        Some(reached) => reached
            .iter()
            .map(|r| r.id.as_str())
            .filter(|id| of_kind(id))
            .collect(),
        None => graph
            .nodes()
            .into_iter()
            .map(|node| node.id.raw.as_str())
            .filter(|id| of_kind(id))
            .collect(),
    };

    let connectivity = view.connectivity();
    let mut connected: Vec<(&'static str, _)> = connectivity
        .iter()
        .filter(|(id, degree)| selection.contains(id) && !degree.is_unconnected())
        .collect();
    connected.sort_by_key(|(id, degree)| (Reverse(degree.lead()), *id));
    let starting_points = connected
        .iter()
        .take(STARTING_POINTS)
        .map(|(id, _)| id.to_string())
        .collect();
    connected.sort_by_key(|(id, degree)| (Reverse(degree.total()), *id));
    let most_connected = connected
        .iter()
        .take(MOST_CONNECTED)
        .map(|(id, _)| id.to_string())
        .collect();
    let unconnected = connectivity
        .unconnected()
        .filter(|id| selection.contains(id))
        .map(str::to_string)
        .collect();

    let paths = reached
        .as_deref()
        .map(|reached| {
            reached
                .iter()
                .skip(1)
                .filter(|r| selection.contains(r.id.as_str()))
                .map(|r| RelationshipPath {
                    to: r.id.as_str().to_string(),
                    labels: Graph::reach_path(reached, r.id)
                        .iter()
                        .map(|label| label.as_str().to_string())
                        .collect(),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Noticed::new(
        Exploration {
            from: request.entity_id.map(str::to_string),
            selected: selection.into_iter().map(str::to_string).collect(),
            paths,
            starting_points,
            most_connected,
            unconnected,
        },
        notices,
    ))
}
