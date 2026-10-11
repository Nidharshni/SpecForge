use crate::OutputFormat;
use crate::outcome::{Exit, Refusal, print_json};
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
    format.eprint_diagnostics(&registry.reported());
    let outcome = match updated {
        Ok(outcome) => outcome,
        Err(error) => {
            return Refusal::of(format).report(&error);
        }
    };
    // The update ran to the end (applied or rolled back): it emits
    // `batch_update_completed`, which JSON output carries in its document.
    if let OutputFormat::Json = format {
        print_json(&outcome.document(chrono::Utc::now()));
        return Exit::of_verdict(outcome.applied());
    }

    if outcome.failures().next().is_some() {
        for (name, e) in outcome.failures() {
            eprintln!("error[{}]: {name}: {}", e.code, e.message);
            if let Some(hint) = &e.suggestion {
                eprintln!("  hint: {hint}");
            }
        }
        eprintln!("no extension was updated");
        return Exit::Failed;
    }

    if outcome.extensions.is_empty() {
        println!("no extensions to update");
    } else if outcome.updated().next().is_none() {
        println!("all extensions are up to date");
    } else {
        println!("updated {} extension(s):", outcome.updated().count());
        for e in &outcome.extensions {
            if let extension::UpdateStatus::Updated {
                from,
                to,
                publisher,
                ..
            } = &e.status
            {
                println!("  {} {from} -> {to}", e.name);
                println!("    {}", crate::add::publisher_line(publisher));
            }
        }
    }
    Exit::Passed
}
