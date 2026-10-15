use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::terminal::{Terminal, out};
use specforge_ops::read::plan::AgentPlan;
use specforge_ops::read::trace::{Ask, Target};
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
        // clap refuses an entity with --plan first; the reading is the operation's all the same.
        let ask = match Ask::of(self.entity.as_deref(), self.plan.as_ref()) {
            Ok(ask) => ask,
            Err(refusal) => {
                return Refusal::of(self.format).report(terminal, &refusal.into_error());
            }
        };
        if let Ask::Plan(plan) = ask {
            let outcome = specforge_ops::read::plan::check(&ProjectView::of(&project), plan);
            match self.format {
                OutputFormat::Human => out!(terminal, "{}", outcome.to_human()),
                OutputFormat::Json => terminal.json(&outcome),
            }
            return Exit::of_verdict(outcome.ok);
        }
        let target = match ask {
            Ask::Entity(entity) => Target::Entity(entity),
            Ask::Every | Ask::Plan(_) => Target::Every,
        };
        let outcome = match specforge_ops::read::trace::trace(&ProjectView::of(&project), target) {
            Ok(outcome) => outcome,
            Err(error) => {
                return Refusal::of(self.format)
                    .report(terminal, &specforge_ops::OpError::from(error));
            }
        };

        match self.format {
            OutputFormat::Human => out!(terminal, "{}", outcome.to_human()),
            OutputFormat::Json => terminal.json(&outcome),
        }
        Exit::Passed
    }
}
