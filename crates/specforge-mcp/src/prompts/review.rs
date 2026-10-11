//! `specforge://prompts/review`: the coverage gaps of an entity's
//! neighbourhood, or of the whole graph. It renders the review
//! (`specforge_ops::review`).

use specforge_ops::review::{DEFAULT_DEPTH, ReviewRequest, review};

use crate::args::Arguments;
use crate::prompt::{PromptOutcome, Rendered};
use crate::tool::McpError;
use specforge_ops::view::ProjectView;

/// `specforge://prompts/review`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Entity ID to review (optional, reviews all if omitted)
    entity_id: Option<String>,
    // MCP prompt arguments have no `default` field, so the description
    // says it (ADR 0033 D9).
    /// Neighbor hops around entity_id to include (default 1)
    #[arg(default = DEFAULT_DEPTH)]
    depth: usize,
}

pub fn render(view: ProjectView<'_>, args: Args) -> PromptOutcome {
    let request = ReviewRequest {
        entity_id: args.entity_id.as_deref(),
        depth: args.depth,
    };
    let review = review(&view, &request).map_err(McpError::from)?;

    let scope = args.entity_id.as_deref().unwrap_or("the entire graph");
    let instruction = format!(
        "Analyze the following coverage report for {scope}. \
         Identify the highest-priority gaps to address. \
         Focus on entities marked 'uncovered' and unconnected entities that may indicate missing relationships."
    );
    Ok(Rendered {
        instruction,
        payload: serde_json::to_value(review.document()).expect("a document serializes"),
    })
}
