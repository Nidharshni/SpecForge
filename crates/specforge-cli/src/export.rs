use crate::options;
use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::terminal::{Terminal, errln, outln};
use specforge_ops::export;
use specforge_ops::schema::SchemaRequest;
use specforge_ops::view::ProjectView;
use std::path::PathBuf;

// `specforge export`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Path to the spec root directory
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Output format
    #[arg(
        long,
        value_parser = options::choice(&specforge_ops::export::FORMAT),
        default_value = specforge_ops::export::FORMAT.default_name()
    )]
    format: export::Format,

    /// Scope export to subgraph reachable from this entity ID
    #[arg(long)]
    scope: Option<String>,

    /// With --scope, how many hops from the scoped entity to include
    #[arg(long, requires = "scope")]
    depth: Option<usize>,

    /// Keep only entities of these kinds (comma-separated); the scoped
    /// entity always stays
    #[arg(long, value_delimiter = ',')]
    kinds: Vec<String>,

    /// Suppress schema embedding in `graph` exports (keeps format_version 1.0)
    #[arg(long)]
    no_schema: bool,

    /// Embed the schema in `context` and `brief` exports, and in a
    /// `graph` export under --max-tokens. They leave it out by default:
    /// an agent reading the export needs the entities, and the schema is
    /// most of the bytes. Under --max-tokens an embedded schema counts
    /// toward the budget, and a budget it doesn't fit in fails (E062).
    #[arg(long, conflicts_with = "no_schema")]
    with_schema: bool,

    /// Request a specific schema version for the export
    #[arg(long)]
    schema_version: Option<String>,

    /// Token budget for the export: keeps the most central entities that
    /// fit. The schema is left out unless --with-schema is given, and
    /// then counts toward the budget. The export lists the dropped
    /// entities under `token_budget`; below one entity it is the envelope
    /// with no entities, and below even that it fails (E062). Ignored by
    /// `dot`.
    #[arg(long)]
    max_tokens: Option<usize>,
}

impl Args {
    /// The export the flags ask for.
    fn request(&self) -> export::Request<'_> {
        export::Request {
            format: Some(self.format),
            scope: self.scope.as_deref(),
            depth: self.depth,
            kinds: self.kinds.iter().map(String::as_str).collect(),
            max_tokens: self.max_tokens,
            // context, brief and a budgeted graph export leave the schema
            // out unless asked for it (the policy lives in specforge-ops).
            schema: match (self.no_schema, self.with_schema) {
                (true, _) => export::Schema::Without,
                (_, true) => export::Schema::With,
                _ => export::Schema::Default,
            },
            schema_version: self.schema_version.as_deref(),
        }
    }

    /// Export the project compiled at `path` to stdout. Before it writes,
    /// the schema the extensions produce is compared with the one the
    /// previous export cached in `<path>/.specforge/schema-cache.json` (the
    /// root it compiled, never an ancestor's): each breaking change is a
    /// W053 warning on stderr, and the cached schema's version, bumped by
    /// what changed, is the version the export carries. After a successful
    /// export that schema replaces the cache, so the next export compares
    /// against it. The export is written whatever the comparison finds.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let project = match crate::project::compile(&self.path) {
            Ok(project) => project,
            Err(error) => return Refusal::of(OutputFormat::Human).report(terminal, &error),
        };
        let view = ProjectView::of(&project);

        // The export goes to stdout: there is no output directory holding
        // earlier exports, and `.specforge/` holds extensions and the watch
        // snapshot too, so nothing here shows the project was exported
        // before.
        let recorded = export::export_recorded(&view, &self.request());
        for diagnostic in &recorded.breaking {
            errln!(terminal, "{}", specforge_common::render_plain(diagnostic));
        }
        let output = match recorded.export {
            Ok(output) => output,
            Err(error) => return Refusal::of(OutputFormat::Human).report(terminal, &error),
        };
        outln!(terminal, "{}", output);

        if let export::CacheWrite::WriteFailed { dir, error } = &recorded.cache {
            errln!(
                terminal,
                "warning: could not write the schema cache in {}: {error}",
                dir.display()
            );
        }
        Exit::Passed
    }
}

// `specforge schema`'s command line.
#[derive(clap::Args)]
pub(crate) struct SchemaArgs {
    /// Path to the spec root directory
    #[arg(default_value = ".")]
    path: PathBuf,

    /// That entity kind and the edge types that can start or end at it
    #[arg(long, conflicts_with = "publish")]
    kind: Option<String>,

    /// Leave the edge types out
    #[arg(long, conflicts_with = "publish")]
    no_edges: bool,

    /// Add the validation rules the loaded extensions declare
    #[arg(long, conflicts_with = "publish")]
    validation_rules: bool,

    /// Publish as standalone JSON Schema (draft 2020-12)
    #[arg(long)]
    publish: bool,

    /// Export format the published schema describes
    #[arg(
        long,
        requires = "publish",
        value_parser = options::choice(&specforge_ops::export::AGENT_FORMAT),
        default_value = specforge_ops::export::AGENT_FORMAT.default_name()
    )]
    format: export::Format,
}

impl SchemaArgs {
    /// The schema the flags ask for.
    fn request(&self) -> SchemaRequest<'_> {
        SchemaRequest {
            kind: self.kind.as_deref(),
            edges: !self.no_edges,
            validation_rules: self.validation_rules,
        }
    }

    /// `specforge schema`: the schema operation over the project compiled
    /// at `path`, versioned as the next export would be (the cache is only
    /// read): the document `specforge.schema` returns for the same request,
    /// pretty printed. With `--publish`, the JSON Schema an export of that
    /// format conforms to. An unknown kind is refused with the closest one
    /// (exit 1).
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let project = match crate::project::compile(&self.path) {
            Ok(project) => project,
            Err(error) => return Refusal::of(OutputFormat::Human).report(terminal, &error),
        };
        let view = ProjectView::of(&project);
        let output = if self.publish {
            specforge_ops::schema::json_schema(&view, self.format)
        } else {
            specforge_ops::schema::schema(&view, &self.request())
                .map(|outcome| serde_json::to_string_pretty(&outcome).expect("a schema serializes"))
        };
        match output {
            Ok(text) => {
                outln!(terminal, "{text}");
                Exit::Passed
            }
            Err(error) => Refusal::of(OutputFormat::Human).report(terminal, &error),
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

    fn schema_policy(flags: &[&str]) -> Result<export::Schema, clap::Error> {
        let line = Line::try_parse_from(std::iter::once("export").chain(flags.iter().copied()))?;
        Ok(line.args.request().schema)
    }

    #[test]
    fn the_schema_flags_choose_the_exports_schema_policy() {
        assert!(matches!(
            schema_policy(&["--no-schema"]),
            Ok(export::Schema::Without)
        ));
        assert!(matches!(
            schema_policy(&["--with-schema"]),
            Ok(export::Schema::With)
        ));
        assert!(matches!(schema_policy(&[]), Ok(export::Schema::Default)));
        let both = schema_policy(&["--no-schema", "--with-schema"]);
        assert!(
            matches!(&both, Err(e) if e.kind() == clap::error::ErrorKind::ArgumentConflict),
            "the two flags conflict"
        );
    }
}
