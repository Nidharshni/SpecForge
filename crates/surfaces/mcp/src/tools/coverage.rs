use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_ops::read::coverage::{CoverageQuery, CoverageRowDocument};
use specforge_project::coverage::Status;

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::Answered;
use specforge_ops::view::{EntityField, ProjectView};

/// `specforge.coverage`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Filter to specific entity
    entity_id: Option<String>,
    /// Filter by entity kind
    kind: Option<String>,
    /// Only entities with this coverage status
    #[arg(choice = specforge_ops::read::coverage::STATUS)]
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
/// coverage, the ones stats counts as testable. An `entity_id` the graph lacks
/// is `entity_not_found` (E003) on `entity_id`.
pub(crate) struct Tool;

impl ArgumentOf<EntityField> for Tool {
    fn argument(field: EntityField) -> Option<&'static str> {
        match field {
            EntityField::EntityId => Some("entity_id"),
        }
    }
}

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
        let outcome =
            specforge_ops::read::coverage::coverage(&view, &query).map_err(Self::refused)?;
        Ok(Reply {
            entities: outcome.rows.iter().map(|row| row.document()).collect(),
        }
        .into())
    }
}
