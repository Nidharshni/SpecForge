use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, outln};
use specforge_ops::OpErrorKind;
use specforge_ops::coverage::STATUS;
use specforge_ops::review::{Review, ReviewRequest, review};
use specforge_ops::view::ProjectView;

// `specforge review`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Entity ID to review (omit to review the whole project)
    entity: Option<String>,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    /// Hops around the entity
    #[arg(long, default_value_t = specforge_ops::review::DEFAULT_DEPTH)]
    depth: usize,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    /// The review the flags ask for.
    fn request(&self) -> ReviewRequest<'_> {
        ReviewRequest {
            entity_id: self.entity.as_deref(),
            depth: self.depth,
        }
    }

    /// `specforge review [ENTITY]`: the review read view over the project
    /// compiled at `path` (ADR 0015, "Prompt read views"), the document the
    /// review prompt renders. An unknown entity is E003 (exit 1); a
    /// recorded report that cannot be read is E045 (exit 2, as `stats`).
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let request = self.request();
        let project = match self.project.compile() {
            Ok(project) => project,
            Err(error) => return Refusal::measuring(format).report(terminal, &error),
        };
        match review(&ProjectView::of(&project), &request) {
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
