use crate::options;
use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::terminal::{Terminal, out};
use specforge_ops::model::{DependencyDepth, OutlineDetail, OutlineFormat, OutlineOptions};
use specforge_ops::view::ProjectView;
use std::path::PathBuf;

// `specforge outline`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Path to the spec root directory
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Output format
    #[arg(
        long,
        value_parser = options::choice(&specforge_ops::model::OUTLINE_FORMAT),
        default_value = specforge_ops::model::OUTLINE_FORMAT.default_name()
    )]
    format: OutlineFormat,

    /// Detail level
    #[arg(
        long,
        value_parser = options::choice(&specforge_ops::model::OUTLINE_FIELDS),
        default_value = specforge_ops::model::OUTLINE_FIELDS.default_name()
    )]
    fields: OutlineDetail,

    /// Dependency visibility
    #[arg(
        long,
        value_parser = options::choice(&specforge_ops::model::DEPS),
        default_value = specforge_ops::model::DEPS.default_name()
    )]
    deps: DependencyDepth,
}

impl Args {
    /// The outline the flags ask for.
    fn options(&self) -> OutlineOptions {
        OutlineOptions {
            format: self.format,
            detail: self.fields,
            deps: self.deps,
        }
    }

    /// `specforge outline`: the outline operation over the project compiled
    /// at `path`.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let project = match crate::project::compile(&self.path) {
            Ok(project) => project,
            Err(error) => return Refusal::of(OutputFormat::Human).report(terminal, &error),
        };
        out!(
            terminal,
            "{}",
            specforge_ops::model::outline(&ProjectView::of(&project), &self.options())
        );
        Exit::Passed
    }
}
