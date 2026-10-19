//! Breadth-first reach over the graph's index: both ways (`Graph::reach`) or one way (`Graph::reach_directed`).

use specforge_common::Sym;
use std::collections::HashSet;

use super::{Edge, Graph, Reached};

/// Which edges a directed reach follows from an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// The edges into it, back to their sources: what references it (a trace's upstream).
    Incoming,
    /// The edges out of it, to their targets: what it references (a trace's downstream).
    Outgoing,
}

impl Graph {
    /// Breadth-first from `root_id` over the edges one way, nearest first. Each entity's edges are taken in the order
    /// the graph holds them (written order), so a reached entity's depth is its distance from the root and its
    /// [`Reached::via`] is the first edge, in that order, from the first entity reached that is one hop from it. Only
    /// nodes of the graph are reached; `None` when `root_id` is not a node. Reads only the edges of the entities it
    /// reaches.
    pub fn reach_directed(&self, root_id: &str, direction: Direction) -> Option<Vec<Reached>> {
        self.breadth_first(root_id, None, |id| self.one_way(id, direction))
    }

    /// `id`'s neighbours over its edges one way, each with the edge's label, in written order.
    fn one_way(&self, id: Sym, direction: Direction) -> Vec<(Sym, Sym)> {
        let (index, far): (_, fn(&Edge) -> Sym) = match direction {
            Direction::Incoming => (&self.target_index, |edge| edge.source),
            Direction::Outgoing => (&self.source_index, |edge| edge.target),
        };
        index.get(&id).map_or_else(Vec::new, |indices| {
            indices
                .iter()
                .map(|&i| (far(&self.edges[i]), self.edges[i].label))
                .collect()
        })
    }

    /// The one breadth-first walk: from `root_id`, nearest first, each popped entity's `neighbours` in the order
    /// given, only nodes, nothing beyond `max_depth` hops (unbounded when `None`). `None` when `root_id` is not a node.
    pub(super) fn breadth_first(
        &self,
        root_id: &str,
        max_depth: Option<usize>,
        mut neighbours: impl FnMut(Sym) -> Vec<(Sym, Sym)>,
    ) -> Option<Vec<Reached>> {
        let root = Sym::new(root_id);
        if !self.nodes.contains_key(&root) {
            return None;
        }
        let mut reached = vec![Reached {
            id: root,
            depth: 0,
            via: None,
        }];
        let mut seen: HashSet<Sym> = HashSet::from([root]);
        let mut next = 0;
        while let Some(&Reached { id, depth, .. }) = reached.get(next) {
            next += 1;
            if max_depth.is_some_and(|max| depth >= max) {
                continue;
            }
            for (neighbor, label) in neighbours(id) {
                if self.nodes.contains_key(&neighbor) && seen.insert(neighbor) {
                    reached.push(Reached {
                        id: neighbor,
                        depth: depth + 1,
                        via: Some((id, label)),
                    });
                }
            }
        }
        Some(reached)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::SourceSpan;
    use specforge_test_macros::test as specforge_test;

    use crate::Node;

    fn node(id: &str) -> Node {
        let span = SourceSpan {
            file: Sym::new("test.spec"),
            start_line: 1,
            start_col: 1,
            end_line: 1,
            end_col: 1,
        };
        Node::new(id, "behavior", span)
    }

    fn edge(source: &str, target: &str) -> Edge {
        Edge {
            source: Sym::new(source),
            target: Sym::new(target),
            label: Sym::new("l"),
        }
    }

    #[specforge_test(
        invariant = "graph_traversal_integrity",
        verify = "a directed reach reads only the edges of the entities it reaches"
    )]
    fn a_directed_reach_reads_only_the_edges_of_what_it_reaches() {
        // c0 -> c1 -> ... -> c99, and 300 entities d_i with 100 edges each among themselves (30,000 edges)
        let mut graph = Graph::new();
        for i in 0..100 {
            graph.add_node(node(&format!("c{i}")));
        }
        for i in 0..99 {
            graph.add_edge(edge(&format!("c{i}"), &format!("c{}", i + 1)));
        }
        for i in 0..300 {
            graph.add_node(node(&format!("d{i}")));
        }
        for i in 0..300 {
            for k in 1..=100 {
                graph.add_edge(edge(&format!("d{i}"), &format!("d{}", (i + k) % 300)));
            }
        }
        for (root, direction, far) in [
            ("c0", Direction::Outgoing, 99),
            ("c99", Direction::Incoming, 99),
        ] {
            let mut visits = 0;
            let counted = graph
                .breadth_first(root, None, |id| {
                    let next = graph.one_way(id, direction);
                    visits += next.len();
                    next
                })
                .unwrap();
            assert_eq!(counted, graph.reach_directed(root, direction).unwrap());
            assert_eq!(counted.len(), 100);
            assert_eq!(
                visits, far,
                "one edge per chain entity but the last; the 30,000 others are never read"
            );
        }
    }
}
