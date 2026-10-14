//! `specforge trace` and `specforge.trace`: traceability chains, one
//! operation over the project view (ADR 0015), and the vocabulary of what
//! falls short ([`Gap`]: a missing link, or a plan gap).

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use specforge_common::shape::Shape;
use specforge_graph::Graph;
use specforge_registry::RegistryBuild;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use crate::read::plan::PlanGap;
use crate::view::{EntityField, ProjectView};
use crate::{OpError, Refusal};

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
    /// Labels of the reverse field: an edge with one of them, from an
    /// entity of `target_kind` to this one, is the same relationship
    /// declared from the other side.
    pub inverse_labels: Vec<String>,
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
            let inverse_labels = fields
                .inverse_fields(kind, field)
                .into_iter()
                .map(str::to_string)
                .collect();
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
                    inverse_labels,
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

/// What to trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'t> {
    /// One entity's chain.
    Entity(&'t str),
    /// Every entity's chain, ordered by entity id.
    Every,
}

/// The chains a trace computed. It serializes as the document `specforge
/// trace --format json` writes: one chain as `{schema_version, entity_id,
/// entity_kind, upstream, downstream, missing}`, every chain as
/// `{schema_version, traces}`.
#[derive(Debug)]
pub struct TraceOutcome {
    pub chains: Vec<TraceChain>,
    /// The project's versioned schema version, as `specforge export` and
    /// `specforge schema` state it.
    schema_version: String,
    /// One entity was traced (else every one).
    single: bool,
}

impl Serialize for TraceOutcome {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match (self.single, self.chains.first()) {
            (true, Some(chain)) => ChainDocument {
                schema_version: self.schema_version.clone(),
                chain,
            }
            .serialize(serializer),
            _ => {
                let mut doc = serializer.serialize_struct("TraceAll", 2)?;
                doc.serialize_field("schema_version", &self.schema_version)?;
                doc.serialize_field("traces", &self.chains)?;
                doc.end()
            }
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

impl TraceOutcome {
    /// The traced entity's chain as its document, for a trace of one entity
    /// (`None` for a trace of every entity).
    pub fn into_chain(self) -> Option<ChainDocument<TraceChain>> {
        let single = self.single;
        let schema_version = self.schema_version;
        self.chains
            .into_iter()
            .next()
            .filter(|_| single)
            .map(|chain| ChainDocument {
                schema_version,
                chain,
            })
    }

    /// The chains as terminal text, one block per chain ([`render_human`]),
    /// separated by a blank line.
    pub fn to_human(&self) -> String {
        self.chains
            .iter()
            .map(render_human)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The missing links of every chain, as gaps.
    pub fn gaps(&self) -> Vec<Gap> {
        self.chains
            .iter()
            .flat_map(|chain| chain.missing.iter().cloned().map(Gap::MissingLink))
            .collect()
    }
}

/// Why a trace could not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceError {
    /// The entity to trace is not in the graph; `near` is the closest id.
    EntityNotFound {
        entity_id: String,
        near: Option<String>,
    },
}

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceError::EntityNotFound { entity_id, .. } => {
                write!(f, "unresolved entity '{entity_id}' — not found in graph")
            }
        }
    }
}

impl std::error::Error for TraceError {}

/// `view::not_found`: E003, its message, and a did-you-mean when an
/// entity is close.
impl From<TraceError> for OpError {
    fn from(error: TraceError) -> Self {
        Refusal::<EntityField>::from(error).into_error()
    }
}

/// [`OpError::from`], about the entity traced.
impl From<TraceError> for Refusal<EntityField> {
    fn from(error: TraceError) -> Self {
        match error {
            TraceError::EntityNotFound { entity_id, near } => {
                crate::view::unresolved(&entity_id, near.as_deref()).about(EntityField::EntityId)
            }
        }
    }
}

/// Trace `target` in the view's graph: each chain both ways, with every
/// edge the registries lead the traced entity to have, and it lacks,
/// flagged as missing.
pub fn trace(view: &ProjectView, target: Target) -> Result<TraceOutcome, TraceError> {
    let expectations = TraceExpectations::of(view.registries());
    let graph = view.graph();
    let schema_version = view.versioned_schema().version().to_string();
    match target {
        Target::Entity(entity_id) => {
            let chain = chain(graph, entity_id, &expectations)
                .ok_or_else(|| entity_not_found(graph, entity_id))?;
            Ok(TraceOutcome {
                chains: vec![chain],
                schema_version,
                single: true,
            })
        }
        Target::Every => {
            let mut chains: Vec<TraceChain> = graph
                .nodes()
                .iter()
                .filter_map(|n| chain(graph, n.id.raw.as_str(), &expectations))
                .collect();
            chains.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));
            Ok(TraceOutcome {
                chains,
                schema_version,
                single: false,
            })
        }
    }
}

/// The refusal for an entity the graph lacks, with its did-you-mean.
fn entity_not_found(graph: &Graph, entity_id: &str) -> TraceError {
    TraceError::EntityNotFound {
        entity_id: entity_id.to_string(),
        near: specforge_common::suggest::find_close_match(
            entity_id,
            graph.nodes().iter().map(|n| n.id.raw.as_str()),
        )
        .map(str::to_string),
    }
}

/// What the trace chains of some entities reach, and which of it is
/// unverified: the trace prompt's read view (ADR 0015 D4, P1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reach {
    /// The seeds and every entity their chains reach, sorted by ID.
    pub affected: Vec<String>,
    /// The affected entities that count toward coverage and are not
    /// proven (`ProjectCoverage::is_unverified`), sorted by ID.
    pub unverified: Vec<String>,
}

/// The reach of `seeds` over `view`: each seed's chain as
/// `trace(view, Target::Entity(seed))` computes it. A seed the graph lacks
/// is the trace's refusal (`TraceError::EntityNotFound`, with its
/// did-you-mean); a recorded report that cannot be used is the coverage
/// view's (E045).
pub fn reach<'s>(
    view: &ProjectView,
    seeds: impl IntoIterator<Item = &'s str>,
) -> Result<Reach, OpError> {
    let expectations = TraceExpectations::of(view.registries());
    let graph = view.graph();
    let mut affected = BTreeSet::new();
    for seed in seeds {
        let chain = chain(graph, seed, &expectations)
            .ok_or_else(|| OpError::from(entity_not_found(graph, seed)))?;
        affected.extend(chain.reached().map(str::to_string));
    }
    let coverage = view.coverage()?;
    let unverified = affected
        .iter()
        .filter(|id| coverage.is_unverified(id))
        .cloned()
        .collect();
    Ok(Reach {
        affected: affected.into_iter().collect(),
        unverified,
    })
}

/// The chain of `entity_id` both ways, with every expected edge it lacks
/// flagged as missing; `None` when the graph lacks it.
fn chain(graph: &Graph, entity_id: &str, expectations: &TraceExpectations) -> Option<TraceChain> {
    let root = graph.node(entity_id)?;
    Some(TraceChain {
        entity_id: entity_id.to_string(),
        entity_kind: root.kind.raw.to_string(),
        upstream: directed_bfs(graph, entity_id, Direction::Upstream),
        downstream: directed_bfs(graph, entity_id, Direction::Downstream),
        missing: missing_links(graph, entity_id, expectations),
    })
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
    let incoming = graph.edges_to(entity_id);
    expectations
        .for_kind(kind)
        .iter()
        .filter(|expected| {
            let declared = outgoing.iter().any(|e| e.label == *expected.label);
            let declared_back = incoming.iter().any(|e| {
                expected.inverse_labels.iter().any(|l| e.label == **l)
                    && graph
                        .node(e.source.as_str())
                        .is_some_and(|source| source.kind.raw == *expected.target_kind)
            });
            !declared && !declared_back
        })
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

/// A chain as terminal text: the entity, then its upstream, downstream and
/// missing links, one per line. Missing links are marked `MISSING`.
fn render_human(chain: &TraceChain) -> String {
    let mut out = format!("{} [{}]\n", chain.entity_id, chain.entity_kind);
    let section = |out: &mut String, name: &str, links: &[TraceLink], arrow: fn(&str) -> String| {
        out.push_str(&format!("  {name}:\n"));
        if links.is_empty() {
            out.push_str("    (none)\n");
        }
        for link in links {
            out.push_str(&format!(
                "    {} {} [{}] (depth {})\n",
                arrow(&link.edge_label),
                link.entity_id,
                link.entity_kind,
                link.depth
            ));
        }
    };
    section(&mut out, "upstream", &chain.upstream, |l| format!("<-{l}-"));
    section(&mut out, "downstream", &chain.downstream, |l| {
        format!("-{l}->")
    });
    if !chain.missing.is_empty() {
        out.push_str("  missing:\n");
        for link in &chain.missing {
            out.push_str(&format!(
                "    MISSING {} -{}-> [{}]{} (depth {})\n",
                link.from,
                link.edge_label,
                link.expected_kind,
                if link.required { " required" } else { "" },
                link.depth
            ));
        }
    }
    out
}

enum Direction {
    Upstream,
    Downstream,
}

fn directed_bfs(graph: &Graph, start: &str, direction: Direction) -> Vec<TraceLink> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut links = Vec::new();

    visited.insert(start.to_string());
    queue.push_back((start.to_string(), 0usize));

    while let Some((current, depth)) = queue.pop_front() {
        for edge in graph.edges() {
            let (neighbor, label) = match direction {
                Direction::Upstream => {
                    if edge.target == *current {
                        (&edge.source, &edge.label)
                    } else {
                        continue;
                    }
                }
                Direction::Downstream => {
                    if edge.source == *current {
                        (&edge.target, &edge.label)
                    } else {
                        continue;
                    }
                }
            };

            if visited.insert(neighbor.to_string())
                && let Some(node) = graph.node(neighbor.as_str())
            {
                links.push(TraceLink {
                    entity_id: neighbor.to_string(),
                    entity_kind: node.kind.raw.to_string(),
                    edge_label: label.to_string(),
                    depth: depth + 1,
                    status: TraceLinkStatus::Resolved,
                });
                queue.push_back((neighbor.to_string(), depth + 1));
            }
        }
    }

    links.sort_by(|a, b| a.depth.cmp(&b.depth).then(a.entity_id.cmp(&b.entity_id)));
    links
}
