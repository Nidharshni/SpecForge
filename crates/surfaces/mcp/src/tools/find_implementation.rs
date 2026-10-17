use crate::target::Project;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use specforge_ops::read::navigate;

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::Answered;
use crate::target::ProjectRef;
use specforge_ops::view::EntityField;

/// `specforge.find_implementation`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Entity ID to find implementations for
    entity_id: String,
}

/// `specforge.find_implementation`'s reply: the anchors read view's entity document (ADR 0140).
pub use specforge_ops::read::navigate::EntityAnchors as Reply;

/// The source items the anchors manifest anchors the entity to
/// (`specforge_ops::read::navigate::anchors_of_entity`), in manifest order.
pub(crate) struct Tool;

impl ArgumentOf<EntityField> for Tool {
    fn argument(field: EntityField) -> Option<&'static str> {
        match field {
            EntityField::EntityId => Some("entity_id"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.find_implementation";
    const DESCRIPTION: &'static str =
        "Find source code locations that implement a specforge entity";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Navigation,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpImplementationResult");
    type Args = Args;
    type Input = Project;
    type Returns = Answered<Reply>;

    fn call(project: ProjectRef<'_>, args: Args) -> Answered<Reply> {
        let found =
            navigate::anchors_of_entity(&project.view(), &args.entity_id).map_err(Self::refused)?;
        Ok(found.into())
    }
}
