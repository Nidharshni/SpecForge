//! Where one collector's evidence comes from, by collect mode (ADR 0125 D7): what the mode checks before
//! anything runs, what it gathers, and how it says it found none. A run and a no-run read the collector's
//! declared report and captured output, so only they resolve and check them; `--report` reads the files it
//! names and nothing the collector declares.

use super::collector::{capture_path, captures_stdout, command_line, read_capture, report_path};
use super::dispatch::read_report;
use super::{Collector, Mode, fail};
use crate::asking::{Asking, Question};
use crate::machine::{CollectorCommand, CollectorMachine, Ran, RunFailure, RunnerOutput};
use crate::{Containment, Held, OpError, OpErrorKind};
use specforge_common::codes;
use specforge_protocol_types::CollectReportFile;
use std::path::{Path, PathBuf};

/// A collector's declared report location and captured output, held.
pub(super) struct Declared {
    report: Held,
    capture: Option<Held>,
}

impl Declared {
    /// `collector`'s declared report and capture, held by `containment`: E058 for a report name that is not
    /// relative-without-`..` or an unknown capture; `outside_root` for one that lands outside.
    fn of(collector: &Collector, root: &Path, containment: &Containment) -> Result<Self, OpError> {
        let report = report_path(collector, root, containment)?;
        let capturing = captures_stdout(collector)
            .map_err(|m| fail(OpErrorKind::PreconditionFailed, codes::E058, m))?;
        let capture = if capturing {
            Some(capture_path(collector, &report, containment)?)
        } else {
            None
        };
        Ok(Declared { report, capture })
    }

    /// The captured output, if the collector captures one and it is there.
    fn stdout(&self) -> Option<String> {
        self.capture.as_ref().and_then(read_capture)
    }
}

/// Where one collector's evidence comes from: decided for every selected collector before any command runs.
pub(super) enum Source<'m> {
    /// Run the approved `argv`, then read the declared report written since it started, and the capture.
    Run {
        output: RunnerOutput,
        declared: Declared,
        argv: Vec<String>,
    },
    /// Read the declared report and the capture the runner already wrote.
    Written(Declared),
    /// Read these files, and nothing the collector declares.
    Given(&'m [PathBuf]),
}

/// What one collector's source gave.
pub(super) struct Evidence {
    pub(super) files: Vec<CollectReportFile>,
    pub(super) stdout: Option<String>,
    pub(super) exit_code: Option<i32>,
    /// Whether the collector's command ran.
    pub(super) ran: bool,
}

impl<'m> Source<'m> {
    /// `collector`'s source under `mode`, decided without running anything or writing a file. Under a run
    /// that it has a command and that the command may run (asking whoever can be asked; E059 otherwise).
    pub(super) fn plan(
        collector: &Collector,
        mode: Mode<'m>,
        root: &Path,
        asking: Asking<'_>,
        containment: &Containment,
        machine: &dyn CollectorMachine,
        unsaved_approvals: &mut Vec<String>,
    ) -> Result<Self, OpError> {
        match mode {
            Mode::Reports(paths) => Ok(Source::Given(paths)),
            Mode::NoRun => Declared::of(collector, root, containment).map(Source::Written),
            Mode::Run(output) => {
                let declared = Declared::of(collector, root, containment)?;
                let argv = command_line(collector, &declared.report.spelled);
                if argv.is_empty() {
                    return Err(fail(
                        OpErrorKind::SchemaMismatch,
                        codes::E045,
                        format!(
                            "collector '{}' of {} declares no command, so it cannot run the tests; collect from the \
                             report they already wrote",
                            collector.name, collector.extension
                        ),
                    ));
                }
                if !allowed(asking, machine, collector, &argv, root, unsaved_approvals) {
                    return Err(fail(
                        OpErrorKind::PermissionDenied,
                        codes::E059,
                        format!(
                            "running `{}` needs approval for this project",
                            argv.join(" ")
                        ),
                    )
                    .with_suggestion(
                        "approve it once with `specforge collect` in a terminal, or run `specforge \
                         collect --yes`; reading the report the runner already wrote needs no approval",
                    ));
                }
                Ok(Source::Run {
                    output,
                    declared,
                    argv,
                })
            }
        }
    }

    /// Gather the evidence: under `Run`, announce then run (E045 when it cannot start or its capture cannot be
    /// written), then read. No report file and no captured output is E045, worded for the source.
    pub(super) fn gather(
        &self,
        collector: &Collector,
        root: &Path,
        containment: &Containment,
        machine: &dyn CollectorMachine,
        announce: &mut dyn FnMut(&Collector, &[String]),
    ) -> Result<Evidence, OpError> {
        let (files, stdout, exit_code, ran) = match self {
            Source::Run {
                output,
                declared,
                argv,
            } => {
                announce(collector, argv);
                let ran = run_command(
                    machine,
                    root,
                    &declared.report,
                    declared.capture.as_ref(),
                    argv,
                    *output,
                )
                .map_err(|m| fail(OpErrorKind::SchemaMismatch, codes::E045, m))?;
                let files = read_report(&declared.report, root, Some(ran.started), containment)?;
                (files, declared.stdout(), ran.exit_code, true)
            }
            Source::Written(declared) => {
                let files = read_report(&declared.report, root, None, containment)?;
                (files, declared.stdout(), None, false)
            }
            Source::Given(paths) => {
                let files = paths
                    .iter()
                    .map(|p| read_report(&containment.resolve(p)?, root, None, containment))
                    .collect::<Result<Vec<_>, OpError>>()?
                    .into_iter()
                    .flatten()
                    .collect();
                (files, None, None, false)
            }
        };
        if files.is_empty() && stdout.as_deref().is_none_or(str::is_empty) {
            return Err(fail(
                OpErrorKind::SchemaMismatch,
                codes::E045,
                self.nothing_found(collector),
            ));
        }
        Ok(Evidence {
            files,
            stdout,
            exit_code,
            ran,
        })
    }

    /// What "no evidence" says for this source.
    fn nothing_found(&self, collector: &Collector) -> String {
        match self {
            Source::Run { declared, .. } => format!(
                "{} produced no report at {} (did the tests build?)",
                collector.name,
                declared.report.spelled.display()
            ),
            Source::Written(declared) => format!(
                "no {} report at {}; run the tests to write one (`specforge collect` runs them)",
                collector.name,
                declared.report.spelled.display()
            ),
            Source::Given(_) => "the report files given don't exist".to_string(),
        }
    }
}

/// Whether `collector`'s command `argv` may run in `root`, with the approvals `machine` keeps: `--yes`
/// first, then a command approved for this project before, then whoever can be asked (ADR 0002,
/// ADR 0110). A yes from the asker is remembered. An approval that could not be remembered is
/// noted in `unsaved`.
fn allowed(
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
fn run_command(
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
