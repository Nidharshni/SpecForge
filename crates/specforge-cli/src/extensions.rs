use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, outln};
use specforge_ops::extension;
use specforge_ops::view::ProjectView;
use std::path::PathBuf;

// `specforge extensions`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long, default_value = ".", help = crate::project::PATH_HELP)]
    path: PathBuf,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    /// `specforge extensions`: every extension the project enables, has
    /// installed or loaded, alphabetically, with the entity kinds each
    /// registered and how many of the project's entities use them.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let project = match crate::project::compile(&self.path) {
            Ok(project) => project,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let listing = extension::list(&ProjectView::of(&project));
        let entries = &listing.extensions;

        match format {
            OutputFormat::Json => terminal.json(&listing.document()),
            OutputFormat::Human => {
                if entries.is_empty() {
                    outln!(terminal, "No extensions installed.");
                    outln!(terminal);
                    outln!(terminal, "Install one with: specforge add <extension>");
                    return Exit::Passed;
                }
                outln!(terminal, "Installed extensions:");
                outln!(terminal);
                for entry in entries {
                    let version = entry
                        .version
                        .as_deref()
                        .map(|v| format!(" v{v}"))
                        .unwrap_or_default();
                    let provides = if entry.entity_kinds.is_empty() {
                        String::new()
                    } else {
                        format!(
                            ": {} entities ({})",
                            entry.entity_count,
                            entry.entity_kinds.join(", ")
                        )
                    };
                    let status = match entry.status {
                        extension::Status::Loaded => String::new(),
                        other => format!(" [{}]", other.as_str()),
                    };
                    outln!(
                        terminal,
                        "  {}{version} ({}){status}{provides}",
                        entry.name,
                        entry.source
                    );
                }
                outln!(terminal);
                outln!(terminal, "{} extension(s) installed.", entries.len());
            }
        }

        Exit::Passed
    }
}
