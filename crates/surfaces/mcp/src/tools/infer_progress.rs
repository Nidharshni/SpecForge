use crate::reply::Answered;
use crate::target::Project;
use crate::target::ProjectRef;
use crate::tool::McpError;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};

/// `specforge.infer_progress`'s reply: the document `specforge infer-status
/// --format json` prints (`McpInferProgressResult`).
type Reply = specforge_ops::infer::ProgressDocument;

/// `specforge.infer_progress`: the shared progress document
/// (`specforge_ops::infer::progress`). With no project served it is the
/// no-project refusal (the call target's), as the operation refuses a view
/// without a root.
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.infer_progress";
    const DESCRIPTION: &'static str = "Check inference progress: summary of analyzed vs unanalyzed source files, stale entries, and entity counts";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpInferProgressResult");
    type Args = crate::args::NoArgs;
    type Input = Project;
    type Returns = Answered<Reply>;

    fn call(project: ProjectRef<'_>, _args: crate::args::NoArgs) -> Answered<Reply> {
        let progress = specforge_ops::infer::progress(&project.view()).map_err(McpError::from)?;
        Ok(progress.document().into())
    }
}
