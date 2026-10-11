use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::coverage::{CoverageQuery, CoverageRowDocument};
use specforge_project::coverage::Status;

use crate::args::Arguments;
use crate::reply::Answered;
use crate::tool::McpError;
use specforge_ops::view::ProjectView;

/// `specforge.coverage`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Filter to specific entity
    entity_id: Option<String>,
    /// Filter by entity kind
    kind: Option<String>,
    /// Only entities with this coverage status
    #[arg(choice = specforge_ops::coverage::STATUS)]
    status_filter: Option<Status>,
}

/// `specforge.coverage`'s reply: one row per entity.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    entities: Vec<CoverageRowDocument>,
}

/// `specforge.coverage`: the coverage view of the served project (its
/// recorded tests read at its root; with nothing served there is no root,
/// so none). With no filter, the entities that count toward
/// coverage, the ones stats counts as testable.
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.coverage";
    const DESCRIPTION: &'static str = "Get coverage status per entity";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpCoverageResults");
    type Args = Args;
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Reply> {
        let query = CoverageQuery {
            entity_id: args.entity_id.as_deref(),
            kind: args.kind.as_deref(),
            status: args.status_filter,
        };
        let outcome = specforge_ops::coverage::coverage(&view, &query).map_err(McpError::from)?;
        Ok(Reply {
            entities: outcome.rows.iter().map(|row| row.document()).collect(),
        }
        .into())
    }
}
