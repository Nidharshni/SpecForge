use crate::terminal::{Terminal, outln};
use std::path::Path;

use specforge_ops::infer::{InferenceGuide, KindGuide, guide, kind_guide};
use specforge_ops::view::ProjectView;

use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::pipeline;

/// `specforge infer-guide [KIND]`: the inference guide read view over the
/// project compiled at `path` (ADR 0015, "Prompt read views"): every
/// declared kind's guide, or one kind's. The JSON is the infer prompt's
/// overview or kind-scope data without the prompt's own `output_format` and
/// `validation` text. An undeclared kind is `unknown_kind` naming the
/// closest declared kind (exit 1).
pub(crate) fn run(
    path: &Path,
    kind: Option<&str>,
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    let project = pipeline::compile_project(path);
    let view = ProjectView::of(&project);
    match kind {
        None => {
            let guide = guide(&view);
            match format {
                OutputFormat::Json => terminal.json(&guide.document()),
                OutputFormat::Human => print_overview(&guide, terminal),
            }
            Exit::Passed
        }
        Some(kind) => match kind_guide(&view, kind) {
            Ok(guide) => {
                match format {
                    OutputFormat::Json => terminal.json(&guide.document()),
                    OutputFormat::Human => print_kind(&guide, terminal),
                }
                Exit::Passed
            }
            Err(error) => Refusal::of(format).report(terminal, &error),
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

fn print_overview(guide: &InferenceGuide, terminal: &Terminal) {
    for kind in &guide.kinds {
        match kind.description {
            Some(description) => outln!(
                terminal,
                "{} ({}): {description}",
                kind.keyword,
                kind.extension
            ),
            None => outln!(terminal, "{} ({})", kind.keyword, kind.extension),
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
        outln!(terminal, "  fields: {}", fields.join(", "));
        if !kind.guide.is_empty() {
            outln!(terminal, "{}", indented(&kind.guide, "  "));
        }
    }
    outln!(terminal);
    outln!(terminal, "Spec directory: {}", guide.spec_directory);
    if let Some(conventions) = guide.conventions {
        outln!(terminal, "Conventions: {conventions}");
    }
}

fn print_kind(guide: &KindGuide, terminal: &Terminal) {
    match guide.description {
        Some(description) => outln!(
            terminal,
            "{} ({}): {description}",
            guide.keyword,
            guide.extension
        ),
        None => outln!(terminal, "{} ({})", guide.keyword, guide.extension),
    }
    outln!(terminal);
    outln!(terminal, "Fields:");
    for field in &guide.fields {
        let declared = field.declared();
        outln!(
            terminal,
            "  {}  {}  {}  {}",
            field.name(),
            field.type_label(),
            if declared.required { "*" } else { "-" },
            declared.description.as_deref().unwrap_or("")
        );
    }
    if guide.existing.is_empty() {
        outln!(terminal, "Existing: none");
    } else {
        outln!(terminal, "Existing: {}", guide.existing.join(", "));
    }
    outln!(terminal, "Guide:");
    outln!(terminal, "{}", indented(&guide.guide, "  "));
    outln!(terminal, "Example:");
    outln!(terminal, "{}", indented(&guide.example(), "  "));
}
