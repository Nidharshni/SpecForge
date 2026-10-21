//! How the entities of a project view are linked: the one rule for an
//! unconnected entity (CONTEXT.md "Unconnected entity", `specforge_graph::Degree`),
//! the degrees the exploration ranks by, and the neighbourhood of one entity that the
//! exploration and the review share (ADR 0015, "Prompt read views").

use std::collections::BTreeMap;

use specforge_common::Sym;
use specforge_graph::{Degree, Graph};

use super::ProjectView;

/// The degree of every entity of one graph (`specforge_graph::Degree`).
#[derive(Debug, Clone, Default)]
pub struct Connectivity {
    /// Every node of the graph, in id order (Sym orders by content).
    degrees: BTreeMap<Sym, Degree>,
}

impl Connectivity {
    /// The degrees of `graph`'s entities (`Graph::degree`), every node in id order.
    pub fn of(graph: &Graph) -> Self {
        Connectivity {
            degrees: graph
                .nodes()
                .into_iter()
                .map(|node| (node.id.raw, graph.degree(node.id.raw.as_str())))
                .collect(),
        }
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
}
