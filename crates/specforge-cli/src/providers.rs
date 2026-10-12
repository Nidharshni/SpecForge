use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, errln, outln};
use specforge_ops::extension;
use specforge_ops::view::ProjectView;
use std::path::PathBuf;

// `specforge providers`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long, default_value = ".", help = crate::project::PATH_HELP)]
    path: PathBuf,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    /// `specforge providers`: the providers `specforge.json` configures, in
    /// declaration order, each with the status the scheme registry built from
    /// the loaded extensions gives it.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let project = match crate::project::compile(&self.path) {
            Ok(project) => project,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let listing = extension::providers(&ProjectView::of(&project));
        let (providers, diagnostics) = (&listing.providers, &listing.diagnostics);

        match format {
            OutputFormat::Json => {
                terminal.json(&listing.document());
            }
            OutputFormat::Human => {
                for diagnostic in diagnostics {
                    let severity = match diagnostic.severity {
                        specforge_common::Severity::Error => "error",
                        specforge_common::Severity::Warning => "warning",
                        specforge_common::Severity::Info => "info",
                    };
                    errln!(
                        terminal,
                        "{severity}[{}]: {}",
                        diagnostic.code,
                        diagnostic.message
                    );
                }
                if providers.is_empty() {
                    outln!(terminal, "No providers configured.");
                    return Exit::Passed;
                }
                outln!(terminal, "Configured providers:");
                outln!(terminal);
                for provider in providers {
                    outln!(
                        terminal,
                        "  {} (extension: {})",
                        provider.alias,
                        provider.extension
                    );
                    outln!(
                        terminal,
                        "    scheme: {} [{}]",
                        provider.scheme,
                        provider.status.as_str()
                    );
                }
                outln!(terminal);
                outln!(terminal, "{} provider(s) configured.", providers.len());
            }
        }

        Exit::Passed
    }
}
