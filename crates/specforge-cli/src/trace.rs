use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::terminal::{Terminal, out};
use specforge_ops::trace::Target;
use specforge_ops::view::ProjectView;
use std::path::PathBuf;

// `specforge trace`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Entity ID to trace (omit to trace every entity)
    entity: Option<String>,

    #[arg(long, default_value = ".", help = crate::project::PATH_HELP)]
    path: PathBuf,

    /// Output format: json or human
    #[arg(long, default_value = "json")]
    format: OutputFormat,
}

impl Args {
    /// `specforge trace [entity]`: the trace operation over the project
    /// compiled at `path`: one entity's chain, or every entity's when none
    /// is named. Expected edges come from the loaded extensions'
    /// registries; the ones a chain lacks are reported as missing.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let project = match crate::project::compile(&self.path) {
            Ok(project) => project,
            Err(error) => return Refusal::of(self.format).report(terminal, &error),
        };
        let target = self.entity.as_deref().map_or(Target::Every, Target::Entity);
        let outcome = match specforge_ops::trace::trace(&ProjectView::of(&project), target) {
            Ok(outcome) => outcome,
            Err(error) => {
                return Refusal::of(self.format).report(terminal, &error.into());
            }
        };

        match self.format {
            OutputFormat::Human => out!(terminal, "{}", outcome.to_human()),
            OutputFormat::Json => terminal.json(&outcome),
        }
        Exit::Passed
    }
}
