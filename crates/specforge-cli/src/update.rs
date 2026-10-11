use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::terminal::{Terminal, errln, outln};
use specforge_ops::extension::{self, Trust, UpdateRequest};
use specforge_ops_registry::ConfiguredRegistry;
use std::path::Path;

/// `specforge update`: the shared update operation, presented. Exit 1
/// when it fails outright or when any extension fails (nothing is then
/// applied).
pub fn run(
    name: Option<&str>,
    path: &Path,
    format: OutputFormat,
    major: bool,
    allow_unsigned: bool,
    trust: Trust,
    terminal: &Terminal,
) -> Exit {
    let registry =
        ConfiguredRegistry::for_project(path, "update").asking(crate::trust::ask_key_change);
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    let request = UpdateRequest {
        root: path,
        name,
        major,
        allow_unsigned,
        trust,
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
