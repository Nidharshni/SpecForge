//! `specforge.extensions`: the installed extensions (`specforge_ops::extension::list`).

use crate::args::NoArgs;
use crate::reply::Answered;
use crate::target::ProjectRef;

/// `specforge.extensions`'s reply (`McpExtensionsResult`): the extensions
/// document `specforge extensions --format json` prints (ADR 0052).
pub use specforge_ops::extension::ExtensionsDocument as Reply;

pub(crate) fn call(project: &ProjectRef<'_>, _args: NoArgs) -> Answered<Reply> {
    // The shared listing, over the project view: what the project
    // compiled, its lock and the kinds its graph uses.
    Ok(specforge_ops::extension::list(&project.view())
        .document()
        .into())
}
