//! What collect asks the machine it runs on (ADR 0125): run a collector's command, and the user's approvals of
//! commands. Two adapters: [`ProcessMachine`](super::ProcessMachine) (a child process and the user's consent
//! store) and `specforge_ops::testing::ScriptedMachine` (tests).

use super::collector::Collector;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;

/// Environment variable the machine sets to the report path when it runs a collector's command, so a
/// runner integration can write there directly.
pub const REPORT_ENV: &str = "SPECFORGE_REPORT";

/// Where the runner's own output goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerOutput {
    /// The user's terminal.
    Inherit,
    /// Stderr, keeping stdout clean for machine-readable output.
    Stderr,
    /// Discarded (the MCP server owns stdio).
    Discard,
}

/// One collector command to run to its end.
#[derive(Debug, Clone, Copy)]
pub struct CollectorCommand<'a> {
    /// The command line, `{report}` expanded. Never empty: collect refuses an empty command first.
    pub argv: &'a [String],
    /// The directory it runs in: the project root.
    pub dir: &'a Path,
    /// What `SPECFORGE_REPORT` is set to: the report location as the project names it.
    pub report: &'a Path,
    /// Where its standard output and error go.
    pub output: RunnerOutput,
}

/// What running a command produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ran {
    /// Its exit code; `None` when a signal ended it. Non-zero is normal when tests fail.
    pub exit_code: Option<i32>,
    /// No later than the modification time of any file the command wrote: collect reads a report
    /// directory's files modified at or after it, so an old report can never pass for a new run.
    pub started: SystemTime,
}

/// Why a command produced no [`Ran`].
#[derive(Debug)]
pub enum RunFailure {
    /// It could not be started, or not waited for.
    NotRun(std::io::Error),
    /// Its standard output could not be written to the capture.
    Capture(std::io::Error),
}

/// One approval: this project may run this extension's collector with this command. A changed command is a
/// different approval. Built only by [`of`](Self::of), so a key is always the canonical project path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandApproval {
    project: String,
    extension: String,
    collector: String,
    run: Vec<String>,
}

impl CommandApproval {
    /// The approval `collector`'s declared command needs in `root` (canonicalized).
    pub fn of(collector: &Collector, root: &Path) -> Self {
        let project = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        CommandApproval {
            project: project.display().to_string(),
            extension: collector.extension.clone(),
            collector: collector.name.clone(),
            run: collector.run.clone(),
        }
    }

    /// Same project, extension and collector: an approval replaces another for a changed command.
    pub fn replaces(&self, other: &Self) -> bool {
        self.project == other.project
            && self.extension == other.extension
            && self.collector == other.collector
    }
}

/// The machine collect runs on.
pub trait CollectorMachine: Send + Sync {
    /// Run `command` to its end. Its standard output goes where `command.output` says and, when `capture`
    /// is given, is also written to it as it arrives. A non-zero exit is `Ok`.
    fn run(
        &self,
        command: &CollectorCommand<'_>,
        capture: Option<&mut dyn Write>,
    ) -> Result<Ran, RunFailure>;

    /// Whether the user approved `approval` before. A store that is missing or unreadable approves nothing.
    fn approved(&self, approval: &CommandApproval) -> bool;

    /// Remember the user's approval, replacing one for the same project, extension and collector.
    /// `Err`: why it could not be remembered (the collect still runs the command;
    /// `Outcome::unsaved_approvals`).
    fn approve(&self, approval: &CommandApproval) -> Result<(), String>;
}
