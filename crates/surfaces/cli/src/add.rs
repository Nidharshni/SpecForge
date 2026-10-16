use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal, Written};
use crate::terminal::{Terminal, outln};
use specforge_common::ProjectPath;
use specforge_ops::Containment;
use specforge_ops::extension::{self, AddOutcome, AddRequest, Added, AddedExtension, Origin};
use specforge_ops_registry::ConfiguredRegistry;

// `specforge add`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Extension specifiers (e.g., @scope/name@1.0.0 or ./path), added as one change
    #[arg(required = true, num_args = 1..)]
    specifiers: Vec<String>,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    #[command(flatten)]
    output: OutputArg,

    /// Accept unsigned packages (publisher verification skipped)
    #[arg(long, default_value_t = false)]
    allow_unsigned: bool,

    /// Accept key changes non-interactively (for CI)
    #[arg(long, default_value_t = false)]
    yes: bool,

    /// Report what the add would do; write, pin and download nothing
    #[arg(long)]
    dry_run: bool,
}

impl Args {
    /// `specforge add`: the shared add operation, presented.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let root = match self.project.root() {
            Ok(root) => root,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let path = root.dir();
        let extensions = match extension::specified(&self.specifiers) {
            Ok(extensions) => extensions,
            Err(error) => {
                return Refusal::of(format).report(terminal, &error);
            }
        };
        let registry =
            ConfiguredRegistry::for_project(&root, "add").asking(crate::trust::asker(terminal));
        let runtime = specforge_component::ComponentRuntime::with_user_cache();
        let request = AddRequest {
            root: &root,
            extensions,
            allow_unsigned: self.allow_unsigned,
            trust: format.trust(self.yes),
            dry_run: self.dry_run,
            containment: &Containment::unbounded(),
        };
        let added = extension::add(&request, &registry, &runtime);
        // What reading the registry configuration reported, once the add
        // asked a registry.
        terminal.diagnostics(&registry.reported());
        match added {
            Ok(added) => {
                present(&added, added.writes.named(Some(path)), format, terminal);
                Exit::Passed
            }
            // An install that failed after placing its module names it.
            Err(error) => Refusal::of(format).at(path).report(terminal, &error),
        }
    }
}

/// The add's outcome; its JSON is the add document with `files_written`
/// (empty when every extension was already enabled).
fn present(
    added: &Added,
    files_written: Vec<ProjectPath>,
    format: OutputFormat,
    terminal: &Terminal,
) {
    match format {
        OutputFormat::Json => terminal.json(&Written::of(added.document(), files_written)),
        OutputFormat::Human => {
            for extension in &added.added {
                print_human(extension, terminal);
            }
        }
    }
}

fn print_human(added: &AddedExtension, terminal: &Terminal) {
    match &added.outcome {
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
            peers_enabled,
            ..
        } => {
            for peer in peers_enabled {
                outln!(terminal, "enabled builtin {peer} (required by {name})");
            }
            match origin {
                Origin::Installed { source } if !source.is_registry() => {
                    outln!(terminal, "installed {} from local path", name);
                }
                _ => {
                    outln!(terminal, "installed {} v{}", name, version);
                    if let Some(publisher) = publisher {
                        outln!(terminal, "  {}", publisher_line(publisher));
                    }
                }
            }
        }
        AddOutcome::AlreadyPresent { name, version } => {
            outln!(terminal, "{name} {version} is already installed");
        }
        AddOutcome::Duplicate { name, first } => {
            outln!(
                terminal,
                "{} names {name}, which {first} adds",
                added.specifier
            );
        }
        // A dry run: what the add would do.
        AddOutcome::Planned {
            name,
            version,
            origin,
            peers_enabled,
        } => {
            for peer in peers_enabled {
                outln!(terminal, "would enable builtin {peer} (required by {name})");
            }
            match version {
                None => outln!(terminal, "would enable builtin {name}"),
                Some(version) => outln!(
                    terminal,
                    "would install {name} v{version} ({})",
                    origin.source()
                ),
            }
        }
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
