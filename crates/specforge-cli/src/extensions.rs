use crate::OutputFormat;
use crate::outcome::Exit;
use serde_json::json;
use specforge_ops::extension;
use specforge_ops::view::ProjectView;
use std::path::Path;

/// `specforge extensions`: every extension the project enables, has
/// installed or loaded, alphabetically, with the entity kinds each
/// registered and how many of the project's entities use them.
pub fn run(path: &Path, format: OutputFormat) -> Exit {
    let project = crate::pipeline::compile_project(path);
    let entries = extension::list(&ProjectView::of(&project)).extensions;

    match format {
        OutputFormat::Json => {
            let output = json!({
                "extensions": entries,
                "count": entries.len(),
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&output).expect("serialize JSON output")
            );
        }
        OutputFormat::Human => {
            if entries.is_empty() {
                println!("No extensions installed.");
                println!();
                println!("Install one with: specforge add <extension>");
                return Exit::Passed;
            }
            println!("Installed extensions:");
            println!();
            for entry in &entries {
                let version = entry
                    .version
                    .as_deref()
                    .map(|v| format!(" v{v}"))
                    .unwrap_or_default();
                let provides = if entry.entity_kinds.is_empty() {
                    String::new()
                } else {
                    format!(
                        ": {} entities ({})",
                        entry.entity_count,
                        entry.entity_kinds.join(", ")
                    )
                };
                let status = match entry.status {
                    extension::Status::Loaded => String::new(),
                    other => format!(" [{}]", other.as_str()),
                };
                println!(
                    "  {}{version} ({}){status}{provides}",
                    entry.name, entry.source
                );
            }
            println!();
            println!("{} extension(s) installed.", entries.len());
        }
    }

    Exit::Passed
}
