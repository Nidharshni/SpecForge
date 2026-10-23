use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::read::plan::{AgentPlan, PlanOutcome};
use specforge_ops::read::trace::{Ask, ChainDocument, Target, TraceChain, TraceField};
use specforge_ops::view::ProjectView;

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::{Answer, Answered};
use crate::tool::McpError;
use specforge_ops::Refusal;
use specforge_ops::view::EntityField;

/// `specforge.trace`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Entity ID to trace
    entity_id: Option<String>,
    /// An AgentPlan ({"plan_id", "entries": [{"entity_id", "action"}]}), or its JSON text, to check for gaps against the graph instead of tracing one entity; not with entity_id
    plan: Option<AgentPlan>,
}

/// `specforge.trace`'s reply: an entity's chain, the
/// document `specforge trace <entity> --format json` writes, or a plan's
/// check.
#[derive(Debug, Serialize, Shape)]
#[serde(untagged)]
pub enum Reply {
    Chain(ChainDocument<TraceChain>),
    Plan(PlanOutcome),
}

/// `specforge.trace`: the trace operation over the served project. An
/// entity's result is the document `specforge trace <entity> --format
/// json` writes; a plan's is an `McpTracePlanResult`.
pub(crate) struct Tool;

impl ArgumentOf<EntityField> for Tool {
    fn argument(field: EntityField) -> Option<&'static str> {
        match field {
            EntityField::EntityId => Some("entity_id"),
        }
    }
}

impl ArgumentOf<TraceField> for Tool {
    fn argument(field: TraceField) -> Option<&'static str> {
        match field {
            TraceField::Plan => Some("plan"),
        }
    }
}

/// The refusal of a trace of every entity, which MCP does not offer (ADR 0101 D4): the tool and the trace
/// prompt answer it when neither `entity_id` nor `plan` is given.
pub(crate) fn every_is_not_offered() -> Box<McpError> {
    Box::new(McpError::invalid_input(
        "entity_id",
        "Missing required parameter: entity_id or plan",
    ))
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.trace";
    const DESCRIPTION: &'static str = "Show traceability chain for an entity, or check an agent plan for gaps (entity_id or plan)";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpTraceResult");
    type Args = Args;
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Reply> {
        let entity_id = match Ask::of(args.entity_id.as_deref(), args.plan.as_ref())
            .map_err(|refusal| Box::new(Self::refused(refusal)))?
        {
            Ask::Plan(plan) => {
                return Ok(Reply::Plan(specforge_ops::read::plan::check(&view, plan)).into());
            }
            Ask::Trace(Target::Entity(entity_id)) => entity_id,
            Ask::Trace(Target::Every) => return Err(every_is_not_offered()),
        };
        let chain = specforge_ops::read::trace::chain(&view, entity_id)
            .map_err(|refusal: Refusal<EntityField>| Self::refused(refusal))?;
        Ok(Answer::noticed(chain.map(Reply::Chain)))
    }
}
