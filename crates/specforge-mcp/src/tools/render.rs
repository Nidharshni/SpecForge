//! `specforge.render`: render the graph in an export format (`specforge_ops::export`).

use crate::refusal::ArgumentOf;
use crate::target::View;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup, WriteHints};
use specforge_ops::export::ExportField;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::json;
use specforge_common::shape::Shape;

use crate::args::Arguments;
use crate::reply::Answered;
use crate::tool::{ErrorCode, McpError};
use specforge_ops::view::ProjectView;

/// `specforge.render`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    // Required: a renderer is named, never assumed. It stays a string so
    // `call` can refuse an unknown one with `available_renderers`.
    /// Renderer to use
    #[arg(choice = specforge_ops::export::FORMAT)]
    format: String,
    /// Directory to write the rendering into, relative to the project root
    /// (returned inline when omitted)
    out_dir: Option<String>,
    /// Scope to entity
    scope: Option<String>,
}

/// `specforge.render`'s reply: the rendering inline, or
/// the files it was written to.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    #[shape(names = specforge_ops::export::FORMAT)]
    format: String,
    /// The rendering itself, when no `out_dir` was given.
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    output_files: Vec<String>,
}

pub(crate) struct Tool;

impl ArgumentOf<ExportField> for Tool {
    fn argument(field: ExportField) -> Option<&'static str> {
        match field {
            ExportField::Scope => Some("scope"),
            // Render sends none of the others.
            ExportField::Depth | ExportField::MaxTokens | ExportField::SchemaVersion => None,
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
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Reply> {
        use specforge_ops::export::{FORMAT, Format};

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
        // The file each renderer writes into out_dir.
        let file_name = match format {
            Format::Graph => "graph.json",
            Format::Dot => "graph.dot",
            Format::Context => "context.json",
            Format::Brief => "brief.json",
        };
        let name = FORMAT.name_of(format);

        // `graph` is the full graph export: Graph Protocol 2.0 with the schema,
        // as `specforge export --format graph` writes it.
        let request = specforge_ops::export::Request {
            format: Some(format),
            scope: args.scope.as_deref(),
            ..specforge_ops::export::Request::default()
        };
        let output = specforge_ops::export::export(&view, &request).map_err(Self::refused)?;

        // With out_dir the rendering lands on disk; without it, inline.
        let Some(out_dir) = args.out_dir.as_deref() else {
            return Ok(Reply {
                format: name.to_string(),
                output: Some(output),
                output_files: Vec::new(),
            }
            .into());
        };
        let Some(out_dir) = under_root(view.root(), out_dir) else {
            return Err(Box::new(
            McpError::new(
                ErrorCode::InvalidInput,
                format!(
                    "out_dir '{out_dir}' is relative and no project is served to resolve it against; give an absolute directory"
                ),
            )
            .with_argument("out_dir"),
        ));
        };
        let path = out_dir.join(file_name);
        if let Err(e) =
            std::fs::create_dir_all(&out_dir).and_then(|()| std::fs::write(&path, output))
        {
            return Err(Box::new(McpError::new(
                ErrorCode::InternalError,
                format!("failed to write {}: {e}", path.display()),
            )));
        }
        Ok(Reply {
            format: name.to_string(),
            output: None,
            output_files: vec![path.display().to_string()],
        }
        .into())
    }
}

/// `given` as a directory the call writes into: absolute as given, else
/// under the call's project root (ADR 0029 D8). `None` when it is relative
/// and the call has no root to resolve it against.
fn under_root(root: Option<&Path>, given: &str) -> Option<PathBuf> {
    let given = Path::new(given);
    if given.is_absolute() {
        return Some(given.to_path_buf());
    }
    root.map(|root| root.join(given))
}
