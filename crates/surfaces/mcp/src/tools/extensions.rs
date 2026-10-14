//! `specforge.extensions`: the installed extensions (`specforge_ops::extension::list`).

use crate::args::NoArgs;
use crate::reply::Answered;
use crate::target::Project;
use crate::target::ProjectRef;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};

/// `specforge.extensions`'s reply: the extensions
/// document `specforge extensions --format json` prints (ADR 0052).
type Reply = specforge_ops::extension::ExtensionsDocument;

pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.extensions";
    const DESCRIPTION: &'static str = "List installed extensions";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Management,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpExtensionsResult");
    type Args = NoArgs;
    type Input = Project;
    type Returns = Answered<Reply>;

    fn call(project: ProjectRef<'_>, _args: NoArgs) -> Answered<Reply> {
        // The shared listing, over the project view: what the project
        // compiled, its lock and the kinds its graph uses.
        Ok(specforge_ops::extension::list(&project.view())
            .document()
            .into())
    }
}
