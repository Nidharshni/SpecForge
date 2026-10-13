use crate::target::View;
use crate::tool::{CoreTool, Effect, RefusalArgument, ToolGroup};
use specforge_ops::export::{self, Format, Schema};

use crate::args::Arguments;
use crate::reply::{Answered, Text};
use crate::tool::McpError;
use specforge_ops::view::ProjectView;

/// `specforge.export`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Export format
    #[arg(choice = specforge_ops::export::AGENT_FORMAT)]
    format: Format,
    /// Scope to entity subgraph
    scope: Option<String>,
    /// With scope, how many hops from the scoped entity to include
    depth: Option<usize>,
    /// Keep only entities of these kinds; the scoped entity always stays
    kinds: Vec<String>,
    /// Fail unless a reader of this schema version reads the export: the same major version, not newer than the project's schema
    schema_version: Option<String>,
    /// Optional token budget; truncates the export to the most central entities that fit
    max_tokens: Option<usize>,
    /// Embed the Graph Protocol schema in a context, brief or budgeted graph export (a full graph export embeds it already); under max_tokens it counts toward the budget
    with_schema: bool,
    /// Leave the schema out of a graph export (Graph Protocol 1.0)
    no_schema: bool,
}

/// `specforge.export`: the export `specforge export` writes, through the
/// same function and schema policy (ADR 0004 D3-a). `with_schema` and
/// `no_schema` are the CLI's `--with-schema` and `--no-schema`.
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.export";
    const DESCRIPTION: &'static str = "Export the graph in various formats";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    type Args = Args;
    type Input = View;
    type Returns = Answered<Text>;
    const REFUSAL_ARGUMENTS: &'static [RefusalArgument] = &[
        RefusalArgument {
            code: export::DEPTH_WITHOUT_SCOPE,
            argument: "depth",
        },
        RefusalArgument {
            code: specforge_common::codes::E062.id(),
            argument: "max_tokens",
        },
        RefusalArgument {
            code: export::INVALID_SCHEMA_VERSION,
            argument: "schema_version",
        },
        RefusalArgument {
            code: specforge_common::codes::E027.id(),
            argument: "schema_version",
        },
    ];

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Text> {
        // The tool serves the agent formats; dot is `specforge.render`'s.
        let schema = match (args.no_schema, args.with_schema) {
            (true, _) => Schema::Without,
            (_, true) => Schema::With,
            _ => Schema::Default,
        };
        let request = export::Request {
            format: Some(args.format),
            scope: args.scope.as_deref(),
            depth: args.depth,
            kinds: args.kinds.iter().map(String::as_str).collect(),
            max_tokens: args.max_tokens,
            schema,
            schema_version: args.schema_version.as_deref(),
        };

        let document = export::export(&view, &request).map_err(McpError::from)?;
        Ok(Text(document).into())
    }
}
