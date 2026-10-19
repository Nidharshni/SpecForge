//! How many edges link an entity to *other* entities (CONTEXT.md "Unconnected entity").

use specforge_common::Sym;
use std::collections::HashMap;

use super::Graph;

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

impl Graph {
    /// `id`'s degree, from the graph's index: O(its edges). Counts the edges at `id` whether or not a node declares
    /// it (the zero degree when there are none).
    pub fn degree(&self, id: &str) -> Degree {
        let id = Sym::new(id);
        let others = |index: &HashMap<Sym, Vec<usize>>| {
            index.get(&id).map_or(0, |indices| {
                indices
                    .iter()
                    .filter(|&&i| self.edges[i].source != self.edges[i].target)
                    .count()
            })
        };
        Degree {
            incoming: others(&self.target_index),
            outgoing: others(&self.source_index),
        }
    }
}
