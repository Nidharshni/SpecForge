use crate::target::View;
use crate::tool::{CoreTool, Effect, ToolGroup};
use specforge_ops::read::export::{self, ExportField, Format, Schema};

use crate::args::Arguments;
use crate::refusal::ArgumentOf;
use crate::reply::{Answer, Answered, Text};
use specforge_ops::view::ProjectView;

/// `specforge.export`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Export format
    #[arg(choice = specforge_ops::read::export::FORMAT)]
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

impl ArgumentOf<ExportField> for Tool {
    fn argument(field: ExportField) -> Option<&'static str> {
        match field {
            ExportField::Scope => Some("scope"),
            ExportField::MaxTokens => Some("max_tokens"),
            ExportField::SchemaVersion => Some("schema_version"),
            ExportField::Schema => Some("with_schema"),
            // Export answers inline.
            ExportField::Directory => None,
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.export";
    const DESCRIPTION: &'static str = "Export the graph in various formats";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    type Args = Args;
    type Input = View;
    type Returns = Answered<Text>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Text> {
        // Every export format, as `specforge export` takes them (ADR 0152 D8).
        let schema = Schema::of_flags(args.with_schema, args.no_schema).map_err(Self::refused)?;
        let request = export::Request {
            format: Some(args.format),
            scope: args.scope.as_deref(),
            depth: args.depth,
            kinds: args.kinds.iter().map(String::as_str).collect(),
            max_tokens: args.max_tokens,
            schema,
            schema_version: args.schema_version.as_deref(),
            coverage: None,
        };

        Ok(Answer::noticed(
            export::export(&view, &request)
                .map_err(Self::refused)?
                .map(Text),
        ))
    }
}
