//! An agent plan checked against the graph: the trace tool's plan mode,
//! one operation over the project view (ADR 0015). It reads the graph and
//! the entity snapshot, never the recorded test report.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specforge_common::codes;
use specforge_common::shape::{self, Shape};
use std::collections::{HashMap, HashSet};

use crate::view::ProjectView;

/// How a plan falls short of the graph: the `McpTracePlanResult` document
/// that `specforge trace --plan` prints and `specforge.trace` answers
/// (ADR 0052).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct PlanOutcome {
    /// The plan passes: no entry names no entity and none comes before an
    /// entity it depends on, entities on one cycle in any order among
    /// themselves (ADR 0204). A missing entry is a warning
    /// (`PlanGapKind::fails`).
    pub ok: bool,
    /// Plan entries that name an entity in the graph, in plan order.
    pub resolved_entries: Vec<String>,
    /// Unresolved entries (in entry order), then the entities that count
    /// toward coverage and have no entry (in id order), then ordering
    /// gaps (in graph edge order).
    pub gaps: Vec<PlanGap>,
}

/// What an agent intends to do, entity by entity, in the order it will do it
/// (`AgentPlan`, spec/types/graph.spec). Its entry order is its dependency
/// order; the graph's edges are the dependencies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Shape)]
#[serde(deny_unknown_fields)]
pub struct AgentPlan {
    pub plan_id: String,
    pub entries: Vec<AgentPlanEntry>,
}

/// One entry of an [`AgentPlan`] (`AgentPlanEntry`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Shape)]
#[serde(deny_unknown_fields)]
pub struct AgentPlanEntry {
    /// The entity the entry is about.
    pub entity_id: String,
    /// What the agent will do to it: free text (implement, modify, remove, ...).
    pub action: String,
}

/// Why a value is not an [`AgentPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotAPlan {
    /// Text that is not JSON: serde_json's message.
    NotJson(String),
    /// Every place the value departs from the type's schema, as
    /// `shape::violations` words it, rooted at `$`
    /// (`$.entries[0]: missing required action`), in its order.
    Departs(Vec<String>),
}

impl NotAPlan {
    /// The refusal's message, the plan named `name` (MCP's argument, the
    /// CLI's `plan`): `plan is not valid JSON: <why>`, or `plan must be an
    /// AgentPlan: plan: missing required plan_id; plan.entries[0]: missing
    /// required action` (each violation's `$` replaced by `name`, joined by
    /// `; `).
    pub fn message(&self, name: &str) -> String {
        match self {
            NotAPlan::NotJson(why) => format!("{name} is not valid JSON: {why}"),
            NotAPlan::Departs(found) => format!(
                "{name} must be an AgentPlan: {}",
                found
                    .iter()
                    .map(|violation| {
                        format!("{name}{}", violation.strip_prefix('$').unwrap_or(violation))
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        }
    }
}

impl std::fmt::Display for NotAPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message("plan"))
    }
}

impl std::error::Error for NotAPlan {}

impl AgentPlan {
    /// The plan `value` is. The value is checked against the type's schema
    /// (`shape::document::<AgentPlan>()`, the one `specforge.trace` lists)
    /// before it is decoded. So a value the schema does not admit, positional
    /// arrays included, is refused, listing every place it departs, and one
    /// it admits always decodes.
    pub fn read(value: &Value) -> Result<AgentPlan, NotAPlan> {
        let found = shape::violations(shape::document::<AgentPlan>(), value);
        if !found.is_empty() {
            return Err(NotAPlan::Departs(found));
        }
        AgentPlan::deserialize(value).map_err(|e| NotAPlan::Departs(vec![format!("$: {e}")]))
    }

    /// The plan the JSON `text` holds ([`Self::read`] of its value).
    pub fn parse(text: &str) -> Result<AgentPlan, NotAPlan> {
        let value =
            serde_json::from_str::<Value>(text).map_err(|e| NotAPlan::NotJson(e.to_string()))?;
        AgentPlan::read(&value)
    }
}

/// One way a plan falls short (`McpTraceGap`): `source` is `plan` when the
/// plan itself is at fault rather than one of its entries.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct PlanGap {
    #[serde(rename = "source_entity")]
    pub source: String,
    #[serde(rename = "target_entity")]
    pub target: String,
    #[serde(rename = "missing_link_type")]
    pub kind: PlanGapKind,
    /// The gap in a sentence (an unresolved entry's starts with E003).
    #[serde(rename = "gap_context")]
    pub context: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum PlanGapKind {
    /// An entry names no entity in the graph.
    UnresolvedEntity,
    /// An entity that counts toward coverage has no entry.
    MissingPlanEntry,
    /// An entry comes before an entity it depends on that is not on a cycle with it.
    Ordering,
}

impl PlanGapKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PlanGapKind::UnresolvedEntity => "unresolved_entity",
            PlanGapKind::MissingPlanEntry => "missing_plan_entry",
            PlanGapKind::Ordering => "ordering",
        }
    }

    /// It fails the plan: an unresolved entry (E003) or an ordering gap. A
    /// missing entry is a warning (validate_agent_plan).
    pub fn fails(self) -> bool {
        !matches!(self, PlanGapKind::MissingPlanEntry)
    }
}

/// The side of a gap that is the plan itself.
const PLAN: &str = "plan";

/// Check `plan` against the view's graph: entries naming no entity,
/// entities that count toward coverage it has no entry for, and entries
/// ordered before what they depend on (`ordering_gaps`). It reads the graph and the
/// view's entity snapshot (`ProjectView::entities`), never the recorded
/// test report, so it cannot fail.
pub fn check(view: &ProjectView, plan: &AgentPlan) -> PlanOutcome {
    let graph = view.graph();
    let mut validated_entries = Vec::new();
    let mut gaps = Vec::new();

    let mut plan_ids: Vec<String> = Vec::new();

    // Validate each entry
    for entry in &plan.entries {
        let id = entry.entity_id.as_str();
        plan_ids.push(id.to_string());
        if graph.node(id).is_some() {
            validated_entries.push(id.to_string());
        } else {
            let message = format!(
                "{}: unresolved entity '{}' in plan — not found in graph",
                codes::E003,
                id
            );
            gaps.push(PlanGap {
                source: PLAN.to_string(),
                target: id.to_string(),
                kind: PlanGapKind::UnresolvedEntity,
                context: message,
            });
        }
    }
    let plan_id_set: HashSet<String> = plan_ids.iter().cloned().collect();

    // The entities that count toward coverage, missing from the plan.
    for (record, standing) in view.entities().iter() {
        let id = &record.id;
        if standing.counts() && !plan_id_set.contains(id.as_str()) {
            let message = format!(
                "testable entity '{id}' ({}) is not covered by the plan",
                record.kind
            );
            gaps.push(PlanGap {
                source: PLAN.to_string(),
                target: id.clone(),
                kind: PlanGapKind::MissingPlanEntry,
                context: message,
            });
        }
    }

    gaps.extend(ordering_gaps(graph, &plan_ids));

    PlanOutcome {
        ok: !gaps.iter().any(|gap| gap.kind.fails()),
        resolved_entries: validated_entries,
        gaps,
    }
}

/// The ordering gaps of a plan whose entries name `plan_ids`, in graph edge
/// order (ADR 0204). Every edge `a -> b` says `a` depends on `b`, and an
/// entity's place is its last entry. Entities on one cycle depend on one
/// another: they are one component, which the plan may list in any order.
/// Every entry of a component comes after every entry of each component it
/// has an edge to; a gap names the depending component's first entry and the
/// needed component's last. A component of one entity is the entity itself,
/// so on a graph without cycles this is the edge-by-edge check.
fn ordering_gaps(graph: &specforge_graph::Graph, plan_ids: &[String]) -> Vec<PlanGap> {
    let position: HashMap<&str, usize> = plan_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    // Each entity on a cycle names its component by the cycle's smallest member.
    let mut on_cycle: HashMap<&str, &'static str> = HashMap::new();
    for cycle in graph.cycles() {
        let name = cycle.members[0].as_str();
        on_cycle.extend(cycle.members.iter().map(|member| (member.as_str(), name)));
    }
    // Each component's first and last entry: (place, entity).
    let mut first: HashMap<&str, (usize, &str)> = HashMap::new();
    let mut last: HashMap<&str, (usize, &str)> = HashMap::new();
    for (&id, &at) in &position {
        let component = component(&on_cycle, id);
        let early = first.entry(component).or_insert((at, id));
        if at < early.0 {
            *early = (at, id);
        }
        let late = last.entry(component).or_insert((at, id));
        if at > late.0 {
            *late = (at, id);
        }
    }
    let mut gaps = Vec::new();
    for edge in graph.edges() {
        let (source, target) = (edge.source.as_str(), edge.target.as_str());
        let (from, to) = (component(&on_cycle, source), component(&on_cycle, target));
        if from == to {
            continue; // one component: no order between its entities
        }
        let (Some(&(early, entry)), Some(&(late, needed))) = (first.get(from), last.get(to)) else {
            continue;
        };
        if late <= early {
            continue;
        }
        let context = if (entry, needed) == (source, target) {
            format!(
                "'{source}' depends on '{target}' (via {}), but '{target}' appears later in the plan",
                edge.label
            )
        } else {
            format!(
                "'{entry}' depends on '{needed}' ('{source}' depends on '{target}' via {}, and entities on one cycle are done together), but '{needed}' appears later in the plan",
                edge.label
            )
        };
        gaps.push(PlanGap {
            source: entry.to_string(),
            target: needed.to_string(),
            kind: PlanGapKind::Ordering,
            context,
        });
    }
    gaps
}

/// The component `id` is in: its cycle's smallest member, or `id` itself.
fn component<'a>(on_cycle: &HashMap<&str, &'static str>, id: &'a str) -> &'a str {
    on_cycle.get(id).copied().unwrap_or(id)
}
