use crate::options;
use crate::outcome::{Exit, Refusal};
use crate::terminal::{Terminal, out};
use specforge_ops::read::model::{DependencyDepth, OutlineDetail, OutlineFormat, OutlineOptions};
use specforge_ops::view::ProjectView;

// `specforge outline`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: crate::project::ProjectArg,

    /// Output format
    #[arg(
        long,
        value_parser = options::choice(&specforge_ops::read::model::OUTLINE_FORMAT),
        default_value = specforge_ops::read::model::OUTLINE_FORMAT.default_name()
    )]
    format: OutlineFormat,

    /// Detail level
    #[arg(
        long,
        value_parser = options::choice(&specforge_ops::read::model::OUTLINE_FIELDS),
        default_value = specforge_ops::read::model::OUTLINE_FIELDS.default_name()
    )]
    fields: OutlineDetail,

    /// Dependency visibility
    #[arg(
        long,
        value_parser = options::choice(&specforge_ops::read::model::DEPS),
        default_value = specforge_ops::read::model::DEPS.default_name()
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
        let project = match self.project.compile() {
            Ok(project) => project,
            Err(error) => return Refusal::fixed().report(terminal, &error),
        };
        out!(
            terminal,
            "{}",
            specforge_ops::read::model::outline(&ProjectView::of(&project), &self.options())
        );
        Exit::Passed
    }
}
