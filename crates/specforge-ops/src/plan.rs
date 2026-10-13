//! An agent plan checked against the graph: the trace tool's plan mode,
//! one operation over the project view (ADR 0015). It reads the graph and
//! the entity snapshot, never the recorded test report.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specforge_common::codes;
use specforge_common::shape::{self, Shape};
use std::collections::{HashMap, HashSet};

use crate::view::ProjectView;

/// How a plan falls short of the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanOutcome {
    /// Plan entries that name an entity in the graph, in plan order.
    pub entries: Vec<String>,
    /// How the plan falls short: entries naming no entity (E003 in their
    /// context), testable entities with obligations it has no entry for,
    /// entries before what they depend on.
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

/// One way a plan falls short of the graph. `source` is `"plan"` when the
/// plan itself is at fault rather than one of its entries.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanGap {
    pub source: String,
    pub target: String,
    pub kind: PlanGapKind,
    /// The diagnostic text of the gap (an unresolved entry's starts with E003).
    pub context: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanGapKind {
    /// An entry names no entity in the graph.
    UnresolvedEntity,
    /// A testable entity with obligations has no entry.
    MissingPlanEntry,
    /// An entry comes before an entity it depends on.
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
}

/// The side of a gap that is the plan itself.
const PLAN: &str = "plan";

/// Check `plan` against the view's graph: entries naming no entity,
/// testable entities with obligations it has no entry for, and entries
/// ordered before an entity they depend on. It reads the graph and the
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

    // Testable entities that declare obligations, missing from the plan.
    for (record, standing) in view.entities().iter() {
        let id = &record.id;
        if standing.testable
            && standing.declares_obligations()
            && !plan_id_set.contains(id.as_str())
        {
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

    // Validate dependency ordering: if plan lists A before B, but graph has edge A->B
    // (A depends on B), that's fine. But if B->A exists (B depends on A) and B comes
    // before A in the plan, B would be implemented before its dependency A.
    // Build position map for plan ordering
    let position: HashMap<&str, usize> = plan_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    for edge in graph.edges() {
        // edge.source -> edge.target means source references target
        // So target should be implemented before source
        if let (Some(&src_pos), Some(&tgt_pos)) = (
            position.get(edge.source.as_str()),
            position.get(edge.target.as_str()),
        ) && tgt_pos > src_pos
        {
            let message = format!(
                "'{}' depends on '{}' (via {}), but '{}' appears later in the plan",
                edge.source, edge.target, edge.label, edge.target
            );
            gaps.push(PlanGap {
                source: edge.source.to_string(),
                target: edge.target.to_string(),
                kind: PlanGapKind::Ordering,
                context: message,
            });
        }
    }

    PlanOutcome {
        entries: validated_entries,
        gaps,
    }
}
