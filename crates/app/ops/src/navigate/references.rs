//! An entity's references without their tokens: who references it, what
//! it refers to, through which field (ADR 0016 D2). The navigator's
//! occurrences and this list select the same edges ([`reference_edges`]);
//! this one needs no file text.

use std::collections::BTreeSet;

use specforge_common::Sym;
use specforge_graph::{Edge, Graph, Node};

use super::Direction;
use crate::view::ProjectView;

/// One reference without its token: the entity at the other end, its
/// kind, and the field (edge label) that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub peer: Sym,
    /// `None` only for an outgoing reference to an entity the graph lacks
    /// (E003 reports it).
    pub peer_kind: Option<Sym>,
    pub field: Sym,
}

/// An entity's references, in graph edge order (one per edge).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct References {
    /// Other entities' references to it.
    pub incoming: Vec<Reference>,
    /// Its references to other entities.
    pub outgoing: Vec<Reference>,
}

impl References {
    /// The references of `id`; empty for an entity the graph lacks.
    pub fn of(view: &ProjectView, id: &str) -> Self {
        let graph = view.graph();
        if graph.node(id).is_none() {
            return References::default();
        }
        let kind_of = |id: Sym| graph.node(id.as_str()).map(|node| node.kind.raw);
        References {
            incoming: reference_edges(graph, id, Direction::Incoming)
                .map(|(holder, edge)| Reference {
                    peer: edge.source,
                    peer_kind: Some(holder.kind.raw),
                    field: edge.label,
                })
                .collect(),
            outgoing: reference_edges(graph, id, Direction::Outgoing)
                .map(|(_, edge)| Reference {
                    peer: edge.target,
                    peer_kind: kind_of(edge.target),
                    field: edge.label,
                })
                .collect(),
        }
    }

    /// The entities referencing it, distinct, sorted by id.
    pub fn referenced_by(&self) -> Vec<&str> {
        distinct(&self.incoming)
    }

    /// The entities it refers to, distinct, sorted by id.
    pub fn refers_to(&self) -> Vec<&str> {
        distinct(&self.outgoing)
    }
}

fn distinct(references: &[Reference]) -> Vec<&str> {
    references
        .iter()
        .map(|r| r.peer.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The edges that are references of `id` in `direction`, each with its
/// holder (the edge's source): an edge whose holder the graph has, in
/// graph edge order, incoming before outgoing for [`Direction::Both`].
/// [`super::Navigator::references`] and [`References::of`] both select
/// through it, so the holder is looked up once, here.
pub(crate) fn reference_edges<'g>(
    graph: &'g Graph,
    id: &str,
    direction: Direction,
) -> impl Iterator<Item = (&'g Node, &'g Edge)> + 'g {
    let incoming = matches!(direction, Direction::Incoming | Direction::Both)
        .then(|| graph.edges_to(id))
        .unwrap_or_default();
    let outgoing = matches!(direction, Direction::Outgoing | Direction::Both)
        .then(|| graph.edges_from(id))
        .unwrap_or_default();
    incoming
        .into_iter()
        .chain(outgoing)
        .filter_map(move |edge| Some((graph.node(edge.source.as_str())?, edge)))
}
