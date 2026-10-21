//! `specforge.render`: render the graph in an export format (`specforge_ops::read::export`).

use crate::refusal::ArgumentOf;
use crate::target::{OutputView, ViewToWrite};
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup, WriteHints};
use specforge_ops::read::export::ExportField;
use std::path::Path;

use serde::Serialize;
use serde_json::json;
use specforge_common::ProjectPath;
use specforge_common::shape::Shape;

use crate::args::Arguments;
use crate::reply::{Answer, Answered};
use crate::tool::McpError;

/// `specforge.render`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    // Required: a renderer is named, never assumed. It stays a string so
    // `call` can refuse an unknown one with `available_renderers`.
    /// Renderer to use
    #[arg(choice = specforge_ops::read::export::FORMAT)]
    format: String,
    /// Directory to write the rendering into, inside the project (relative to
    /// its root; returned inline when omitted)
    out_dir: Option<String>,
    /// Scope to entity
    scope: Option<String>,
}

/// `specforge.render`'s reply: the rendering inline, or
/// the files it was written to.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    #[shape(names = specforge_ops::read::export::FORMAT)]
    format: String,
    /// The rendering itself, when no `out_dir` was given.
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    /// The files written, named from the project root.
    output_files: Vec<ProjectPath>,
}

pub(crate) struct Tool;

impl ArgumentOf<ExportField> for Tool {
    fn argument(field: ExportField) -> Option<&'static str> {
        match field {
            ExportField::Scope => Some("scope"),
            ExportField::Directory => Some("out_dir"),
            // Render sends none of the others.
            ExportField::MaxTokens | ExportField::SchemaVersion | ExportField::Schema => None,
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.render";
    const DESCRIPTION: &'static str = "Render output in a specified format";
    const EFFECT: Effect = Effect::WritesOutput {
        group: ToolGroup::Management,
        hints: WriteHints {
            destructive: true,
            idempotent: true,
            open_world: false,
        },
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpRenderResult");
    type Args = Args;
    type Input = OutputView;
    type Returns = Answered<Reply>;

    fn call(given: ViewToWrite<'_>, args: Args) -> Answered<Reply> {
        use specforge_ops::read::export::{self as export, FORMAT};

        // The renderers are the export formats, named as `specforge export
        // --format` names them (ADR 0027 D8); `json` is `graph`'s alias, which
        // is accepted and never listed: the refusal's "Expected:" and
        // `available_renderers` are the one list the table names.
        let format = match FORMAT.parse(&args.format) {
            Ok(format) => format,
            Err(error) => {
                let mut refusal = McpError::from(error).with_argument("format");
                let mut data = refusal.data.take().unwrap_or_else(|| json!({}));
                data["available_renderers"] = json!(FORMAT.names().collect::<Vec<_>>());
                return Err(Box::new(refusal.with_data(data)));
            }
        };

        // `graph` is the full graph export: Graph Protocol 2.0 with the schema,
        // as `specforge export --format graph` writes it.
        let request = specforge_ops::read::export::Request {
            format: Some(format),
            scope: args.scope.as_deref(),
            ..specforge_ops::read::export::Request::default()
        };
        let name = FORMAT.name_of(format);
        let reply = |output, output_files| Reply {
            format: name.to_string(),
            output,
            output_files,
        };

        // With out_dir the export operation writes the rendering into it; without it, inline.
        let noticed = match args.out_dir.as_deref() {
            None => export::export(&given.view, &request)
                .map_err(Self::refused)?
                .map(|document| reply(Some(document), Vec::new())),
            Some(dir) => {
                let directory = export::Directory {
                    name: Path::new(dir),
                    caller: given.caller(),
                };
                export::write_into(&given.view, &request, &directory)
                    .map_err(Self::refused)?
                    .map(|file| reply(None, vec![file]))
            }
        };
        Ok(Answer::noticed(noticed))
    }
}
