use crate::OutputFormat;
use crate::outcome::{Exit, Refusal, Written, print_json};
use specforge_ops::extension::{self, AddOutcome, AddRequest, Added, Origin, Trust};
use specforge_ops_registry::ConfiguredRegistry;
use std::path::Path;

/// `specforge add`: the shared add operation, presented.
pub fn run(
    specifier: &str,
    path: &Path,
    format: OutputFormat,
    allow_unsigned: bool,
    trust: Trust,
) -> Exit {
    let source = match extension::parse(specifier) {
        Ok(source) => source,
        Err(error) => {
            return Refusal::of(format).report(&error);
        }
    };
    let registry =
        ConfiguredRegistry::for_project(path, "add").asking(crate::trust::ask_key_change);
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    let request = AddRequest {
        root: path,
        source,
        allow_unsigned,
        trust,
        dry_run: false,
    };
    let added = extension::add(&request, &registry, &runtime);
    // What reading the registry configuration reported, once the add asked a registry.
    format.eprint_diagnostics(&registry.reported());
    match added {
        Ok(added) => {
            present(&added, added.writes.names_under(path), format);
            Exit::Passed
        }
        // An install that failed after placing its module names it.
        Err(error) => Refusal::of(format).at(path).report(&error),
    }
}

/// The add's outcome; its JSON is the add document with `files_written`
/// (empty when the extension was already enabled).
fn present(added: &Added, files_written: Vec<String>, format: OutputFormat) {
    match format {
        OutputFormat::Json => print_json(&Written::of(added.document(), files_written)),
        OutputFormat::Human => print_human(&added.outcome),
    }
}

fn print_human(outcome: &AddOutcome) {
    match outcome {
        AddOutcome::Builtin {
            name,
            changed,
            peers_enabled,
        } => {
            for peer in peers_enabled {
                println!("enabled builtin {peer} (required by {name})");
            }
            if *changed {
                println!("enabled builtin {}", name);
            } else {
                println!("{} is already enabled", name);
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
                println!("installed {} from local path", name);
            }
            _ => {
                println!("installed {} v{}", name, version);
                if let Some(publisher) = publisher {
                    println!("  {}", publisher_line(publisher));
                }
            }
        },
        AddOutcome::AlreadyPresent { name, version } => {
            println!("{name} {version} is already installed");
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
