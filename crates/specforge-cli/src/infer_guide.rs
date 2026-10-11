use std::path::Path;

use specforge_ops::infer::{InferenceGuide, KindGuide, guide, kind_guide};
use specforge_ops::view::ProjectView;

use crate::OutputFormat;
use crate::outcome::{Exit, Refusal, print_json};
use crate::pipeline;

/// `specforge infer-guide [KIND]`: the inference guide read view over the
/// project compiled at `path` (ADR 0015, "Prompt read views"): every
/// declared kind's guide, or one kind's. The JSON is the infer prompt's
/// overview or kind-scope data without the prompt's own `output_format` and
/// `validation` text. An undeclared kind is `unknown_kind` naming the
/// closest declared kind (exit 1).
pub(crate) fn run(path: &Path, kind: Option<&str>, format: OutputFormat) -> Exit {
    let project = pipeline::compile_project(path);
    let view = ProjectView::of(&project);
    match kind {
        None => {
            let guide = guide(&view);
            match format {
                OutputFormat::Json => print_json(&guide.document()),
                OutputFormat::Human => print_overview(&guide),
            }
            Exit::Passed
        }
        Some(kind) => match kind_guide(&view, kind) {
            Ok(guide) => {
                match format {
                    OutputFormat::Json => print_json(&guide.document()),
                    OutputFormat::Human => print_kind(&guide),
                }
                Exit::Passed
            }
            Err(error) => Refusal::of(format).report(&error),
        },
    }
}

/// `text`, each non-empty line indented by `indent`.
fn indented(text: &str, indent: &str) -> String {
    text.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn print_overview(guide: &InferenceGuide) {
    for kind in &guide.kinds {
        match kind.description {
            Some(description) => println!("{} ({}): {description}", kind.keyword, kind.extension),
            None => println!("{} ({})", kind.keyword, kind.extension),
        }
        let fields: Vec<String> = kind
            .fields
            .iter()
            .map(|field| {
                if field.declared().required {
                    format!("{}*", field.name())
                } else {
                    field.name().to_string()
                }
            })
            .collect();
        println!("  fields: {}", fields.join(", "));
        if !kind.guide.is_empty() {
            println!("{}", indented(&kind.guide, "  "));
        }
    }
    println!();
    println!("Spec directory: {}", guide.spec_directory);
    if let Some(conventions) = guide.conventions {
        println!("Conventions: {conventions}");
    }
}

fn print_kind(guide: &KindGuide) {
    match guide.description {
        Some(description) => println!("{} ({}): {description}", guide.keyword, guide.extension),
        None => println!("{} ({})", guide.keyword, guide.extension),
    }
    println!();
    println!("Fields:");
    for field in &guide.fields {
        let declared = field.declared();
        println!(
            "  {}  {}  {}  {}",
            field.name(),
            field.type_label(),
            if declared.required { "*" } else { "-" },
            declared.description.as_deref().unwrap_or("")
        );
    }
    if guide.existing.is_empty() {
        println!("Existing: none");
    } else {
        println!("Existing: {}", guide.existing.join(", "));
    }
    println!("Guide:");
    println!("{}", indented(&guide.guide, "  "));
    println!("Example:");
    println!("{}", indented(&guide.example(), "  "));
}
