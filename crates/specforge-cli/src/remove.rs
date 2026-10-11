use crate::outcome::{Exit, OutputFormat, Refusal, Written};
use crate::terminal::{Terminal, errln, outln};
use specforge_ops::extension::{self, Origin, RemoveRequest};
use specforge_ops::view::ProjectView;
use std::path::Path;

/// `specforge remove`: the shared remove operation over the view of a fresh
/// compile of the project, whose loaded declarations say which extensions
/// depend on the one removed.
pub fn run(
    name: &str,
    path: &Path,
    force: bool,
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    let project = crate::pipeline::compile_project(path);
    let request = RemoveRequest {
        name,
        force,
        dry_run: false,
    };
    let outcome = match extension::remove(&ProjectView::of(&project), &request) {
        Ok(outcome) => outcome,
        // A removal that failed after editing specforge.json names it.
        Err(error) => {
            return Refusal::of(format).at(path).report(terminal, &error);
        }
    };

    match format {
        OutputFormat::Json => terminal.json(&Written::of(
            outcome.document(),
            outcome.writes.names_under(path),
        )),
        OutputFormat::Human => {
            match &outcome.origin {
                Origin::Builtin => {
                    outln!(terminal, "Disabled builtin extension '{}'", outcome.name)
                }
                Origin::File { path } => outln!(
                    terminal,
                    "Disabled extension '{}' loaded from {path} (the file is left in place)",
                    outcome.name
                ),
                Origin::Installed { .. } => outln!(
                    terminal,
                    "Removed extension '{}' (v{})",
                    outcome.name,
                    outcome.version.as_deref().unwrap_or("?")
                ),
                Origin::Unknown => outln!(
                    terminal,
                    "Removed the specforge.json entry of '{}' (it was not installed)",
                    outcome.name
                ),
            }
            for entity in &outcome.stranded {
                errln!(terminal, "warning: {}", entity.warning(&outcome.name));
            }
        }
    }
    Exit::Passed
}
