//! `specforge.format`: format the spec files (`specforge_ops::authoring::format`).

use crate::target::{AnyProject, Project};
use crate::tool::{CoreTool, Effect, ReplySpec, WriteHints};
use std::path::{Path, PathBuf};

use crate::containment;
use specforge_ops::Containment;

use crate::args::Arguments;
use crate::mutation::{
    Failed, Mutated, Mutation, MutationArgs, MutationReply, NoDiagnostics, Written,
};
use crate::target::ProjectRef;
use crate::tool::{ErrorCode, McpError};
use specforge_ops::OpErrorKind;
use specforge_ops::authoring::format::{self, Request};

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

        // The run `specforge format` makes. Relative paths name files under
        // the project root, and none leaves it (ADR 0092).
        let bound = Containment::within(project_root.dir());
        let explicit = args
            .paths
            .iter()
            .map(|p| {
                let named = Path::new(p);
                bound
                    .resolve(named)
                    .map(|held| held.spelled)
                    .map_err(|outside| Box::new(containment::refusal(&outside, "paths")))
            })
            .collect::<Result<Vec<PathBuf>, Box<McpError>>>()?;
        // A source that is a symlink leaving the project is not followed.
        let outcome = format::run_within(
            &Request {
                root: project_root,
                paths: &explicit,
                mode,
            },
            Some(project_root.dir()),
        )?;

        let reply = outcome.document(args.diff);
        if outcome.succeeded() {
            return Ok(Mutated::new(reply, Written::files(outcome.writes())));
        }

        // Every other file was still formatted, and what was written is
        // reported; the call failed for these (read or write), with the kind
        // the files share (permission denied for locked files, not found for
        // missing ones), else an internal failure. The failed call's `data`
        // is the reply.
        let code = ErrorCode::from(outcome.failure_kind().unwrap_or(OpErrorKind::Internal));
        let error = McpError::new(code, reply.message.clone().unwrap_or_default())
            .with_data(serde_json::to_value(&reply).expect("a reply serializes"));
        Err(Failed::after(error, outcome.writes()))
    }
}
