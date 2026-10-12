use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, errln, outln};
use specforge_ops::extension::{self, UpdateRequest};
use specforge_ops_registry::ConfiguredRegistry;
use specforge_package::Reach;
use std::path::PathBuf;

// `specforge update`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Extension name (updates all if omitted)
    name: Option<String>,

    /// Path to the project root
    #[arg(long, default_value = ".")]
    path: PathBuf,

    #[command(flatten)]
    output: OutputArg,

    /// Allow new major versions (otherwise each extension stays within
    /// its locked version's caret range); an update never moves an
    /// extension to a lower version
    #[arg(long, default_value_t = false)]
    major: bool,

    /// Accept unsigned packages (publisher verification skipped)
    #[arg(long, default_value_t = false)]
    allow_unsigned: bool,

    /// Accept key changes non-interactively (for CI)
    #[arg(long, default_value_t = false)]
    yes: bool,
}

impl Args {
    /// `specforge update`: the shared update operation, presented. Exit 1
    /// when it fails outright or when any extension fails (nothing is then
    /// applied).
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let path = self.path.as_path();
        let registry =
            ConfiguredRegistry::for_project(path, "update").asking(crate::trust::asker(terminal));
        let runtime = specforge_component::ComponentRuntime::with_user_cache();
        let request = UpdateRequest {
            root: path,
            name: self.name.as_deref(),
            reach: if self.major {
                Reach::Major
            } else {
                Reach::Compatible
            },
            allow_unsigned: self.allow_unsigned,
            trust: format.trust(self.yes),
        };
        let updated = extension::update(&request, &registry, &runtime);
        terminal.diagnostics(&registry.reported());
        let outcome = match updated {
            Ok(outcome) => outcome,
            Err(error) => {
                return Refusal::of(format).report(terminal, &error);
            }
        };
        // The update ran to the end (applied or rolled back): it emits
        // `batch_update_completed`, which JSON output carries in its document.
        if let OutputFormat::Json = format {
            terminal.json(&outcome.document(chrono::Utc::now()));
            return Exit::of_verdict(outcome.applied());
        }

        if outcome.failures().next().is_some() {
            for (name, e) in outcome.failures() {
                errln!(terminal, "error[{}]: {name}: {}", e.code, e.message);
                if let Some(hint) = &e.suggestion {
                    errln!(terminal, "  hint: {hint}");
                }
            }
            errln!(terminal, "no extension was updated");
            return Exit::Failed;
        }

        if outcome.extensions.is_empty() {
            outln!(terminal, "no extensions to update");
        } else if outcome.updated().next().is_none() {
            outln!(terminal, "all extensions are up to date");
        } else {
            outln!(
                terminal,
                "updated {} extension(s):",
                outcome.updated().count()
            );
            for e in &outcome.extensions {
                if let extension::UpdateStatus::Updated {
                    from,
                    to,
                    publisher,
                    ..
                } = &e.status
                {
                    outln!(terminal, "  {} {from} -> {to}", e.name);
                    outln!(terminal, "    {}", crate::add::publisher_line(publisher));
                }
            }
        }
        Exit::Passed
    }
}
