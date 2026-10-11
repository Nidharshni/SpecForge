use crate::terminal::{Terminal, outln};
use std::path::Path;

use specforge_ops::OpErrorKind;
use specforge_ops::coverage::STATUS;
use specforge_ops::review::{Review, ReviewRequest, review};
use specforge_ops::view::ProjectView;

use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::pipeline;

/// `specforge review [ENTITY]`: the review read view over the project
/// compiled at `path` (ADR 0015, "Prompt read views"), the document the
/// review prompt renders. An unknown entity is E003 (exit 1); a recorded
/// report that cannot be read is E045 (exit 2, as `stats`).
pub(crate) fn run(
    path: &Path,
    request: &ReviewRequest,
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    let project = pipeline::compile_project(path);
    match review(&ProjectView::of(&project), request) {
        Ok(review) => {
            match format {
                OutputFormat::Json => terminal.json(&review.document()),
                OutputFormat::Human => print_human(&review, request.depth, terminal),
            }
            Exit::Passed
        }
        Err(error) if error.kind == OpErrorKind::EntityNotFound => {
            Refusal::of(format).report(terminal, &error)
        }
        Err(error) => Refusal::measuring(format).report(terminal, &error),
    }
}

fn print_human(review: &Review, depth: usize, terminal: &Terminal) {
    match &review.entity_id {
        Some(entity) => {
            let hops = if depth == 1 { "hop" } else { "hops" };
            outln!(terminal, "Review of {entity} within {depth} {hops}");
        }
        None => outln!(terminal, "Review of the whole project"),
    }
    for row in &review.rows {
        outln!(
            terminal,
            "{}  {}  {}  {}/{} proven",
            row.entity_id,
            row.kind,
            STATUS.name_of(row.status()),
            row.verdict.proven,
            row.verdict.obligations
        );
    }
    outln!(terminal, "Findings:");
    if review.findings.is_empty() {
        outln!(terminal, "  none");
    }
    for finding in &review.findings {
        outln!(
            terminal,
            "{}  {}  {}",
            finding.gap.severity(),
            finding.entity_id,
            finding.message()
        );
    }
}
