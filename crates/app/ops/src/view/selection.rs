//! A read's selection (CONTEXT.md "Read selection", ADR 0177): the entity a request names, the hops
//! counted from it and the kinds it keeps, checked once by the project view for every read view, and the entities
//! it covers (ADR 0190).

use specforge_common::Diagnostic;
use specforge_graph::{Graph, Node, Selected};
use specforge_registry::{EntityRecord, Standing};

use super::{KindFilter, ProjectView, not_found};
use crate::{OpError, OpErrorKind, Refusal};

use super::entity::DEPTH_WITHOUT_ENTITY;

/// One entity of the view: its node, and its record and standing in the view's entity snapshot
/// (ADR 0019).
#[derive(Debug, Clone, Copy)]
pub struct Entity<'v> {
    pub node: &'v Node,
    pub record: &'v EntityRecord,
    pub standing: &'v Standing,
}

/// What a read asks to select. No entity: every entity; no kinds: every kind.
#[derive(Debug, Clone, Copy)]
pub struct Select<'r, F> {
    /// The entity the read is about, or counts hops from.
    pub entity: Option<&'r str>,
    /// The request field that names the entity: an entity the graph lacks, and a depth without
    /// one, are refused about it (ADR 0072).
    pub named_by: F,
    /// Hops from the entity. Refused without one.
    pub depth: Option<usize>,
    /// The kinds kept ([`ProjectView::kind_filter`]).
    pub kinds: &'r [&'r str],
}

/// A checked selection: its entity resolved, its depth anchored, its kinds with their notices.
pub struct Selection<'v, 'r> {
    pub entity: Option<Entity<'v>>,
    depth: Option<usize>,
    kinds: KindFilter<'r>,
    graph: &'v Graph,
}

impl<'r> Selection<'_, 'r> {
    /// The filter as the request gave it, unknown kinds included: what the emitter's
    /// `kind_filter` takes.
    pub fn kinds(&self) -> &'r [&'r str] {
        self.kinds.kinds()
    }

    /// The entities the selection covers ([`Graph::select`], ADR 0190): what its entity reaches within the depth the
    /// request gave, else `default` (`None`: unbounded), or every entity without one; of its kinds. Coverage reads it
    /// at `Some(0)` (the entity alone), the review at `Some(DEFAULT_DEPTH)`, explore and export at `None`.
    pub fn selected(&self, default: Option<usize>) -> Selected {
        self.graph
            .select(
                self.entity.map(|entity| entity.node.id.raw.as_str()),
                self.depth.or(default),
                self.kinds.kinds(),
            )
            .expect("a selected entity is a node of the view's graph")
    }

    /// I020 for each kind the project lacks, once each, in the filter's order.
    pub fn into_notices(self) -> Vec<Diagnostic> {
        self.kinds.into_notices()
    }
}

impl<'a> ProjectView<'a> {
    /// The entity `id`, or [`not_found`] (E003, did-you-mean) about `named_by`.
    pub fn entity<F>(&self, id: &str, named_by: F) -> Result<Entity<'a>, Refusal<F>> {
        let graph = self.graph();
        match graph.node(id).zip(self.entities().get(id)) {
            Some((node, (record, standing))) => Ok(Entity {
                node,
                record,
                standing,
            }),
            None => Err(not_found(graph, id).about(named_by)),
        }
    }

    /// The selection `select` asks for. A depth without an entity is `depth_without_entity`
    /// (invalid input) about `named_by`, checked first; then the entity ([`Self::entity`]); kinds
    /// never refuse.
    pub fn select<'r, F>(&self, select: &Select<'r, F>) -> Result<Selection<'a, 'r>, Refusal<F>>
    where
        F: Copy,
    {
        let entity = match (select.entity, select.depth) {
            (None, Some(_)) => return Err(depth_without_entity(select.named_by)),
            (None, None) => None,
            (Some(id), _) => Some(self.entity(id, select.named_by)?),
        };
        Ok(Selection {
            entity,
            depth: select.depth,
            kinds: self.kind_filter(select.kinds),
            graph: self.graph(),
        })
    }
}

fn depth_without_entity<F>(named_by: F) -> Refusal<F> {
    OpError::new(
        OpErrorKind::InvalidInput,
        DEPTH_WITHOUT_ENTITY,
        "a depth counts hops from an entity: give an entity too",
    )
    .about(named_by)
}
