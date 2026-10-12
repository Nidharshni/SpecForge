use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, outln};
use specforge_ops::infer::{InferenceGuide, KindGuide, guide, kind_guide};
use specforge_ops::view::ProjectView;
use std::path::PathBuf;

// `specforge infer-guide`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Entity kind (omit for every declared kind)
    kind: Option<String>,

    #[arg(long, default_value = ".", help = crate::project::PATH_HELP)]
    path: PathBuf,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    /// `specforge infer-guide [KIND]`: the inference guide read view over
    /// the project compiled at `path` (ADR 0015, "Prompt read views"):
    /// every declared kind's guide, or one kind's. The JSON is the infer
    /// prompt's overview or kind-scope data without the prompt's own
    /// `output_format` and `validation` text. An undeclared kind is
    /// `unknown_kind` naming the closest declared kind (exit 1).
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let project = match crate::project::compile(&self.path) {
            Ok(project) => project,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let view = ProjectView::of(&project);
        match self.kind.as_deref() {
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
