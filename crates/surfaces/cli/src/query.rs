use crate::notes::Notes;
use crate::options;
use crate::outcome::{Exit, Refusal};
use crate::terminal::{Terminal, outln};
use specforge_ops::read::export::Format;
use specforge_ops::read::query::{QueryRequest, query};
use specforge_ops::view::ProjectView;

// `specforge query`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Entity ID to query
    entity: String,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    /// Number of hops from the entity (0 = entity only)
    #[arg(long, default_value_t = specforge_ops::read::query::DEFAULT_DEPTH)]
    depth: usize,

    /// Keep only entities of these kinds (comma-separated, or the flag repeated); the queried
    /// entity always stays
    #[arg(long)]
    kinds: Vec<String>,

    /// Output detail level
    #[arg(
        long,
        value_parser = options::choice(&specforge_ops::read::export::AGENT_FORMAT),
        default_value = specforge_ops::read::export::AGENT_FORMAT.default_name()
    )]
    format: Format,

    /// Give every entity its coverage status
    #[arg(long)]
    include_coverage: bool,
}

impl Args {
    /// The query the flags ask for.
    fn request(&self) -> QueryRequest<'_> {
        QueryRequest {
            entity_id: &self.entity,
            depth: Some(self.depth),
            kinds: specforge_ops::view::kinds_of_texts(&self.kinds).collect(),
            format: Some(self.format),
            include_coverage: self.include_coverage,
        }
    }

    /// `specforge query <entity>`: the query read view over the project
    /// compiled at `path`. The document on stdout; the notices (I020) on
    /// stderr; a refusal as `error[CODE]` with the closest entity as its
    /// hint (exit 1).
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let project = match self.project.compile() {
            Ok(project) => project,
            Err(error) => return Refusal::fixed().report(terminal, &error),
        };
        match query(&ProjectView::of(&project), &self.request()) {
            Ok(document) => {
                let document = Notes::fixed(terminal).noticed(document);
                outln!(terminal, "{}", document);
                Exit::Passed
            }
            Err(error) => Refusal::fixed().report(terminal, &error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Line {
        #[command(flatten)]
        args: Args,
    }

    #[test]
    fn the_kinds_flag_is_a_kinds_list() {
        let line =
            Line::try_parse_from(["query", "x", "--kinds", "behavior, type", "--kinds", "adr"])
                .unwrap();
        assert_eq!(line.args.request().kinds, ["behavior", "type", "adr"]);
    }
}
