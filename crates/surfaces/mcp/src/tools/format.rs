//! `specforge.format`: format the spec files (`specforge_ops::authoring::format`).

use crate::target::{AnyProject, Project};
use crate::tool::{CoreTool, Effect, ReplySpec, WriteHints};
use std::path::PathBuf;

use crate::refusal::ArgumentOf;

use crate::args::Arguments;
use crate::mutation::{Mutated, Mutation, MutationArgs, MutationReply, NoDiagnostics, Written};
use crate::target::ProjectRef;
use specforge_ops::authoring::format::{self, FormatField, Request};

/// `specforge.format`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Files or directories to format, relative to the project root (defaults to every spec file)
    paths: Vec<String>,
    /// Check only, don't modify
    check: bool,
    /// Return a before/after diff for each file that would change, without modifying it
    diff: bool,
    /// Write formatted output (defaults to false in check or diff mode)
    write: Option<bool>,
}

impl MutationArgs for Args {
    fn previews(&self) -> bool {
        !self.mode().writes()
    }
}

impl MutationReply for Reply {
    type After = NoDiagnostics;
}

impl Args {
    /// Whether the call writes or only reports: `specforge format`'s one
    /// reading of check, diff and write.
    pub(crate) fn mode(&self) -> format::Mode {
        format::Mode::of_flags(self.check, self.diff, self.write)
    }
}

/// `specforge.format`'s reply: the format document
/// `specforge format --format json` prints (a failed call carries it in its
/// error's `data`; ADR 0052).
type Reply = format::FormatDocument;

pub(crate) struct Tool;

impl ArgumentOf<FormatField> for Tool {
    fn argument(field: FormatField) -> Option<&'static str> {
        match field {
            FormatField::Paths => Some("paths"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.format";
    const DESCRIPTION: &'static str = "Format spec files";
    const EFFECT: Effect = Effect::Mutates {
        hints: WriteHints {
            destructive: true,
            idempotent: true,
            open_world: false,
        },
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpFormatResult");
    type Args = Args;
    type Input = Project<AnyProject>;
    type Returns = Mutation<Reply>;

    fn call(project: ProjectRef<'_>, args: Args) -> Mutation<Reply> {
        // The one reading of check, diff and write.
        let mode = args.mode();

        // The project the call formats: the served one, or the one `path`
        // names (a directory that is no project is its own root, formatted with
        // the defaults, as `specforge format` formats it); its config decides
        // what is formatted.
        let project_root = project.root;

        // The run `specforge format` makes. Relative paths name files under the project root, and
        // none leaves it: the operation holds each to the containment (ADR 0092, ADR 0104), and a
        // source that is a symlink leaving the project is not followed.
        let explicit: Vec<PathBuf> = args.paths.iter().map(PathBuf::from).collect();
        let outcome = format::run(&Request {
            root: project_root,
            paths: &explicit,
            mode,
            caller: project.caller(),
            // The served project's forms are lent: no load per call; a nested project's load through the
            // state's source, never into the served runtime.
            forms: format::Forms::lending(project.view().env(), project.source.clone()),
        })
        .map_err(Self::failed)?;

        // A target it could not read or write fails the call, the others still formatted: its
        // error is the run's, the document its data and what it wrote reported (ADR 0170).
        if let Some(failed) = outcome.failed(args.diff) {
            return Err(failed.into());
        }
        Ok(Mutated::new(
            outcome.document(args.diff),
            Written::of(outcome.wrote()),
        ))
    }
}
