use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal, Written};
use crate::terminal::{Terminal, outln};
use specforge_ops::extension::{self, AddOutcome, AddRequest, Added, Origin};
use specforge_ops_registry::ConfiguredRegistry;
use std::path::PathBuf;

// `specforge add`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Extension specifier (e.g., @scope/name@1.0.0 or ./path)
    specifier: String,

    /// Path to the project root
    #[arg(long, default_value = ".")]
    path: PathBuf,

    #[command(flatten)]
    output: OutputArg,

    /// Accept unsigned packages (publisher verification skipped)
    #[arg(long, default_value_t = false)]
    allow_unsigned: bool,

    /// Accept key changes non-interactively (for CI)
    #[arg(long, default_value_t = false)]
    yes: bool,
}

impl Args {
    /// `specforge add`: the shared add operation, presented.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let path = self.path.as_path();
        let source = match extension::parse(&self.specifier) {
            Ok(source) => source,
            Err(error) => {
                return Refusal::of(format).report(terminal, &error);
            }
        };
        let registry =
            ConfiguredRegistry::for_project(path, "add").asking(crate::trust::asker(terminal));
        let runtime = specforge_component::ComponentRuntime::with_user_cache();
        let request = AddRequest {
            root: path,
            source,
            allow_unsigned: self.allow_unsigned,
            trust: format.trust(self.yes),
            dry_run: false,
        };
        let added = extension::add(&request, &registry, &runtime);
        // What reading the registry configuration reported, once the add
        // asked a registry.
        terminal.diagnostics(&registry.reported());
        match added {
            Ok(added) => {
                present(&added, added.writes.names_under(path), format, terminal);
                Exit::Passed
            }
            // An install that failed after placing its module names it.
            Err(error) => Refusal::of(format).at(path).report(terminal, &error),
        }
    }
}

/// The add's outcome; its JSON is the add document with `files_written`
/// (empty when the extension was already enabled).
fn present(added: &Added, files_written: Vec<String>, format: OutputFormat, terminal: &Terminal) {
    match format {
        OutputFormat::Json => terminal.json(&Written::of(added.document(), files_written)),
        OutputFormat::Human => print_human(&added.outcome, terminal),
    }
}

fn print_human(outcome: &AddOutcome, terminal: &Terminal) {
    match outcome {
        AddOutcome::Builtin {
            name,
            changed,
            peers_enabled,
        } => {
            for peer in peers_enabled {
                outln!(terminal, "enabled builtin {peer} (required by {name})");
            }
            if *changed {
                outln!(terminal, "enabled builtin {}", name);
            } else {
                outln!(terminal, "{} is already enabled", name);
            }
        }
        AddOutcome::Installed {
            name,
            version,
            publisher,
            origin,
            ..
        } => match origin {
            Origin::Installed { source } if !source.is_registry() => {
                outln!(terminal, "installed {} from local path", name);
            }
            _ => {
                outln!(terminal, "installed {} v{}", name, version);
                if let Some(publisher) = publisher {
                    outln!(terminal, "  {}", publisher_line(publisher));
                }
            }
        },
        AddOutcome::AlreadyPresent { name, version } => {
            outln!(terminal, "{name} {version} is already installed");
        }
        // `specforge add` has no dry run.
        AddOutcome::Planned { .. } => {}
    }
}

/// How the install's publisher is told on the human surface.
pub(crate) fn publisher_line(publisher: &specforge_ops::registry::Publisher) -> String {
    use specforge_ops::registry::Publisher;
    match publisher {
        Publisher::Unsigned => "unsigned".to_string(),
        Publisher::Signed {
            key_id,
            first_use: true,
        } => format!("signed by key: {key_id} (pinned on first use)"),
        Publisher::Signed { key_id, .. } => format!("signed by key: {key_id}"),
        Publisher::Repinned { key_id, previous } => {
            format!("signed by key: {key_id} (re-pinned; was {previous})")
        }
    }
}
