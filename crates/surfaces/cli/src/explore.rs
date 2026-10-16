use crate::notes::Notes;
use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, outln};
use specforge_ops::read::explore::{Exploration, ExplorationRequest, explore};
use specforge_ops::view::ProjectView;

// `specforge explore`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Entity ID to explore from (omit to explore the whole project)
    entity: Option<String>,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    /// Only entities of this kind
    #[arg(long)]
    kind: Option<String>,

    /// Hops from the entity (unbounded when omitted)
    #[arg(long)]
    depth: Option<usize>,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    /// The exploration the flags ask for.
    fn request(&self) -> ExplorationRequest<'_> {
        ExplorationRequest {
            entity_id: self.entity.as_deref(),
            kind: self.kind.as_deref(),
            depth: self.depth,
        }
    }

    /// `specforge explore [ENTITY]`: the exploration read view over the
    /// project compiled at `path` (ADR 0015, "Prompt read views"), the
    /// document the explore prompt renders. The notices (I020) go to
    /// stderr under human output and are the document's `notices` under
    /// JSON; a refusal (an unknown entity, E003) is `error[CODE]` with the
    /// closest entity as its hint (exit 1).
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let project = match self.project.compile() {
            Ok(project) => project,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let view = ProjectView::of(&project);
        match explore(&view, &self.request()) {
            Ok(exploration) => {
                Notes::new(terminal, format).carried(&exploration.notices);
                match format {
                    OutputFormat::Json => terminal.json(&exploration.document()),
                    OutputFormat::Human => print_human(&exploration, &view, terminal),
                }
                Exit::Passed
            }
            Err(error) => Refusal::of(format).report(terminal, &error),
        }
    }
}

/// `none` for an empty list, else the items joined by commas.
fn list(items: impl IntoIterator<Item = String>) -> String {
    let items: Vec<String> = items.into_iter().collect();
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

fn print_human(exploration: &Exploration, view: &ProjectView, terminal: &Terminal) {
    let connectivity = view.connectivity();
    let count = exploration.selected.len();
    let noun = if count == 1 { "entity" } else { "entities" };
    match &exploration.from {
        Some(from) => outln!(terminal, "Selected: {count} {noun} reached from {from}"),
        None => outln!(terminal, "Selected: {count} {noun}"),
    }
    outln!(
        terminal,
        "Starting points: {}",
        list(exploration.starting_points.iter().cloned())
    );
    outln!(
        terminal,
        "Most connected:  {}",
        list(
            exploration
                .most_connected
                .iter()
                .map(|id| format!("{id} ({})", connectivity.degree(id).total()))
        )
    );
    outln!(
        terminal,
        "Unconnected:     {}",
        list(exploration.unconnected.iter().cloned())
    );
    if let Some(from) = &exploration.from {
        outln!(terminal, "Paths from {from}:");
        for path in &exploration.paths {
            outln!(terminal, "  {}  {}", path.to, path.labels.join(", "));
        }
    }
}
