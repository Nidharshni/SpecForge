//! `specforge://prompts/trace`: the dependency chain of a plan or of one
//! entity, and what in it is unverified.

use std::collections::BTreeSet;

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::plan::PlanError;
use specforge_ops::trace::{Target, trace};

use crate::args::{AgentPlan, Arguments};
use crate::prompt::{CorePrompt, Prompted};
use crate::refusal::ArgumentOf;
use crate::tool::{ErrorCode, McpError};
use crate::tools::trace::{Gap, analyze_plan};
use specforge_ops::navigate::EntityField;
use specforge_ops::view::ProjectView;

/// The trace prompt (`McpTracePromptResult`).
pub struct Prompt;

impl ArgumentOf<EntityField> for Prompt {
    fn argument(field: EntityField) -> Option<&'static str> {
        match field {
            EntityField::EntityId => Some("entity_id"),
        }
    }
}

impl CorePrompt for Prompt {
    const NAME: &'static str = "specforge://prompts/trace";
    const DESCRIPTION: &'static str = "Identify traceability gaps for a plan";
    const SPEC: &'static str = "McpTracePromptResult";
    type Args = Args;
    type Payload = Payload;

    fn render(view: ProjectView<'_>, args: Args) -> Result<Prompted<Payload>, Box<McpError>> {
        render(view, args)
    }
}

/// The trace prompt's payload: the dependency chain of a plan or of one
/// entity, and what in it is unverified.
#[derive(Debug, Serialize, Shape)]
pub struct Payload {
    pub coverage_gaps: Vec<Gap>,
    pub unverified_entities: Vec<String>,
    pub affected_entities: Vec<String>,
}

/// `specforge://prompts/trace`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// AgentPlan JSON ({"entries": [{"entity_id", "action"}]}) to check against the graph
    plan: Option<AgentPlan>,
    /// Entity ID to trace when no plan is given
    entity_id: Option<String>,
}

fn render(view: ProjectView<'_>, args: Args) -> Result<Prompted<Payload>, Box<McpError>> {
    // A plan's entries, or the one entity, seed the trace; the gaps are the
    // plan's, or the entity's chain's missing links.
    let (seeds, coverage_gaps, subject) = match (&args.plan, args.entity_id.as_deref()) {
        (Some(plan), _) => {
            let analysis = analyze_plan(&view, &plan.0).map_err(|PlanError::NotAPlan(why)| {
                McpError::new(ErrorCode::InvalidInput, why).with_argument("plan")
            })?;
            (analysis.entries, analysis.gaps, "the plan".to_string())
        }
        (None, Some(entity_id)) => {
            // As the trace tool traces it: its chain and missing links.
            let outcome = trace(&view, Target::Entity(entity_id))
                .map_err(|error| Prompt::refused(specforge_ops::Refusal::from(error)))?;
            let gaps = outcome.gaps().iter().map(Gap::of).collect();
            (
                vec![entity_id.to_string()],
                gaps,
                format!("entity '{entity_id}'"),
            )
        }
        (None, None) => {
            // As the trace tool refuses it.
            return Err(Box::new(
                McpError::new(
                    ErrorCode::InvalidInput,
                    "Missing required parameter: entity_id or plan",
                )
                .with_argument("entity_id"),
            ));
        }
    };

    // Everything the seeds' trace chains reach, the seeds included.
    let mut affected: BTreeSet<String> = seeds.iter().cloned().collect();
    for seed in &seeds {
        if let Ok(outcome) = trace(&view, Target::Entity(seed)) {
            affected.extend(outcome.reached().into_iter().map(str::to_string));
        }
    }

    // The affected entities that count toward coverage and are not proven
    // (the coverage view's one definition of unverified).
    let coverage = view.coverage().map_err(McpError::from)?;
    let unverified: Vec<String> = affected
        .iter()
        .filter(|id| coverage.is_unverified(id))
        .cloned()
        .collect();

    let payload = Payload {
        coverage_gaps,
        unverified_entities: unverified,
        affected_entities: affected.into_iter().collect(),
    };

    let instruction = format!(
        "Trace the dependency chain for {}. \
         The data below shows all affected entities, unverified entities in the chain, \
         and coverage gaps. Prioritize addressing unverified entities that are on critical paths.",
        subject
    );

    Ok(Prompted {
        instruction,
        payload,
    })
}
