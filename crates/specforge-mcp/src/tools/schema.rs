use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use specforge_ops::schema::SchemaRequest;

use crate::args::Arguments;
use crate::reply::Answered;
use crate::tool::McpError;

/// `specforge.schema`'s reply: the document `specforge schema` prints.
type Reply = specforge_ops::schema::SchemaDocument;
use specforge_ops::view::ProjectView;

/// `specforge.schema`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Filter schema to a specific entity kind
    kind: Option<String>,
    /// Include edge type definitions
    #[arg(default = SchemaRequest::default().edges)]
    include_edges: bool,
    /// Include the validation rules loaded extensions declare
    #[arg(default = SchemaRequest::default().validation_rules)]
    include_validation_rules: bool,
}

/// `specforge.schema`: the schema operation over the served project: the
/// GraphProtocolSchema a full export embeds, versioned as `specforge
/// export` versions it. `kind` keeps that kind and the edge types that can
/// start or end at it (a kind no extension declares is `invalid_input` on
/// `kind`, naming the closest); `include_edges: false` drops `edge_types`;
/// `include_validation_rules` adds the rules the loaded extensions declare.
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.schema";
    const DESCRIPTION: &'static str = "Get the GraphProtocolSchema: entity kinds with their typed fields, edge types and the loaded extensions";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Document("specforge schema's GraphProtocolSchema document");
    const OPEN_VALUES: &'static [&'static str] = &["$.validation_rules[]"];
    type Args = Args;
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Reply> {
        let request = SchemaRequest {
            kind: args.kind.as_deref(),
            edges: args.include_edges,
            validation_rules: args.include_validation_rules,
        };
        match specforge_ops::schema::schema(&view, &request) {
            Ok(outcome) => Ok(outcome.document().into()),
            Err(error) => Err(Box::new(McpError::from(error).with_argument("kind"))),
        }
    }
}
