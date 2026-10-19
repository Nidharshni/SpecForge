//! `specforge publish`: the publish operation, presented.

use crate::notes::Notes;
use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, outln};
use specforge_component::ComponentRuntime;
use specforge_ops::extension::{PublishOutcome, publish};
use specforge_ops_registry::ConfiguredRegistry;
use std::path::PathBuf;

// `specforge publish`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// The extension to publish: a .wasm component, or the extension's
    /// crate directory (its target/wasm32-wasip2/release component).
    /// Defaults to the --path directory
    extension: Option<PathBuf>,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    /// Publish the extension at `extension` (a `.wasm` component or its
    /// crate directory) to the registry that serves its name among those
    /// `project`'s `specforge.json` configures (ADR 0045). The package's
    /// manifest is the declaration read from the binary (ADR 0012): a
    /// binary whose declaration has errors is refused before any network
    /// call. The declaration's warnings, then what reading the registry
    /// configuration reported, go to stderr whatever the result.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let extension = self.extension.as_deref().unwrap_or(self.project.path());
        let root = match self.project.root() {
            Ok(root) => root,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let registry = ConfiguredRegistry::for_project(&root, "publish");
        let runtime = ComponentRuntime::with_user_cache();
        let report = publish(extension, &registry, &runtime);
        let notes = Notes::new(terminal, format);
        notes.beside(&report.warnings);
        notes.beside(&registry.reported());
        match &report.result {
            Ok(outcome) => {
                present(outcome, format, terminal);
                Exit::Passed
            }
            Err(error) => Refusal::of(format).report(terminal, error),
        }
    }
}

fn present(outcome: &PublishOutcome, format: OutputFormat, terminal: &Terminal) {
    let published = &outcome.published;
    match format {
        OutputFormat::Json => terminal.json(&outcome.document()),
        OutputFormat::Human => {
            if published.key_created {
                outln!(
                    terminal,
                    "generated publisher signing key {}",
                    published.key_id
                );
            }
            outln!(terminal, "published {} v{}", outcome.name, outcome.version);
            outln!(terminal, "  registry: {}", published.registry);
            outln!(terminal, "  url: {}", published.url);
            outln!(terminal, "  size: {} bytes", outcome.size_bytes);
            outln!(terminal, "  signed by key: {}", published.key_id);
        }
    }
}
