use std::path::Path;

use specforge_ops::explore::{Exploration, ExplorationRequest, explore};
use specforge_ops::view::ProjectView;

use crate::OutputFormat;
use crate::outcome::{Exit, Refusal, print_json};
use crate::pipeline;

/// `specforge explore [ENTITY]`: the exploration read view over the project
/// compiled at `path` (ADR 0015, "Prompt read views"), the document the
/// explore prompt renders. The notices (I020) go to stderr; a refusal (an
/// unknown entity, E003) is `error[CODE]` with the closest entity as its
/// hint (exit 1).
pub(crate) fn run(path: &Path, request: &ExplorationRequest, format: OutputFormat) -> Exit {
    let project = pipeline::compile_project(path);
    let view = ProjectView::of(&project);
    match explore(&view, request) {
        Ok(exploration) => {
            for notice in &exploration.notices {
                eprintln!("{}", specforge_common::render_plain(notice));
            }
            match format {
                OutputFormat::Json => print_json(&exploration.document()),
                OutputFormat::Human => print_human(&exploration, &view),
            }
            Exit::Passed
        }
        Err(error) => Refusal::of(format).report(&error),
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

fn print_human(exploration: &Exploration, view: &ProjectView) {
    let connectivity = view.connectivity();
    let count = exploration.selected.len();
    let noun = if count == 1 { "entity" } else { "entities" };
    match &exploration.from {
        Some(from) => println!("Selected: {count} {noun} reached from {from}"),
        None => println!("Selected: {count} {noun}"),
    }
    println!(
        "Starting points: {}",
        list(exploration.starting_points.iter().cloned())
    );
    println!(
        "Most connected:  {}",
        list(
            exploration
                .most_connected
                .iter()
                .map(|id| format!("{id} ({})", connectivity.degree(id).total()))
        )
    );
    println!(
        "Unconnected:     {}",
        list(exploration.unconnected.iter().cloned())
    );
    if let Some(from) = &exploration.from {
        println!("Paths from {from}:");
        for path in &exploration.paths {
            println!("  {}  {}", path.to, path.labels.join(", "));
        }
    }
}
