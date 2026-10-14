use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::plan::{AgentPlan, PlanOutcome};
use specforge_ops::trace::{ChainDocument, Target, TraceChain};
use specforge_ops::view::ProjectView;

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::Answered;
use crate::tool::{ErrorCode, McpError};
use specforge_ops::navigate::EntityField;

/// `specforge.trace`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Entity ID to trace
    entity_id: Option<String>,
    /// An AgentPlan ({"plan_id", "entries": [{"entity_id", "action"}]}), or its JSON text, to check for gaps against the graph instead of tracing one entity
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
    pub(crate) fn of(gap: &specforge_ops::trace::Gap) -> Self {
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
        if let Some(plan) = &args.plan {
            return Ok(Reply::Plan(specforge_ops::plan::check(&view, plan)).into());
        }
        let Some(entity_id) = args.entity_id.as_deref() else {
            return Err(Box::new(McpError::invalid_input(
                "entity_id",
                "Missing required parameter: entity_id or plan",
            )));
        };
        let outcome =
            specforge_ops::trace::trace(&view, Target::Entity(entity_id)).map_err(|error| {
                Self::refused(specforge_ops::Refusal::from(error)).with_entity(entity_id)
            })?;
        match outcome.into_chain() {
            Some(chain) => Ok(Reply::Chain(chain).into()),
            None => Err(Box::new(McpError::new(
                ErrorCode::InternalError,
                "trace of one entity answered no chain",
            ))),
        }
    }
}
