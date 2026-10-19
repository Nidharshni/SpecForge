use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, outln};
use specforge_ops::read::coverage::STATUS;
use specforge_ops::read::review::{Review, ReviewRequest, review};
use specforge_ops::view::ProjectView;

// `specforge review`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Entity ID to review (omit to review the whole project)
    entity: Option<String>,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    /// Hops around the entity (default 1; requires ENTITY)
    // clap's `requires` refuses a depth without an entity first; the reading is the operation's.
    #[arg(long, requires = "entity")]
    depth: Option<usize>,

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
    /// review prompt renders. An unknown entity is E003, a recorded report that
    /// cannot be read E045; either exits 1, as every read view refuses.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let request = self.request();
        let project = match self.project.compile() {
            Ok(project) => project,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        match review(&ProjectView::of(&project), &request) {
            Ok(review) => {
                match format {
                    OutputFormat::Json => terminal.json(&review.document()),
                    OutputFormat::Human => print_human(&review, terminal),
                }
                Exit::Passed
            }
            Err(error) => Refusal::of(format).report(terminal, &error),
        }
    }
}

fn print_human(review: &Review, terminal: &Terminal) {
    match (&review.entity_id, review.depth) {
        (Some(entity), Some(depth)) => {
            let hops = if depth == 1 { "hop" } else { "hops" };
            outln!(terminal, "Review of {entity} within {depth} {hops}");
        }
        _ => outln!(terminal, "Review of the whole project"),
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
