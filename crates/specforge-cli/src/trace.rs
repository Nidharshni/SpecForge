use crate::terminal::{Terminal, out};
use std::path::Path;

use specforge_ops::trace::Target;
use specforge_ops::view::ProjectView;

use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::pipeline;

/// `specforge trace [entity]`: the trace operation over the project
/// compiled at `path`: one entity's chain, or every entity's when none is
/// named. Expected edges come from the loaded extensions' registries; the
/// ones a chain lacks are reported as missing.
pub fn run(path: &Path, entity: Option<&str>, format: OutputFormat, terminal: &Terminal) -> Exit {
    let project = pipeline::compile_project(path);
    let target = entity.map_or(Target::Every, Target::Entity);
    let outcome = match specforge_ops::trace::trace(&ProjectView::of(&project), target) {
        Ok(outcome) => outcome,
        Err(error) => return Refusal::of(format).report(terminal, &error.into()),
    };

    match format {
        OutputFormat::Human => out!(terminal, "{}", outcome.to_human()),
        OutputFormat::Json => terminal.json(&outcome),
    }
    Exit::Passed
}
