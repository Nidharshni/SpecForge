//! `specforge://prompts/explore`: where to start exploring the graph. It
//! renders the exploration (`specforge_ops::explore`).

use specforge_ops::explore::{ExplorationDocument, ExplorationRequest, explore};

use crate::args::Arguments;
use crate::prompt::{CorePrompt, Prompted};
use crate::tool::McpError;
use specforge_ops::view::ProjectView;

/// The explore prompt (`McpExplorePromptResult`).
pub struct Prompt;

impl CorePrompt for Prompt {
    const NAME: &'static str = "specforge://prompts/explore";
    const DESCRIPTION: &'static str = "Discover exploration starting points in the graph";
    const SPEC: &'static str = "McpExplorePromptResult";
    type Args = Args;
    type Payload = ExplorationDocument;

    fn render(
        view: ProjectView<'_>,
        args: Args,
    ) -> Result<Prompted<ExplorationDocument>, Box<McpError>> {
        render(view, args)
    }
}

/// `specforge://prompts/explore`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Starting entity (optional)
    entity_id: Option<String>,
    /// Filter by entity kind
    kind: Option<String>,
    /// Hops from entity_id the exploration reaches (unbounded if omitted)
    depth: Option<usize>,
}

const INSTRUCTION: &str = "Explore the spec graph using the data below. \
     Start with high-connectivity entities to understand the core structure, \
     then investigate unconnected entities that may need relationships. \
     Use starting_points for top-down traversal.";

fn render(
    view: ProjectView<'_>,
    args: Args,
) -> Result<Prompted<ExplorationDocument>, Box<McpError>> {
    let request = ExplorationRequest {
        entity_id: args.entity_id.as_deref(),
        kind: args.kind.as_deref(),
        depth: args.depth,
    };
    let exploration = explore(&view, &request).map_err(McpError::from)?;
    Ok(Prompted {
        instruction: INSTRUCTION.to_string(),
        payload: exploration.document(),
    })
}
