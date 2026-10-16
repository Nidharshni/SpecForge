use crate::notes::Notes;
use crate::options;
use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, outln};
use serde_json::json;
use specforge_ops::registry::{Found, Registry};
use specforge_ops_registry::ConfiguredRegistry;
use specforge_protocol_types::DeclaredCategory;

// `specforge search`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Search query
    query: String,

    /// Only extensions that declare this category of contribution
    #[arg(long, value_parser = options::choice(&specforge_ops::registry::CONTRIBUTES))]
    contributes: Option<DeclaredCategory>,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let (query, contributes, format) =
            (self.query.as_str(), self.contributes, self.output.format);
        // No registry configured: fail before any network call (ADR 0004 N1).
        let root = match self.project.root() {
            Ok(root) => root,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let registry = ConfiguredRegistry::for_project(&root, "search");
        let searched = registry.search(query, contributes);
        let notes = Notes::new(terminal, format);
        notes.beside(&registry.reported());
        let searched = match searched {
            Ok(searched) => searched,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        // Each registry that failed, once.
        notes.beside(&searched.failures);
        print_found(
            query,
            contributes,
            &searched.found,
            &searched.failures,
            format,
            terminal,
        );
        Exit::of_verdict(!searched.failed())
    }
}

fn print_found(
    query: &str,
    contributes: Option<DeclaredCategory>,
    found: &[Found],
    failures: &[specforge_common::Diagnostic],
    format: OutputFormat,
    terminal: &Terminal,
) {
    match format {
        OutputFormat::Json => {
            let output = json!({
                "query": query,
                "contributes": contributes.map(DeclaredCategory::name),
                "results": found.iter().map(|r| json!({
                    "name": r.name,
                    "version": r.version,
                    "description": r.description,
                    "registry": r.registry,
                })).collect::<Vec<_>>(),
                "diagnostics": failures,
            });
            terminal.json(&output);
        }
        OutputFormat::Human => {
            let clause = contributes
                .map(|c| format!(" that declare {}", c.name()))
                .unwrap_or_default();
            if found.is_empty() {
                outln!(terminal, "no extensions found matching '{query}'{clause}");
                return;
            }
            outln!(
                terminal,
                "found {} extension(s) matching '{query}'{clause}:",
                found.len()
            );
            outln!(terminal);
            for result in found {
                outln!(
                    terminal,
                    "  {} v{}  ({})",
                    result.name,
                    result.version,
                    result.registry
                );
                if !result.description.is_empty() {
                    outln!(terminal, "    {}", result.description);
                }
            }
        }
    }
}
