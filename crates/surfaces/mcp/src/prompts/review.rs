//! `specforge://prompts/review`: the coverage gaps of an entity's
//! neighbourhood, or of the whole graph. It renders the review
//! (`specforge_ops::read::review`).

use specforge_ops::read::review::{ReviewDocument, ReviewRequest, review};

use crate::args::Arguments;
use crate::prompt::{CorePrompt, Prompted};
use crate::refusal::ArgumentOf;
use crate::tool::McpError;
use specforge_ops::view::EntityField;
use specforge_ops::view::ProjectView;

/// The review prompt (`McpReviewPromptResult`).
pub struct Prompt;

impl ArgumentOf<EntityField> for Prompt {
    fn argument(field: EntityField) -> Option<&'static str> {
        match field {
            EntityField::EntityId => Some("entity_id"),
        }
    }
}

impl CorePrompt for Prompt {
    const NAME: &'static str = "specforge://prompts/review";
    const DESCRIPTION: &'static str = "Analyze coverage gaps for an entity or the whole graph";
    const SPEC: &'static str = "McpReviewPromptResult";
    type Args = Args;
    type Payload = ReviewDocument;

    fn render(
        view: ProjectView<'_>,
        args: Args,
    ) -> Result<Prompted<ReviewDocument>, Box<McpError>> {
        render(view, args)
    }
}

/// `specforge://prompts/review`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Entity ID to review (optional, reviews all if omitted)
    entity_id: Option<String>,
    // MCP prompt arguments have no `default` field, so the description
    // says it (ADR 0033 D9).
    /// Neighbor hops around entity_id to include (default 1; needs entity_id)
    depth: Option<usize>,
}

fn render(view: ProjectView<'_>, args: Args) -> Result<Prompted<ReviewDocument>, Box<McpError>> {
    let request = ReviewRequest {
        entity_id: args.entity_id.as_deref(),
        depth: args.depth,
    };
    let review = review(&view, &request).map_err(|refusal| Box::new(Prompt::refused(refusal)))?;

    let scope = args.entity_id.as_deref().unwrap_or("the entire graph");
    let instruction = format!(
        "Analyze the following coverage report for {scope}. \
         Identify the highest-priority gaps to address. \
         Focus on entities marked 'uncovered' and unconnected entities that may indicate missing relationships."
    );
    Ok(Prompted {
        instruction,
        payload: review.document(),
    })
}
