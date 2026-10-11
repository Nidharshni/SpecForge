use crate::OutputFormat;
use crate::outcome::{Exit, Refusal, Written, print_json};
use specforge_common::find_project_root;
use specforge_ops::init;
use std::path::Path;

/// `specforge init`: the shared scaffold (`specforge_ops::init`), planned
/// and validated before anything is written, then presented.
pub fn run(
    path: &Path,
    name: Option<&str>,
    version: &str,
    extensions: &[String],
    format: OutputFormat,
) -> Exit {
    // A project further up doesn't block init: the new one is separate, and
    // commands run inside it resolve to it (the nearest wins).
    let request = init::Request {
        dir: path,
        name,
        version,
        extensions,
    };
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    let plan = match init::plan(&request, &runtime) {
        Ok(plan) => plan,
        Err(error) => return Refusal::of(format).report(&error),
    };
    if let Some(enclosing) = path.parent().and_then(find_project_root)
        && format != OutputFormat::Json
    {
        eprintln!(
            "note: {} is inside the project at {}; the new project is separate",
            path.display(),
            enclosing.display()
        );
    }
    let outcome = match init::apply(path, plan) {
        Ok(outcome) => outcome,
        Err(error) => return Refusal::of(format).report(&error),
    };

    // Every file init wrote: .gitignore when it lacked an entry, the
    // config, the starter, and each installed module and lock.
    let written = outcome.writes.names_under(path);
    match format {
        OutputFormat::Json => {
            print_json(&Written::of(outcome.document(), written));
        }
        OutputFormat::Human => {
            println!(
                "Initialized project '{}' at {}",
                outcome.name,
                path.display()
            );
            for file in &written {
                println!("  {file}");
            }
            if outcome.extensions.is_empty() {
                println!("\nNo extensions installed. Add one with: specforge add <extension>");
            }
            println!("\nNext steps:");
            println!("  specforge check    # validate your spec files");
            println!("  specforge export   # export the graph");
            if outcome
                .extensions
                .iter()
                .any(|e| e == "@specforge/cargo-test" || e == "@specforge/vitest")
            {
                println!("  specforge collect  # run the tests and record what they prove");
            }
        }
    }
    Exit::Passed
}
