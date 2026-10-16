use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use specforge_common::SourceSpan;
use specforge_common::shape::Shape;
use specforge_ops::read::navigate::{
    DIRECTION, Direction, Navigator, Occurrence, Precision, ReferenceQuery, Role,
};

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::Answered;
use specforge_ops::view::EntityField;
use specforge_ops::view::ProjectView;

/// `specforge.find_references`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Entity ID
    entity_id: String,
    /// Which references
    #[arg(choice = specforge_ops::read::navigate::DIRECTION)]
    direction: Direction,
    /// Also return the entity's own declaration (its name)
    include_declaration: bool,
}

/// `specforge.find_references`'s reply.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    entity_id: String,
    #[shape(names = specforge_ops::read::navigate::DIRECTION)]
    direction: String,
    locations: Vec<Location>,
}

/// One occurrence of the entity's ID (`McpReferenceLocation`).
#[derive(Debug, Serialize, Shape)]
pub struct Location {
    referencing_entity_id: String,
    referenced_entity_id: String,
    /// The field naming the referenced entity; none for its declaration.
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<String>,
    role: Role,
    precision: Precision,
    source_span: SourceSpan,
}

impl Location {
    /// One occurrence as a location.
    pub(crate) fn of(occurrence: &Occurrence) -> Self {
        Location {
            referencing_entity_id: occurrence.holder.to_string(),
            referenced_entity_id: occurrence.target.to_string(),
            field: occurrence.field.as_ref().map(ToString::to_string),
            role: occurrence.role,
            precision: occurrence.precision,
            source_span: occurrence.span.clone(),
        }
    }
}

/// `specforge.find_references`: each occurrence of the entity's ID, as
/// the LSP's references answer it (ADR 0016). Incoming by default, the
/// declaration only when asked for.
pub(crate) struct Tool;

impl ArgumentOf<EntityField> for Tool {
    fn argument(field: EntityField) -> Option<&'static str> {
        match field {
            EntityField::EntityId => Some("entity_id"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.find_references";
    const DESCRIPTION: &'static str = "Find the references to an entity: each place another entity's field names it, as the identifier token (what an IDE's find-references shows)";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Navigation,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpReferenceResult");
    type Args = Args;
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Reply> {
        let query = ReferenceQuery {
            direction: args.direction,
            include_declaration: args.include_declaration,
        };
        let occurrences = Navigator::new(view)
            .references(&args.entity_id, query)
            .map_err(Self::refused)?;
        Ok(Reply {
            direction: DIRECTION.name_of(args.direction).to_string(),
            entity_id: args.entity_id,
            locations: occurrences.iter().map(Location::of).collect(),
        }
        .into())
    }
}
