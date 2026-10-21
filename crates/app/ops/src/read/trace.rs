//! `specforge trace` and `specforge.trace`: traceability chains, one
//! operation over the project view (ADR 0015), and the vocabulary of what
//! falls short ([`Gap`]: a missing link, or a plan gap).

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_graph::{Direction, Graph, Node};
use specforge_registry::RegistryBuild;
use std::collections::{BTreeMap, BTreeSet};

mod ask;

pub use ask::{Ask, ENTITY_WITH_PLAN, TraceField};

use crate::read::plan::PlanGap;
use crate::view::{EntityField, ProjectView};
use crate::{Noticed, OpError, Refusal};

#[derive(Debug, Serialize, Shape)]
pub struct TraceChain {
    pub entity_id: String,
    pub entity_kind: String,
    pub upstream: Vec<TraceLink>,
    pub downstream: Vec<TraceLink>,
    /// Expected edges the traced entity does not have. Only the traced
    /// entity's: each entity's own trace reports its gaps, so a trace of
    /// every entity lists each gap once, and a trace of one entity is not
    /// buried under the gaps of everything below it.
    pub missing: Vec<MissingLink>,
}

impl TraceChain {
    /// The traced entity and every entity its chain links.
    fn reached(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.entity_id.as_str()).chain(
            self.upstream
                .iter()
                .chain(&self.downstream)
                .map(|link| link.entity_id.as_str()),
        )
    }
}

/// Whether a link of a chain exists in the graph (`TraceLinkStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "lowercase")]
pub enum TraceLinkStatus {
    Resolved,
    Missing,
}

#[derive(Debug, Serialize, Shape)]
#[shape(define)]
pub struct TraceLink {
    pub entity_id: String,
    pub entity_kind: String,
    pub edge_label: String,
    pub depth: usize,
    pub status: TraceLinkStatus,
}

/// An edge the registries lead an entity of the chain to have, which the
/// graph does not instantiate. Always has status `missing`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct MissingLink {
    /// The entity that lacks the edge.
    pub from: String,
    pub from_kind: String,
    /// The field that would declare the edge — the graph's edge label.
    pub edge_label: String,
    /// The registered edge type the field instantiates, if it names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_type: Option<String>,
    /// The kind the edge would point at.
    pub expected_kind: String,
    /// Always 1: the link would leave the traced entity (depth 0).
    pub depth: usize,
    /// The field is required, not just declared.
    pub required: bool,
    pub status: TraceLinkStatus,
}

/// An outgoing edge one kind is expected to have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedEdge {
    /// The field that declares the edge, used as the graph's edge label.
    pub label: String,
    pub edge_type: Option<String>,
    pub target_kind: String,
    pub required: bool,
}

/// The edges each kind is expected to have, derived from the registries
/// only — the core knows nothing about any kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceExpectations {
    by_kind: BTreeMap<String, Vec<ExpectedEdge>>,
}

impl TraceExpectations {
    /// No expectations: traces flag nothing as missing.
    pub fn new() -> Self {
        Self::default()
    }

    /// A required reference field is always an expected edge. Otherwise a
    /// reference field is expected when:
    /// - the kind it points at is loaded: a field whose target no loaded
    ///   extension declares is an optional peer;
    /// - the extension that declares the kind declares the field: fields
    ///   another extension adds to the kind (entity enhancements) are
    ///   opt-in;
    /// - it points at another kind: a self-relation (`depends_on`,
    ///   `extends`, `superseded_by`...) is a hierarchy or ordering whose
    ///   roots or leaves lack it by construction.
    pub fn of(build: &RegistryBuild) -> Self {
        let (fields, kinds) = (build.fields(), build.kinds());
        let mut expectations = Self::new();
        for (kind, field, entry) in fields.iter() {
            let Some(target) = entry.declared().target_kind.as_deref() else {
                continue;
            };
            let is_reference = entry.field_type().is_reference();
            let own_field = kinds
                .get(kind)
                .is_some_and(|k| k.source_extension == entry.source_extension());
            if !is_reference
                || !kinds.contains(target)
                || !(own_field || entry.declared().required)
                || (target == kind && !entry.declared().required)
            {
                continue;
            }
            expectations.expect(
                kind,
                ExpectedEdge {
                    label: field.to_string(),
                    edge_type: build
                        .edges()
                        .written_by(kind, field)
                        .map(|edge| edge.label().to_string()),
                    target_kind: target.to_string(),
                    required: entry.declared().required,
                },
            );
        }
        expectations
    }

    /// Expect entities of `kind` to have `edge`.
    pub fn expect(&mut self, kind: &str, edge: ExpectedEdge) {
        let edges = self.by_kind.entry(kind.to_string()).or_default();
        edges.push(edge);
        edges.sort_by(|a, b| a.label.cmp(&b.label));
        edges.dedup_by(|a, b| a.label == b.label);
    }

    /// The edges `kind` is expected to have, ordered by label.
    pub fn for_kind(&self, kind: &str) -> &[ExpectedEdge] {
        self.by_kind.get(kind).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.by_kind.is_empty()
    }
}

/// What a trace request asks to trace (`Ask::Trace`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'t> {
    /// One entity's chain.
    Entity(&'t str),
    /// Every entity's chain, ordered by entity id.
    Every,
}

/// Every entity's chain, ordered by entity id: the document `specforge
/// trace --format json` writes with no entity, `{schema_version, traces}`.
#[derive(Debug, Serialize)]
pub struct Traces {
    pub schema_version: String,
    pub traces: Vec<TraceChain>,
}

/// What a trace computed, as the document it serializes as: one entity's
/// chain, `{schema_version, entity_id, entity_kind, upstream, downstream,
/// missing}`, or every entity's, `{schema_version, traces}`.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum TraceOutcome {
    Chain(ChainDocument<TraceChain>),
    Every(Traces),
}

impl TraceOutcome {
    /// Its chains: the traced entity's, or every entity's in id order.
    pub fn chains(&self) -> &[TraceChain] {
        match self {
            TraceOutcome::Chain(document) => std::slice::from_ref(&document.chain),
            TraceOutcome::Every(every) => &every.traces,
        }
    }
}

/// One entity's chain as the document `specforge trace <entity> --format
/// json` writes: `{schema_version, entity_id, entity_kind, upstream,
/// downstream, missing}`.
#[derive(Debug, Serialize, Shape)]
pub struct ChainDocument<C> {
    pub schema_version: String,
    #[serde(flatten)]
    pub chain: C,
}

/// Trace `target` in the view's graph: [`chain`] for one entity, [`every`]
/// for all.
pub fn trace(
    view: &ProjectView,
    target: Target,
) -> Result<Noticed<TraceOutcome>, Refusal<EntityField>> {
    match target {
        Target::Entity(entity_id) => Ok(chain(view, entity_id)?.map(TraceOutcome::Chain)),
        Target::Every => Ok(every(view).map(TraceOutcome::Every)),
    }
}

/// The chain of `entity_id` both ways, with every edge the registries lead
/// it to have, and it lacks, flagged as missing. Its notices are what
/// versioning the schema reported (I016, W053; ADR 0111). An entity the
/// graph lacks is `view::not_found` (E003, with its did-you-mean).
pub fn chain(
    view: &ProjectView,
    entity_id: &str,
) -> Result<Noticed<ChainDocument<TraceChain>>, Refusal<EntityField>> {
    let expectations = TraceExpectations::of(view.registries());
    let graph = view.graph();
    let versioned = view.versioned_schema();
    let node = view.entity(entity_id, EntityField::EntityId)?.node;
    let chain = traced(graph, node, &expectations);
    Ok(Noticed::new(
        ChainDocument {
            schema_version: versioned.version().to_string(),
            chain,
        },
        versioned.notices(),
    ))
}

/// Every entity's chain, ordered by entity id; the notices as [`chain`]'s.
pub fn every(view: &ProjectView) -> Noticed<Traces> {
    let expectations = TraceExpectations::of(view.registries());
    let graph = view.graph();
    let versioned = view.versioned_schema();
    let mut traces: Vec<TraceChain> = graph
        .nodes()
        .iter()
        .map(|node| traced(graph, node, &expectations))
        .collect();
    traces.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));
    Noticed::new(
        Traces {
            schema_version: versioned.version().to_string(),
            traces,
        },
        versioned.notices(),
    )
}

/// What the trace chains of some entities reach, and which of it is
/// unverified: the trace prompt's read view (ADR 0015 D4, P1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Affected {
    /// The seeds and every entity their chains reach, sorted by ID.
    pub entities: Vec<String>,
    /// The affected entities that count toward coverage and are not
    /// proven (`ProjectCoverage::is_unverified`), sorted by ID.
    pub unverified: Vec<String>,
}

/// The entities affected by `seeds` over `view`: each seed's chain as
/// `trace(view, Target::Entity(seed))` computes it. A seed the graph lacks
/// is `view::not_found` (E003, with its
/// did-you-mean); a recorded report that cannot be used is the coverage
/// view's (E045).
pub fn affected<'s>(
    view: &ProjectView,
    seeds: impl IntoIterator<Item = &'s str>,
) -> Result<Affected, OpError> {
    let expectations = TraceExpectations::of(view.registries());
    let graph = view.graph();
    let mut entities = BTreeSet::new();
    for seed in seeds {
        let node = view
            .entity(seed, EntityField::EntityId)
            .map_err(|refusal| refusal.error)?
            .node;
        let chain = traced(graph, node, &expectations);
        entities.extend(chain.reached().map(str::to_string));
    }
    let coverage = view.coverage()?;
    let unverified = entities
        .iter()
        .filter(|id| coverage.is_unverified(id))
        .cloned()
        .collect();
    Ok(Affected {
        entities: entities.into_iter().collect(),
        unverified,
    })
}

/// The chain of `entity_id` both ways, with every expected edge it lacks
/// flagged as missing.
fn traced(graph: &Graph, root: &Node, expectations: &TraceExpectations) -> TraceChain {
    let entity_id = root.id.raw.as_str();
    TraceChain {
        entity_id: entity_id.to_string(),
        entity_kind: root.kind.raw.to_string(),
        upstream: links(graph, entity_id, Direction::Incoming),
        downstream: links(graph, entity_id, Direction::Outgoing),
        missing: missing_links(graph, entity_id, expectations),
    }
}

/// What falls short, in one vocabulary (CONTEXT.md): a missing link, an
/// expected edge a traced entity lacks; or a plan gap, how an agent plan
/// falls short of the graph. An edge to an entity that does not exist is
/// neither: it is E003, `check`'s business.
#[derive(Debug, Clone, PartialEq)]
pub enum Gap {
    MissingLink(MissingLink),
    Plan(PlanGap),
}

impl Gap {
    /// The entity that falls short (`plan` for the plan itself).
    pub fn source(&self) -> &str {
        match self {
            Gap::MissingLink(link) => &link.from,
            Gap::Plan(gap) => &gap.source,
        }
    }

    /// What it falls short of: the kind a missing link would point at, or
    /// the entity a plan gap names.
    pub fn target(&self) -> &str {
        match self {
            Gap::MissingLink(link) => &link.expected_kind,
            Gap::Plan(gap) => &gap.target,
        }
    }

    /// `missing_link`, or the plan gap's kind (`unresolved_entity`,
    /// `missing_plan_entry`, `ordering`).
    pub fn kind(&self) -> &'static str {
        match self {
            Gap::MissingLink(_) => "missing_link",
            Gap::Plan(gap) => gap.kind.as_str(),
        }
    }

    /// The gap in a sentence.
    pub fn context(&self) -> String {
        match self {
            Gap::MissingLink(link) => format!(
                "{} '{}' has no '{}' link to a {}{}",
                link.from_kind,
                link.from,
                link.edge_label,
                link.expected_kind,
                if link.required { " (required)" } else { "" }
            ),
            Gap::Plan(gap) => gap.context.clone(),
        }
    }
}

/// The expected edges `entity_id` does not have, one hop from it. An edge
/// with the expected label counts even when its target is undeclared: that
/// is a broken reference (E003), not a missing link.
fn missing_links(
    graph: &Graph,
    entity_id: &str,
    expectations: &TraceExpectations,
) -> Vec<MissingLink> {
    let Some(node) = graph.node(entity_id) else {
        return Vec::new();
    };
    let kind = node.kind.raw.as_str();
    let outgoing = graph.edges_from(entity_id);
    expectations
        .for_kind(kind)
        .iter()
        .filter(|expected| !outgoing.iter().any(|e| e.label == *expected.label))
        .map(|expected| MissingLink {
            from: entity_id.to_string(),
            from_kind: kind.to_string(),
            edge_label: expected.label.clone(),
            edge_type: expected.edge_type.clone(),
            expected_kind: expected.target_kind.clone(),
            depth: 1,
            required: expected.required,
            status: TraceLinkStatus::Missing,
        })
        .collect()
}

/// `entity_id`'s links one way: what the graph's directed reach reaches, by depth, then id.
fn links(graph: &Graph, entity_id: &str, direction: Direction) -> Vec<TraceLink> {
    let mut links: Vec<TraceLink> = graph
        .reach_directed(entity_id, direction)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|reached| {
            let (_, label) = reached.via?; // the root has none
            let node = graph.node(reached.id.as_str())?;
            Some(TraceLink {
                entity_id: reached.id.to_string(),
                entity_kind: node.kind.raw.to_string(),
                edge_label: label.to_string(),
                depth: reached.depth,
                status: TraceLinkStatus::Resolved,
            })
        })
        .collect();
    links.sort_by(|a, b| a.depth.cmp(&b.depth).then(a.entity_id.cmp(&b.entity_id)));
    links
}
