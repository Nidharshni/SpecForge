//! `specforge://prompts/trace`: the dependency chain of a plan or of one
//! entity, and what in it is unverified.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::read::plan::AgentPlan;
use specforge_ops::read::trace::{Ask, Target, TraceField, trace};

use crate::args::Arguments;
use crate::prompt::{CorePrompt, Prompted};
use crate::refusal::ArgumentOf;
use crate::tool::McpError;
use crate::tools::trace::{Gap, every_is_not_offered};
use specforge_ops::Refusal;
use specforge_ops::view::EntityField;
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

impl ArgumentOf<TraceField> for Prompt {
    fn argument(field: TraceField) -> Option<&'static str> {
        match field {
            TraceField::Plan => Some("plan"),
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
    /// AgentPlan JSON ({"plan_id", "entries": [{"entity_id", "action"}]}) to check against the graph; not with entity_id
    plan: Option<AgentPlan>,
    /// Entity ID to trace when no plan is given
    entity_id: Option<String>,
}

fn render(view: ProjectView<'_>, args: Args) -> Result<Prompted<Payload>, Box<McpError>> {
    // A plan's entries, or the one entity, seed the trace; the gaps are the
    // plan's, or the entity's chain's missing links.
    let (seeds, coverage_gaps, subject) =
        match Ask::of(args.entity_id.as_deref(), args.plan.as_ref())
            .map_err(|refusal| Box::new(Prompt::refused(refusal)))?
        {
            Ask::Plan(plan) => {
                let outcome = specforge_ops::read::plan::check(&view, plan);
                let gaps = outcome
                    .gaps
                    .iter()
                    .map(|gap| Gap::of(&specforge_ops::read::trace::Gap::Plan(gap.clone())))
                    .collect();
                (outcome.entries, gaps, "the plan".to_string())
            }
            Ask::Trace(Target::Entity(entity_id)) => {
                // As the trace tool traces it: its chain and missing links.
                // ADR 0111 D6: the trace prompt's payload has no `_meta`, so it reports no notices.
                let (outcome, _unreported) = trace(&view, Target::Entity(entity_id))
                    .map_err(|refusal: Refusal<EntityField>| Prompt::refused(refusal))?
                    .into_value_and_notices();
                let gaps = outcome.gaps().iter().map(Gap::of).collect();
                (
                    vec![entity_id.to_string()],
                    gaps,
                    format!("entity '{entity_id}'"),
                )
            }
            Ask::Trace(Target::Every) => return Err(every_is_not_offered()),
        };

    // What the seeds' chains reach, and which of it is unverified.
    let reach = specforge_ops::read::trace::reach(&view, seeds.iter().map(String::as_str))
        .map_err(|error| Box::new(McpError::from(error)))?;
    let payload = Payload {
        coverage_gaps,
        unverified_entities: reach.unverified,
        affected_entities: reach.affected,
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
