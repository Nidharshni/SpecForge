//! The entities a read selection covers (CONTEXT.md "Read selection", ADR 0190).

use specforge_common::Sym;
use std::collections::BTreeSet;

use super::{Graph, Reached};

/// The entities a read selection covers: what its root reaches within its depth over edges both ways
/// ([`Graph::reach`]), or every entity without a root, of the kinds it keeps (none: every kind). The root is the
/// entity the read is about: a member only when its kind is kept, and always in a subgraph a read draws
/// ([`Selected::drawn`]: an export, a query), which a token budget never cuts.
#[derive(Debug, Clone)]
pub struct Selected {
    root: Option<Sym>,
    /// Hops from the root as the read asked (`None`: unbounded); `None` without a root.
    depth: Option<usize>,
    /// The kinds as the read gave them, unknown ones included (a kind no entity has keeps nothing).
    kinds: Vec<Sym>,
    /// What the root reached, nearest first; empty without a root.
    reached: Vec<Reached>,
    ids: BTreeSet<Sym>,
}

impl Graph {
    /// The selection of `root` (within `depth` hops) and `kinds`. `None` when `root` is not a node. Without a root,
    /// `depth` is ignored.
    pub fn select(
        &self,
        root: Option<&str>,
        depth: Option<usize>,
        kinds: &[&str],
    ) -> Option<Selected> {
        let keeps = |kind: &str| kinds.is_empty() || kinds.contains(&kind);
        let kept = |id: Sym| {
            self.nodes
                .get(&id)
                .is_some_and(|n| keeps(n.kind.raw.as_str()))
        };
        let reached = match root {
            Some(root) => self.reach(root, depth)?,
            None => Vec::new(),
        };
        let ids = match root {
            Some(_) => reached
                .iter()
                .map(|r| r.id)
                .filter(|&id| kept(id))
                .collect(),
            None => self.nodes.keys().copied().filter(|&id| kept(id)).collect(),
        };
        Some(Selected {
            root: root.map(Sym::new),
            depth: root.and(depth),
            kinds: kinds.iter().map(|k| Sym::new(k)).collect(),
            reached,
            ids,
        })
    }
}

impl Selected {
    /// The entity the read is about; `None` for a selection of every entity.
    pub fn root(&self) -> Option<Sym> {
        self.root
    }

    /// Hops from the root as the read asked (`None`: unbounded, or no root).
    pub fn depth(&self) -> Option<usize> {
        self.depth
    }

    /// The kinds as the read gave them.
    pub fn kinds(&self) -> &[Sym] {
        &self.kinds
    }

    /// What the root reached, nearest first, each with the edge it was reached by (paths: [`Graph::reach_path`]).
    pub fn reached(&self) -> &[Reached] {
        &self.reached
    }

    /// `id` is selected.
    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(&Sym::new(id))
    }

    /// The selected entities, in id order.
    pub fn ids(&self) -> impl Iterator<Item = Sym> + '_ {
        self.ids.iter().copied()
    }

    /// What a drawn subgraph holds: the selected entities and the root.
    pub fn drawn(&self) -> impl Iterator<Item = Sym> + '_ {
        self.ids
            .iter()
            .copied()
            .chain(self.root.filter(|root| !self.ids.contains(root)))
    }

    /// It selects every entity and every edge: no root and no kinds. An export of it is the graph as it is.
    pub fn is_whole(&self) -> bool {
        self.root.is_none() && self.kinds.is_empty()
    }
}
