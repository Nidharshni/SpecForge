//! `specforge://prompts/explore`: where to start exploring the graph. It
//! renders the exploration (`specforge_ops::explore`).

use specforge_ops::explore::{ExplorationRequest, explore};

use crate::args::Arguments;
use crate::prompt::{PromptOutcome, Rendered};
use crate::tool::McpError;
use specforge_ops::view::ProjectView;

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

pub fn render(view: ProjectView<'_>, args: Args) -> PromptOutcome {
    let request = ExplorationRequest {
        entity_id: args.entity_id.as_deref(),
        kind: args.kind.as_deref(),
        depth: args.depth,
    };
    let exploration = explore(&view, &request).map_err(McpError::from)?;
    Ok(Rendered {
        instruction: INSTRUCTION.to_string(),
        payload: serde_json::to_value(exploration.document()).expect("a document serializes"),
    })
}
