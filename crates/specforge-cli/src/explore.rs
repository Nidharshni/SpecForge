use crate::terminal::{Terminal, errln, outln};
use std::path::Path;

use specforge_ops::explore::{Exploration, ExplorationRequest, explore};
use specforge_ops::view::ProjectView;

use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::pipeline;

/// `specforge explore [ENTITY]`: the exploration read view over the project
/// compiled at `path` (ADR 0015, "Prompt read views"), the document the
/// explore prompt renders. The notices (I020) go to stderr; a refusal (an
/// unknown entity, E003) is `error[CODE]` with the closest entity as its
/// hint (exit 1).
pub(crate) fn run(
    path: &Path,
    request: &ExplorationRequest,
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    let project = pipeline::compile_project(path);
    let view = ProjectView::of(&project);
    match explore(&view, request) {
        Ok(exploration) => {
            for notice in &exploration.notices {
                errln!(terminal, "{}", specforge_common::render_plain(notice));
            }
            match format {
                OutputFormat::Json => terminal.json(&exploration.document()),
                OutputFormat::Human => print_human(&exploration, &view, terminal),
            }
            Exit::Passed
        }
        Err(error) => Refusal::of(format).report(terminal, &error),
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
