use crate::target::Unscoped;
use crate::tool::{CoreTool, Effect, ReplySpec, ToolGroup};

use crate::args::Arguments;
use crate::reply::Answered;
use crate::tool::McpError;

/// `specforge.explain`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// A diagnostic code, any case
    code: String,
}

/// `specforge.explain`'s reply: the document `specforge explain --format json` prints, a
/// catalogued code's entry, or a retired code and the entry that replaced it.
pub use specforge_ops::explain::Explanation as Reply;

/// `specforge.explain`: the explain operation (`specforge_ops::explain`), as data. A code the
/// catalogue neither has nor retired is invalid input naming the `code` argument.
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.explain";
    const DESCRIPTION: &'static str = "Explain a diagnostic code: its title, owner, level, what triggers it and how to fix it, and its docs link";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpExplainResult");
    type Args = Args;
    type Input = Unscoped;
    type Returns = Answered<Reply>;

    fn call(_: (), args: Args) -> Answered<Reply> {
        specforge_ops::explain::explain(&args.code)
            .map(Into::into)
            .map_err(|error| Box::new(McpError::from(error).with_argument("code")))
    }
}
