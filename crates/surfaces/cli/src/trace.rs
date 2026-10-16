use crate::notes::Notes;
use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::terminal::{Terminal, out};
use specforge_ops::read::plan::AgentPlan;
use specforge_ops::read::trace::Ask;
use specforge_ops::view::ProjectView;

// `specforge trace`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Entity ID to trace (omit to trace every entity)
    entity: Option<String>,

    /// An agent plan file (an AgentPlan's JSON) to check against the graph instead of tracing
    #[arg(long, value_name = "FILE", value_parser = plan_file, conflicts_with = "entity")]
    plan: Option<AgentPlan>,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    /// Output format: json or human
    #[arg(long, default_value = "json")]
    format: OutputFormat,
}

/// The AgentPlan the file at `path` holds: clap's refusal (exit 2) when it
/// cannot be read or is not one.
fn plan_file(path: &str) -> Result<AgentPlan, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    AgentPlan::parse(&text).map_err(|error| error.message("plan"))
}

impl Args {
    /// `specforge trace [entity]`: the trace operation over the project
    /// compiled at `path`: one entity's chain, or every entity's when none
    /// is named. Expected edges come from the loaded extensions'
    /// registries; the ones a chain lacks are reported as missing. With
    /// `--plan`, the plan check of an agent plan file instead (exit 1 when
    /// the plan fails).
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let project = match self.project.compile() {
            Ok(project) => project,
            Err(error) => return Refusal::of(self.format).report(terminal, &error),
        };
        let view = ProjectView::of(&project);
        // clap refuses an entity with --plan first; the reading is the operation's all the same.
        let target = match Ask::of(self.entity.as_deref(), self.plan.as_ref()) {
            Err(refusal) => {
                return Refusal::of(self.format).report(terminal, &refusal.into_error());
            }
            Ok(Ask::Plan(plan)) => {
                let outcome = specforge_ops::read::plan::check(&view, plan);
                match self.format {
                    OutputFormat::Human => out!(terminal, "{}", outcome.to_human()),
                    OutputFormat::Json => terminal.json(&outcome),
                }
                return Exit::of_verdict(outcome.ok);
            }
            Ok(Ask::Trace(target)) => target,
        };
        let traced = match specforge_ops::read::trace::trace(&view, target) {
            Ok(traced) => traced,
            Err(refusal) => return Refusal::of(self.format).report(terminal, &refusal),
        };

        let outcome = Notes::new(terminal, self.format).noticed(traced);
        match self.format {
            OutputFormat::Human => out!(terminal, "{}", outcome.to_human()),
            OutputFormat::Json => terminal.json(&outcome),
        }
        Exit::Passed
    }
}
