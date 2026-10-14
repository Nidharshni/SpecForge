//! How the entities of a project view are linked: the one rule for an
//! unconnected entity (CONTEXT.md "Unconnected entity"), the degrees the
//! exploration ranks by, and the neighbourhood of one entity that the
//! exploration and the review share (ADR 0015, "Prompt read views").

use std::collections::BTreeMap;

use specforge_common::Sym;
use specforge_graph::{Graph, Reached};

use super::{EntityField, ProjectView, not_found};
use crate::Refusal;

/// An entity's edges to and from *other* entities. An edge from the entity
/// to itself is in neither count; the graph holds no edge to a reference
/// that does not resolve (E003, I004 report those), so none is counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Degree {
    /// Edges from another entity to this one.
    pub incoming: usize,
    /// Edges from this entity to another one.
    pub outgoing: usize,
}

impl Degree {
    /// Every edge linking the entity to another one, both ways.
    pub fn total(self) -> usize {
        self.incoming + self.outgoing
    }

    /// No edge links the entity to another entity.
    pub fn is_unconnected(self) -> bool {
        self.total() == 0
    }

    /// How many more entities it references than reference it
    /// (`outgoing - incoming`): the starting points' rank.
    pub fn lead(self) -> isize {
        self.outgoing as isize - self.incoming as isize
    }
}

/// The degree of every entity of one graph, counted once over its edges.
#[derive(Debug, Clone, Default)]
pub struct Connectivity {
    /// Every node of the graph, in id order (Sym orders by content).
    degrees: BTreeMap<Sym, Degree>,
}

impl Connectivity {
    /// The degrees of `graph`'s entities: one pass over its edges, skipping
    /// each edge whose source is its target.
    pub fn of(graph: &Graph) -> Self {
        let mut degrees: BTreeMap<Sym, Degree> = graph
            .nodes()
            .into_iter()
            .map(|node| (node.id.raw, Degree::default()))
            .collect();
        for edge in graph.edges() {
            if edge.source == edge.target {
                continue;
            }
            if let Some(degree) = degrees.get_mut(&edge.source) {
                degree.outgoing += 1;
            }
            if let Some(degree) = degrees.get_mut(&edge.target) {
                degree.incoming += 1;
            }
        }
        Connectivity { degrees }
    }

    /// `id`'s degree; the zero degree for an id the graph lacks.
    pub fn degree(&self, id: &str) -> Degree {
        self.degrees.get(&Sym::new(id)).copied().unwrap_or_default()
    }

    /// Every entity with its degree, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, Degree)> + '_ {
        self.degrees
            .iter()
            .map(|(id, degree)| (id.as_str(), *degree))
    }

    /// The unconnected entities, in id order.
    pub fn unconnected(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.iter()
            .filter(|(_, degree)| degree.is_unconnected())
            .map(|(id, _)| id)
    }
}

impl<'a> ProjectView<'a> {
    /// How this view's entities are linked to one another.
    pub fn connectivity(&self) -> Connectivity {
        Connectivity::of(self.graph())
    }

    /// The entities [`Graph::reach`] reaches from `entity_id` within `depth`
    /// hops (unbounded when `None`; 0 is the entity alone), the entity first,
    /// nearest first: the one neighbourhood the exploration and the review
    /// read. An entity the graph lacks is `view::not_found` (E003,
    /// did-you-mean).
    pub fn neighbourhood(
        &self,
        entity_id: &str,
        depth: Option<usize>,
    ) -> Result<Vec<Reached>, Refusal<EntityField>> {
        self.graph()
            .reach(entity_id, depth)
            .ok_or_else(|| not_found(self.graph(), entity_id).about(EntityField::EntityId))
    }
}
