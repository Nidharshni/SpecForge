use crate::options;
use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::pipeline;
use crate::terminal::{Terminal, errln, out};
use specforge_ops::model::{FieldLevel, GroupBy, ModelFormat, ModelOptions, ModelRoot};
use specforge_ops::view::ProjectView;
use std::path::PathBuf;

// `specforge model`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Path to the spec root directory
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Output format
    #[arg(
        long,
        value_parser = options::choice(&specforge_ops::model::MODEL_FORMAT),
        default_value = specforge_ops::model::MODEL_FORMAT.default_name()
    )]
    format: ModelFormat,

    /// How to group entities
    #[arg(
        long,
        value_parser = options::choice(&specforge_ops::model::GROUP_BY),
        default_value = specforge_ops::model::GROUP_BY.default_name()
    )]
    group_by: GroupBy,

    /// Field detail level
    #[arg(
        long,
        value_parser = options::choice(&specforge_ops::model::MODEL_FIELDS),
        default_value = specforge_ops::model::MODEL_FIELDS.default_name()
    )]
    fields: FieldLevel,

    /// Filter to a single extension
    #[arg(long)]
    extension: Option<String>,

    /// Filter to specific entity kinds (comma-separated)
    #[arg(long, value_delimiter = ',')]
    kinds: Vec<String>,

    /// Root entity kind for depth-scoped output
    #[arg(long)]
    root: Option<String>,

    /// Maximum depth from root kind (requires --root)
    #[arg(long, requires = "root")]
    depth: Option<usize>,
}

impl Args {
    /// The model the flags ask for.
    fn options(&self) -> ModelOptions {
        ModelOptions {
            format: self.format,
            group_by: self.group_by,
            fields: self.fields,
            extension: self.extension.clone(),
            kinds: self.kinds.clone(),
            root: self.root.clone().map(|kind| ModelRoot {
                kind,
                depth: self.depth,
            }),
        }
    }

    /// `specforge model`: the model operation over the project compiled at
    /// `path`. The model on stdout, the notices (I020) on stderr, a refusal
    /// as `error[CODE]` with its hint (exit 1).
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let project = pipeline::compile_project(&self.path);
        match specforge_ops::model::model(&ProjectView::of(&project), &self.options()) {
            Ok(outcome) => {
                for notice in &outcome.notices {
                    errln!(terminal, "{}", specforge_common::render_plain(notice));
                }
                out!(terminal, "{}", outcome.document);
                Exit::Passed
            }
            Err(error) => Refusal::of(OutputFormat::Human).report(terminal, &error),
        }
    }
}
