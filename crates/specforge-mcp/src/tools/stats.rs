use crate::args::NoArgs;
use crate::reply::Answered;
use crate::tool::McpError;
use specforge_ops::view::ProjectView;

/// `specforge.stats`'s reply (`McpStatsResult`): the statistics document
/// `specforge stats --format json` prints (ADR 0052).
pub use specforge_ops::stats::Stats as Reply;

/// `specforge.stats`: the stats operation over the call's project view, its
/// diagnostics what the server reports for the project (the view's
/// `reported`). The proof percentage reads the project's recorded tests; a
/// report that is there but unusable is an error result (ADR 0004, D2-e).
pub fn call(view: ProjectView<'_>, _args: NoArgs) -> Answered<Reply> {
    let stats = specforge_ops::stats::stats(&view).map_err(McpError::from)?;
    Ok(stats.into())
}
