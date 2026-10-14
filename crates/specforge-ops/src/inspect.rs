//! Inspect: what one entity is, as a read view over the project view
//! (ADR 0015, section "Inspect"). MCP `specforge.inspect` renders it as
//! JSON, the LSP hover as markdown, the context prompt as its payload;
//! none of them reads the graph or the registries for it, and those that
//! show coverage ask [`coverage`] for it.

use specforge_common::Diagnostic;
use specforge_graph::Node;
use specforge_project::coverage::{Status, Verdict};
use specforge_project::snapshot::{ObligationRecord, Standing};
use specforge_registry::KindRegistryEntry;

use crate::navigate::{EntityField, References, is_about, not_found};
use crate::view::ProjectView;
use crate::{OpError, Refusal};

/// Everything a surface shows about one entity, borrowed from the view.
#[derive(Debug, Clone)]
pub struct EntityFacts<'v> {
    /// The entity: id, kind, title, block span, and its fields in
    /// declaration order (a struct member named `verify` and the verify
    /// statements are two entries).
    pub node: &'v Node,
    /// Its kind's registry entry: the declaring extension, the kind's
    /// description, `supports_verify`, `singleton`. `None` for a kind no
    /// loaded extension declares.
    pub kind: Option<&'v KindRegistryEntry>,
    /// The statement its extension declares headline and normative (a
    /// behavior's `contract`), borrowed from the node's field; `None` when
    /// its kind declares none.
    pub headline: Option<&'v str>,
    /// How the coverage rule counts it: its standing in the view's entity
    /// snapshot (ADR 0019), `testable`, `obligated()`, `exempt()`.
    /// Independent of the recorded report.
    pub standing: &'v Standing,
    /// Its `verify` statements, in declaration order: its record's in the
    /// view's entity snapshot (ADR 0019).
    pub obligations: &'v [ObligationRecord],
    /// The references to it and the ones it makes (ADR 0016 D2), in graph
    /// edge order.
    pub references: References,
    /// The diagnostics the view reports about it (`navigate::is_about`:
    /// what their data names, else the innermost block holding their
    /// span), in the order they are reported.
    pub diagnostics: Vec<Diagnostic>,
}

/// An entity's coverage: its verdict, and whether a recorded report was
/// read at the view's root (without one every verdict has no tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityCoverage {
    pub verdict: Verdict,
    pub recorded: bool,
}

impl EntityCoverage {
    pub fn status(&self) -> Status {
        self.verdict.status()
    }
}

/// The coverage of `entity_id` under the one rule, over the view's recorded
/// report: what MCP inspect reports (`coverage_status`) and the hover's
/// Coverage line. A surface that shows no coverage (the context prompt) does
/// not call it, so it reads no report. An entity the graph lacks has the
/// default verdict (no obligations, no tests). A recorded report that cannot
/// be used is its E045 (ADR 0029 D1): MCP inspect fails on it (ADR 0004
/// D2-e), the hover shows it (ADR 0015 I3).
pub fn coverage(view: &ProjectView, entity_id: &str) -> Result<EntityCoverage, OpError> {
    view.recorded().map(|recorded| EntityCoverage {
        verdict: recorded
            .coverage
            .verdict(entity_id)
            .cloned()
            .unwrap_or_default(),
        recorded: recorded.report.is_some(),
    })
}

/// The facts of the entity `entity_id`, with the diagnostics the view
/// reports about it (MCP: its call target's; the LSP: what it published).
/// An entity the graph lacks is `navigate::not_found`.
pub fn inspect<'v>(
    view: &ProjectView<'v>,
    entity_id: &str,
) -> Result<EntityFacts<'v>, Refusal<EntityField>> {
    let graph = view.graph();
    let node = graph
        .node(entity_id)
        .ok_or_else(|| not_found(graph, entity_id).about(EntityField::EntityId))?;
    // The snapshot is built over the same graph: a node it lacks is not one
    // this view can state facts about.
    let (record, standing) = view
        .entities()
        .get(entity_id)
        .ok_or_else(|| not_found(graph, entity_id).about(EntityField::EntityId))?;
    Ok(EntityFacts {
        node,
        kind: view.registries().kinds().get(node.kind.raw.as_str()),
        headline: specforge_emitter::context::headline_statement(node, view.registries().fields()),
        standing,
        obligations: &record.obligations,
        references: References::of(view, entity_id),
        diagnostics: view
            .reported()
            .into_iter()
            .filter(|d| is_about(graph, d, entity_id))
            .collect(),
    })
}

/// An obligation as MCP inspect and the context prompt list it:
/// `"<kind> <text>"`.
pub fn obligation_text(obligation: &ObligationRecord) -> String {
    format!("{} {}", obligation.kind, obligation.text)
}
