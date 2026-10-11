use crate::args::NoArgs;
use crate::reply::Answered;
use crate::target::View;
use crate::tool::McpError;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};
use specforge_ops::view::ProjectView;

/// `specforge.stats`'s reply: the statistics document
/// `specforge stats --format json` prints (ADR 0052).
type Reply = specforge_ops::stats::Stats;

/// `specforge.stats`: the stats operation over the call's project view, its
/// diagnostics what the server reports for the project (the view's
/// `reported`). The proof percentage reads the project's recorded tests; a
/// report that is there but unusable is an error result (ADR 0004, D2-e).
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.stats";
    const DESCRIPTION: &'static str = "Get project statistics";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpStatsResult");
    type Args = NoArgs;
    type Input = View;
    type Returns = Answered<Reply>;

    fn call(view: ProjectView<'_>, _args: NoArgs) -> Answered<Reply> {
        let stats = specforge_ops::stats::stats(&view).map_err(McpError::from)?;
        Ok(stats.into())
    }
}
