//! Where one collector's evidence comes from, by collect mode (ADR 0125 D7): what the mode checks before
//! anything runs, what it gathers, and how it says it found none.

use super::Collector;
use crate::Held;
use crate::asking::{Asking, Question};
use crate::machine::{CollectorCommand, CollectorMachine, Ran, RunFailure, RunnerOutput};
use std::path::Path;

/// Whether `collector`'s command `argv` may run in `root`, with the approvals `machine` keeps: `--yes`
/// first, then a command approved for this project before, then whoever can be asked (ADR 0002,
/// ADR 0110). A yes from the asker is remembered. An approval that could not be remembered is
/// noted in `unsaved`.
pub(super) fn allowed(
    asking: Asking<'_>,
    machine: &dyn CollectorMachine,
    collector: &Collector,
    argv: &[String],
    root: &Path,
    unsaved: &mut Vec<String>,
) -> bool {
    match asking {
        Asking::Yes => true,
        _ if machine.approved(&collector.approval(root)) => true,
        Asking::Nobody => false,
        Asking::Ask(asker) => {
            let question = Question::RunCommand {
                extension: &collector.extension,
                argv,
                root,
            };
            if !asker.answer(&question) {
                return false;
            }
            if let Err(e) = machine.approve(&collector.approval(root)) {
                unsaved.push(e);
            }
            true
        }
    }
}

/// Run the collector's declared command in the project root through `machine`. A report *file* left by
/// an earlier run is removed first; a report *directory* is left alone (other tools may keep files
/// there) and filtered by modification time when it's read. A captured standard output still reaches
/// the chosen output as it's produced, and is also written to `capture`. Every file it removes,
/// creates or writes is a held one.
pub(super) fn run_command(
    machine: &dyn CollectorMachine,
    root: &Path,
    report: &Held,
    capture: Option<&Held>,
    argv: &[String],
    output: RunnerOutput,
) -> Result<Ran, String> {
    if report.real.is_file() {
        report
            .remove()
            .map_err(|e| format!("{}: {e}", report.spelled.display()))?;
    }
    for file in std::iter::once(report).chain(capture) {
        file.create_parent()
            .map_err(|e| format!("{}: {e}", file.spelled.display()))?;
    }
    let mut kept = match capture {
        Some(held) => Some(
            held.create_fresh()
                .map_err(|e| format!("{}: {e}", held.spelled.display()))?,
        ),
        None => None,
    };
    let command = CollectorCommand {
        argv,
        dir: root,
        report: &report.spelled,
        output,
    };
    machine
        .run(
            &command,
            kept.as_mut().map(|f| f as &mut dyn std::io::Write),
        )
        .map_err(|failure| match (failure, capture) {
            (RunFailure::Capture(e), Some(held)) => format!("{}: {e}", held.spelled.display()),
            (RunFailure::Capture(e) | RunFailure::NotRun(e), _) => {
                format!("failed to run `{}`: {e}", argv.join(" "))
            }
        })
}
