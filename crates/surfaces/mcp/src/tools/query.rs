use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use serde::Serialize;
use serde_json::Value;
use specforge_common::shape::{Object, Shape};
use specforge_ops::read::export::Format;
use specforge_ops::read::query::{QueryField, QueryRequest, query};

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::{Answer, Answered};
use specforge_ops::view::ProjectView;

/// `specforge.query`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Entity ID to query
    entity_id: String,
    /// Number of hops
    #[arg(default = specforge_ops::read::query::DEFAULT_DEPTH)]
    depth: usize,
    /// Filter by entity kinds
    kinds: Vec<String>,
    /// Output detail level
    #[arg(choice = specforge_ops::read::export::AGENT_FORMAT)]
    format: Format,
    /// Include coverage metadata in the response
    include_coverage: bool,
}

/// `specforge.query`'s reply: the export document of the requested format
/// (ADR 0007), its nodes carrying `coverage_status` when asked. Its schema
/// is the emitter's derived document schemas
/// ([`specforge_ops::read::query::document_schema`]); the document reaches MCP as
/// the text the emitter wrote, read once into the structured result.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Reply(Value);

impl Shape for Reply {
    fn body(defs: &mut specforge_common::shape::Definitions) -> Value {
        specforge_ops::read::query::document_schema(defs)
    }
}

impl Object for Reply {}

/// `specforge.query`: the query read view (`specforge_ops::read::query`), the
/// document `specforge query` prints for the same arguments; an unknown
/// kind of the filter rides in `_meta.diagnostics` (I020).
pub(crate) struct Tool;

impl ArgumentOf<QueryField> for Tool {
    fn argument(field: QueryField) -> Option<&'static str> {
        match field {
            QueryField::EntityId => Some("entity_id"),
            QueryField::Format => Some("format"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.query";
    const DESCRIPTION: &'static str = "Query the graph at multiple resolutions";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Document("the agent export, ADR 0007");
    const OPEN_VALUES: &'static [&'static str] = &["$.nodes[]", "$.nodes[].fields"];
    type Args = Args;
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Reply> {
        let request = QueryRequest {
            entity_id: &args.entity_id,
            depth: Some(args.depth),
            kinds: args.kinds.iter().map(String::as_str).collect(),
            format: Some(args.format),
            include_coverage: args.include_coverage,
        };
        Ok(Answer::noticed(
            query(&view, &request)
                .map_err(Self::refused)?
                .map(|document| Reply(serde_json::from_str(&document).expect("an export is JSON"))),
        ))
    }
}
