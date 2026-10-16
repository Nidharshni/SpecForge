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
use crate::tool::{ErrorCode, McpError};
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

/// A gap as MCP spells it (`McpTraceGap`).
#[derive(Debug, Serialize, Shape)]
pub struct Gap {
    source_entity: String,
    target_entity: String,
    missing_link_type: String,
    gap_context: String,
}

impl Gap {
    pub(crate) fn of(gap: &specforge_ops::read::trace::Gap) -> Self {
        Gap {
            source_entity: gap.source().to_string(),
            target_entity: gap.target().to_string(),
            missing_link_type: gap.kind().to_string(),
            gap_context: gap.context().to_string(),
        }
    }
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
            Ask::Trace(Target::Every) => {
                return Err(Box::new(McpError::invalid_input(
                    "entity_id",
                    "Missing required parameter: entity_id or plan",
                )));
            }
        };
        let traced = specforge_ops::read::trace::trace(&view, Target::Entity(entity_id))
            .map_err(|refusal: Refusal<EntityField>| Self::refused(refusal))?;
        let chain = traced.try_map(|outcome| {
            outcome.into_chain().ok_or_else(|| {
                Box::new(McpError::new(
                    ErrorCode::InternalError,
                    "trace of one entity answered no chain",
                ))
            })
        })?;
        Ok(Answer::noticed(chain.map(Reply::Chain)))
    }
}
