use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal, Written};
use crate::terminal::{Terminal, errln, outln};
use specforge_ops::extension::{self, RemoveRequest};
use specforge_ops::view::ProjectView;
use specforge_project::Origin;

// `specforge remove`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Extension name
    name: String,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    /// Force removal even if other extensions depend on it
    #[arg(long)]
    force: bool,

    /// Report what the removal would do; write nothing
    #[arg(long)]
    dry_run: bool,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    /// `specforge remove`: the shared remove operation over the view of a fresh
    /// compile of the project, whose loaded declarations say which extensions
    /// depend on the one removed.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let root = match self.project.root() {
            Ok(root) => root,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let path = root.dir();
        let project = crate::project::compile_at(&root);
        let request = RemoveRequest {
            name: &self.name,
            force: self.force,
            dry_run: self.dry_run,
            caller: crate::caller::of(false, format, terminal),
        };
        let outcome = match extension::remove(&ProjectView::of(&project), &request) {
            Ok(outcome) => outcome,
            // A removal that failed after editing specforge.json names it.
            Err(error) => {
                return Refusal::of(format).at(path).report(terminal, &error);
            }
        };

        match format {
            OutputFormat::Json => {
                terminal.json(&Written::of(outcome.document(), &outcome.wrote(), path));
            }
            OutputFormat::Human => {
                let (removed, disabled) = if outcome.dry_run {
                    ("Would remove", "Would disable")
                } else {
                    ("Removed", "Disabled")
                };
                match &outcome.origin {
                    Origin::Builtin => {
                        outln!(terminal, "{disabled} builtin extension '{}'", outcome.name)
                    }
                    Origin::File { path } => outln!(
                        terminal,
                        "{disabled} extension '{}' (the .wasm file entry {path}; the file is left in place)",
                        outcome.name
                    ),
                    Origin::Installed { .. } => outln!(
                        terminal,
                        "{removed} extension '{}' (v{})",
                        outcome.name,
                        outcome.version.as_deref().unwrap_or("?")
                    ),
                    Origin::Unknown => outln!(
                        terminal,
                        "{removed} the specforge.json entry of '{}' (it was not installed)",
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
}
