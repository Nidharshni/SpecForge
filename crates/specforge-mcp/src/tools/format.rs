//! `specforge.format`: format the spec files (`specforge_ops::format`).

use std::path::PathBuf;

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{DiagnosticList, project_root_of};

use crate::args::Arguments;
use crate::mutation::{
    Failed, Mutated, Mutation, MutationArgs, MutationReply, NoDiagnostics, Written,
};
use crate::target::ProjectRef;
use crate::tool::{ErrorCode, McpError};
use specforge_ops::OpErrorKind;
use specforge_ops::format::{self, Outcome, Request};

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

/// `specforge.format`'s reply (`McpFormatResult`).
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    changed_files: Vec<String>,
    total_checked: usize,
    ok: bool,
    all_clean: bool,
    check_only: bool,
    diagnostics: DiagnosticList,
    /// Each file that would change, before and after (with `diff`).
    #[serde(skip_serializing_if = "Option::is_none")]
    diffs: Option<Vec<Diff>>,
    /// Why the call failed (a failed call's `data` holds the reply).
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed_files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failures: Option<Vec<FileFailure>>,
}

/// One file's before and after text.
#[derive(Debug, Serialize, Shape)]
pub struct Diff {
    file_path: String,
    before: String,
    after: String,
    insertions: usize,
    deletions: usize,
}

/// One file that could not be read or written.
#[derive(Debug, Serialize, Shape)]
pub struct FileFailure {
    file: String,
    operation: String,
    /// The `McpErrorCode` of the file's failure.
    code: String,
    message: String,
}

fn shown(path: &std::path::Path) -> String {
    path.display().to_string()
}

impl Reply {
    fn of(outcome: &Outcome, mode: format::Mode, with_diffs: bool) -> Self {
        Reply {
            changed_files: outcome.changes.iter().map(|c| shown(&c.path)).collect(),
            total_checked: outcome.checked,
            ok: outcome.ok(),
            all_clean: outcome.clean(),
            check_only: !mode.writes(),
            diagnostics: DiagnosticList(outcome.diagnostics.clone()),
            diffs: with_diffs.then(|| {
                outcome
                    .changes
                    .iter()
                    .map(|c| {
                        let file_path = shown(&c.path);
                        let stats =
                            specforge_formatter::unified_diff(&file_path, &c.before, &c.after);
                        Diff {
                            file_path,
                            before: c.before.clone(),
                            after: c.after.clone(),
                            insertions: stats.insertions,
                            deletions: stats.deletions,
                        }
                    })
                    .collect()
            }),
            message: None,
            failed_files: None,
            failures: None,
        }
    }

    /// The reply of a call that failed for these files (read or write).
    fn fail_with(&mut self, outcome: &Outcome) {
        let reasons: Vec<String> = outcome.failures.iter().map(ToString::to_string).collect();
        self.message = Some(reasons.join("; "));
        self.failed_files = Some(outcome.failures.iter().map(|f| shown(f.path())).collect());
        self.failures = Some(
            outcome
                .failures
                .iter()
                .map(|f| FileFailure {
                    file: shown(f.path()),
                    operation: f.verb().to_string(),
                    code: ErrorCode::from(f.kind()).as_str().to_string(),
                    message: f.to_string(),
                })
                .collect(),
        );
    }
}

pub(crate) fn call(project: &ProjectRef<'_>, args: Args) -> Mutation<Reply> {
    // The one reading of check, diff and write.
    let mode = args.mode();

    // The project the call formats: the served one, or the one `path`
    // names (a directory that is no project is its own root, formatted with
    // the defaults, as `specforge format` formats it); its config decides
    // what is formatted.
    let project_root = project_root_of(project.root);

    // The run `specforge format` makes. Relative paths name files under
    // the project root.
    let explicit: Vec<PathBuf> = args.paths.iter().map(|p| project_root.join(p)).collect();
    let outcome = format::run(&Request {
        root: &project_root,
        paths: &explicit,
        mode,
    });

    let mut reply = Reply::of(&outcome, mode, args.diff);
    if outcome.succeeded() {
        return Ok(Mutated::new(reply, Written::files(outcome.writes())));
    }

    // Every other file was still formatted, and what was written is
    // reported; the call failed for these (read or write), with the kind
    // the files share (permission denied for locked files, not found for
    // missing ones), else an internal failure. The failed call's `data`
    // is the reply.
    reply.fail_with(&outcome);
    let code = ErrorCode::from(outcome.failure_kind().unwrap_or(OpErrorKind::Internal));
    let error = McpError::new(code, reply.message.clone().unwrap_or_default())
        .with_data(serde_json::to_value(&reply).expect("a reply serializes"));
    Err(Failed::after(error, outcome.writes()))
}
