use crate::target::View;
use crate::tool::{CoreTool, Effect, ToolGroup};
use specforge_ops::model::{self, DependencyDepth, OutlineDetail, OutlineFormat, OutlineOptions};

use crate::args::Arguments;
use crate::reply::{Answered, Text};
use specforge_ops::view::ProjectView;

/// `specforge.outline_extensions`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Output format; json is meant for programs
    #[arg(choice = specforge_ops::model::OUTLINE_FORMAT)]
    format: OutlineFormat,
    /// Detail level
    #[arg(choice = specforge_ops::model::OUTLINE_FIELDS)]
    fields: OutlineDetail,
    /// Dependency visibility
    #[arg(choice = specforge_ops::model::DEPS)]
    deps: DependencyDepth,
}

/// `specforge.outline_extensions`: the outline operation over the served
/// project.
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.outline_extensions";
    const DESCRIPTION: &'static str = "Renders the extension architecture hierarchy — how extensions relate via dependencies, enhancements, and cross-extension edges. Shows entity kinds, edge types, validation rules, and surface contributions per extension. Use this to understand the project's extension topology before making structural changes.";
    const EFFECT: Effect = Effect::Reads {
        group: ToolGroup::Core,
    };
    type Args = Args;
    type Input = View;
    type Returns = Answered<Text>;

    fn call(view: ProjectView<'_>, args: Args) -> Answered<Text> {
        Ok(Text(model::outline(&view, &options(&args))).into())
    }
}

/// The outline options the arguments name; an absent one is its table's
/// default, as `specforge outline` reads it (ADR 0027).
fn options(args: &Args) -> OutlineOptions {
    OutlineOptions {
        format: args.format,
        detail: args.fields,
        deps: args.deps,
    }
}
