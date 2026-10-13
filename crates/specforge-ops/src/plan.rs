//! An agent plan checked against the graph: the trace tool's plan mode,
//! one operation over the project view (ADR 0015). It reads the graph and
//! the entity snapshot, never the recorded test report.

use serde::Serialize;
use serde_json::Value;
use specforge_common::codes;
use std::collections::{HashMap, HashSet};

use crate::view::ProjectView;
use crate::{OpError, OpErrorKind};

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

/// Why a plan could not be checked.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanError {
    /// The value is not an `AgentPlan`: why.
    NotAPlan(String),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::NotAPlan(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for PlanError {}

/// `invalid_input` for a value that is no plan.
impl From<PlanError> for OpError {
    fn from(error: PlanError) -> Self {
        match error {
            PlanError::NotAPlan(why) => {
                OpError::new(OpErrorKind::InvalidInput, "invalid_input", why)
            }
        }
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

/// Check `plan`, an `AgentPlan` object or JSON text of one, against the
/// view's graph: entries naming no entity, testable entities with
/// obligations it has no entry for, and entries ordered before an entity
/// they depend on.
pub fn check(view: &ProjectView, plan: &Value) -> Result<PlanOutcome, PlanError> {
    let parsed;
    let plan = match plan {
        Value::String(text) => {
            parsed = serde_json::from_str::<Value>(text)
                .map_err(|e| PlanError::NotAPlan(format!("plan is not valid JSON: {e}")))?;
            &parsed
        }
        other => other,
    };
    let Some(entries) = plan.get("entries").and_then(Value::as_array) else {
        return Err(PlanError::NotAPlan(
            "plan must be an AgentPlan object with an entries array".into(),
        ));
    };
    for (i, entry) in entries.iter().enumerate() {
        if !entry.get("entity_id").is_some_and(Value::is_string) {
            return Err(PlanError::NotAPlan(format!(
                "plan.entries[{i}].entity_id must be a string"
            )));
        }
    }
    Ok(validate(view, entries))
}

/// The plan's gaps. It reads the graph and the view's entity snapshot, never
/// the recorded test report: who is testable and what an entity declares
/// are its standing.
fn validate(view: &ProjectView, entries: &[Value]) -> PlanOutcome {
    let graph = view.graph();
    let mut validated_entries = Vec::new();
    let mut gaps = Vec::new();

    let mut plan_ids: Vec<String> = Vec::new();

    // Validate each entry
    for entry in entries {
        if let Some(id) = entry["entity_id"].as_str() {
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
