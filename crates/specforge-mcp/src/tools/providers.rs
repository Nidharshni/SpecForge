//! `specforge.providers`: the configured providers (`specforge_ops::extension::providers`).

use crate::args::NoArgs;
use crate::reply::Answered;
use crate::target::Project;
use crate::target::ProjectRef;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};

/// `specforge.providers`'s reply: the document `specforge providers
/// --format json` prints (`McpProvidersResult`).
type Reply = specforge_ops::extension::ProvidersDocument;

pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.providers";
    const DESCRIPTION: &'static str = "List configured providers";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Management,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpProvidersResult");
    type Args = NoArgs;
    type Input = Project;
    type Returns = Answered<Reply>;

    fn call(project: ProjectRef<'_>, _args: NoArgs) -> Answered<Reply> {
        // The providers specforge.json configures, as the scheme registry built
        // from the loaded extensions sees them: the listing the CLI prints.
        let listing = specforge_ops::extension::providers(&project.view());
        Ok(listing.document().into())
    }
}
