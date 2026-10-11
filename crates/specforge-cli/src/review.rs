use std::path::Path;

use specforge_ops::OpErrorKind;
use specforge_ops::coverage::STATUS;
use specforge_ops::review::{Review, ReviewRequest, review};
use specforge_ops::view::ProjectView;

use crate::OutputFormat;
use crate::outcome::{Exit, Refusal, print_json};
use crate::pipeline;

/// `specforge review [ENTITY]`: the review read view over the project
/// compiled at `path` (ADR 0015, "Prompt read views"), the document the
/// review prompt renders. An unknown entity is E003 (exit 1); a recorded
/// report that cannot be read is E045 (exit 2, as `stats`).
pub(crate) fn run(path: &Path, request: &ReviewRequest, format: OutputFormat) -> Exit {
    let project = pipeline::compile_project(path);
    match review(&ProjectView::of(&project), request) {
        Ok(review) => {
            match format {
                OutputFormat::Json => print_json(&review.document()),
                OutputFormat::Human => print_human(&review, request.depth),
            }
            Exit::Passed
        }
        Err(error) if error.kind == OpErrorKind::EntityNotFound => {
            Refusal::of(format).report(&error)
        }
        Err(error) => Refusal::measuring(format).report(&error),
    }
}

fn print_human(review: &Review, depth: usize) {
    match &review.entity_id {
        Some(entity) => {
            let hops = if depth == 1 { "hop" } else { "hops" };
            println!("Review of {entity} within {depth} {hops}");
        }
        None => println!("Review of the whole project"),
    }
    for row in &review.rows {
        println!(
            "{}  {}  {}  {}/{} proven",
            row.entity_id,
            row.kind,
            STATUS.name_of(row.status()),
            row.verdict.proven,
            row.verdict.obligations
        );
    }
    println!("Findings:");
    if review.findings.is_empty() {
        println!("  none");
    }
    for finding in &review.findings {
        println!(
            "{}  {}  {}",
            finding.gap.severity(),
            finding.entity_id,
            finding.message()
        );
    }
}
