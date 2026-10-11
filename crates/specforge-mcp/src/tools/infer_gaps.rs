use crate::reply::Answered;
use crate::target::Project;
use crate::target::ProjectRef;
use crate::tool::McpError;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};

/// `specforge.infer_gaps`'s reply: the document `specforge infer-status
/// --gaps-detail --format json` prints under `gap_analysis`
/// (`McpInferGapsResult`).
type Reply = specforge_ops::infer::GapsDocument;

/// `specforge.infer_gaps`: the shared gap report
/// (`specforge_ops::infer::gaps`). With no project served it is the
/// no-project refusal (the call target's), as the operation refuses a view
/// without a root.
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.infer_gaps";
    const DESCRIPTION: &'static str =
        "Analyze inference gaps: public Rust items not yet covered by spec entities (approximate)";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpInferGapsResult");
    type Args = crate::args::NoArgs;
    type Input = Project;
    type Returns = Answered<Reply>;

    fn call(project: ProjectRef<'_>, _args: crate::args::NoArgs) -> Answered<Reply> {
        let gaps = specforge_ops::infer::gaps(&project.view()).map_err(McpError::from)?;
        Ok(gaps.document().into())
    }
}
