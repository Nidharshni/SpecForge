use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, outln};
use specforge_ops::extension;
use specforge_ops::view::ProjectView;

// `specforge extensions`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: crate::project::ProjectArg,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    /// `specforge extensions`: every extension the project enables, has
    /// locked or loaded, alphabetically, with the entity kinds each
    /// registered and how many of the project's entities use them.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let project = match self.project.compile() {
            Ok(project) => project,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let listing = extension::list(&ProjectView::of(&project));
        let entries = &listing.extensions;

        match format {
            OutputFormat::Json => terminal.json(&listing.document()),
            OutputFormat::Human => {
                if entries.is_empty() {
                    outln!(terminal, "This project has no extensions.");
                    outln!(terminal);
                    outln!(terminal, "Add one with: specforge add <extension>");
                    return Exit::Passed;
                }
                outln!(terminal, "Extensions of the project:");
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
                            ": {} {} ({})",
                            entry.entity_count,
                            if entry.entity_count == 1 {
                                "entity"
                            } else {
                                "entities"
                            },
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
                let loaded = entries
                    .iter()
                    .filter(|e| e.status == extension::Status::Loaded)
                    .count();
                outln!(terminal, "{} extension(s), {loaded} loaded.", entries.len());
            }
        }

        Exit::Passed
    }
}
