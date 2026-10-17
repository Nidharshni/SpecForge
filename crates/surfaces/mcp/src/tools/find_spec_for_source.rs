use crate::target::Project;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use specforge_ops::read::navigate;

use crate::args::Arguments;
use crate::reply::Answered;
use crate::target::ProjectRef;
use crate::tool::McpError;

/// `specforge.find_spec_for_source`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Path of the source file, relative to the project root (a directory lists the files under it; a trailing part of a path matches it)
    file_path: String,
}

/// `specforge.find_spec_for_source`'s reply: the anchors read view's file document (ADR 0140).
pub use specforge_ops::read::navigate::FileAnchors as Reply;

/// The entities anchored to the source files `file_path` names, under the
/// one file rule (`specforge_ops::read::navigate::anchors_of_file`).
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.find_spec_for_source";
    const DESCRIPTION: &'static str = "Find specforge entities anchored to a source file";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Navigation,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpSpecForSourceResult");
    type Args = Args;
    type Input = Project;
    type Returns = Answered<Reply>;

    fn call(project: ProjectRef<'_>, args: Args) -> Answered<Reply> {
        let found =
            navigate::anchors_of_file(&project.view(), &args.file_path).map_err(McpError::from)?;
        Ok(found.into())
    }
}
