//! Test-result collection (ADR 0002).
//!
//! A runner extension declares a *collector*: the files that select it, the
//! command that runs its test runner, where the runner leaves its report,
//! and a pure `collect__<name>` export that maps the report to entity
//! results. The host does everything that touches the machine: it asks for
//! consent, runs the declared command, reads the report and merges the
//! extension's answer into `specforge-report.json`. The extension never
//! runs anything itself.
//!
//! The CLI and MCP are adapters: they hand the operation who can be asked
//! (`crate::asking::Asking`) and the machine it runs on ([`CollectorMachine`], ADR 0125),
//! and present the [`Outcome`].

mod collector;
mod convention;
mod dispatch;
mod merge;
mod process;

pub use crate::machine::{
    CollectorCommand, CollectorMachine, CommandApproval, REPORT_ENV, Ran, RunFailure, RunnerOutput,
};
pub use collector::{Collector, collectors};
use collector::{capture_path, captures_stdout, command_line, detect, read_capture, report_path};
pub use dispatch::dispatch;
use dispatch::read_report;
pub use merge::MergeStats;
use merge::{KnownEntities, merge};
pub use process::ProcessMachine;

use crate::asking::{Asking, Question};
use crate::view::ProjectView;
use crate::{Caller, Containment, Held, OpError, OpErrorKind, Refusal};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Code, Diagnostic, DiagnosticList, ProjectPath, codes};
use specforge_project::coverage::TestReport;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use specforge_project::coverage::REPORT_FILE;

// ── the whole flow ──────────────────────────────────────────────────────────

/// How to get each collector's report.
#[derive(Debug, Clone, Copy)]
pub enum Mode<'a> {
    /// Run the declared command (after approval), then read its report.
    Run(RunnerOutput),
    /// Read the report already at the declared location.
    NoRun,
    /// Read these files instead (one collector only).
    Reports(&'a [PathBuf]),
}

/// A `collect` request: what to collect, and who decides and hears about
/// a command that runs.
pub struct Request<'a> {
    /// Collector name or extension; detected from project files when absent.
    pub runner: Option<&'a str>,
    pub mode: Mode<'a>,
    /// Told just before a collector's command runs.
    pub announce: &'a mut dyn FnMut(&Collector, &[String]),
    /// What the collect may reach and who can be asked. Every file it reads or writes must land
    /// inside `caller.containment()` (ADR 0104 D3, ADR 0112 D2): the report, a collector's
    /// report files and its captured output; a name that lands outside is refused before any
    /// command runs, for every collector. `caller.asking()` decides whether a collector's command may run; the consent
    /// store is read first (ADR 0110).
    pub caller: Caller<'a>,
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

/// A failure reported as diagnostic `code` (E058, E045).
fn fail(kind: OpErrorKind, code: Code, message: impl Into<String>) -> OpError {
    OpError::coded(kind, code, message)
}

/// What happened for one collector.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct RunnerResult {
    pub name: String,
    pub extension: String,
    pub ran: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub files: usize,
    #[serde(flatten)]
    pub stats: MergeStats,
    /// Tests linked by naming convention rather than by the report.
    pub by_convention: usize,
}

/// The outcome of a successful collect.
#[derive(Debug)]
pub struct Outcome {
    /// The project the collect ran in: the document names the report from it.
    pub root: PathBuf,
    pub runners: Vec<RunnerResult>,
    pub diagnostics: Vec<Diagnostic>,
    pub report: PathBuf,
    /// Approvals given at a prompt that could not be remembered (the
    /// consent store could not be written), each with why.
    pub unsaved_approvals: Vec<String>,
}

impl Outcome {
    /// The document both surfaces answer with:
    /// `{status, runners, diagnostics, report}`.
    pub fn document(&self) -> CollectDocument {
        CollectDocument {
            status: CollectStatus::Collected,
            runners: self.runners.clone(),
            diagnostics: DiagnosticList(self.diagnostics.clone()),
            report: ProjectPath::of(&self.root, &self.report),
        }
    }
}

/// What a collect answers with, on both surfaces.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct CollectDocument {
    pub status: CollectStatus,
    pub runners: Vec<RunnerResult>,
    pub diagnostics: DiagnosticList,
    /// The merged test report the collect wrote, named from the project root.
    pub report: ProjectPath,
}

/// A collect that finished says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "lowercase")]
pub enum CollectStatus {
    Collected,
}

/// Collect test results for the project the view was compiled from:
/// select collectors among its declarations', run (in `runtime`) or read
/// each one's report, map it through the extension to the view's entities
/// and merge the answer into `<root>/specforge-report.json`. Every refusal that needs no run (a
/// report location, a capture, a command, an approval) is decided for every selected collector
/// before the first command runs (ADR 0125); the request's `caller.asking()` decides whether a
/// collector's command may run, and its `announce` is told just before it runs. The machine runs
/// the commands and keeps the approvals. Without a root, or at a root that holds no project:
/// `no_project`.
pub fn collect(
    view: &ProjectView,
    request: Request,
    machine: &dyn CollectorMachine,
) -> Result<Outcome, Refusal<CollectField>> {
    let root = view.project_root()?;
    if !view.env().config_found() {
        return Err(OpError::no_project(format!(
            "no specforge project at {} (no specforge.json at or above it)",
            root.display()
        ))
        .into());
    }
    let Request {
        runner,
        mode,
        announce,
        caller,
    } = request;
    let (asking, containment) = (caller.asking(), caller.containment());
    let known = &KnownEntities::of(view.entities());
    let available = collectors(view.registries().declarations());
    let parse_only = !matches!(mode, Mode::Run(_));
    let selected = select(&available, runner, root)?;
    if let Mode::Reports(_) = mode
        && selected.len() > 1
    {
        let names: Vec<&str> = selected.iter().map(|c| c.name.as_str()).collect();
        return Err(fail(
            OpErrorKind::PreconditionFailed,
            codes::E058,
            format!(
                "report files need one collector, but {} apply here; name the runner that wrote them",
                names.join(", ")
            ),
        )
        .into());
    }

    // Held before anything runs: what is read and what is written is the file checked here.
    let report_file = containment
        .resolve(&root.join(REPORT_FILE))
        .map_err(OpError::from)?;
    let mut report = load_report(&report_file);
    let mut runners = Vec::new();
    let mut diagnostics = Vec::new();
    let mut unsaved_approvals = Vec::new();
    // Decided for every collector before anything runs: a refusal that needs no run (a report
    // location, a capture, a command, an approval) never follows an earlier collector's command.
    let planned = selected
        .into_iter()
        .map(|collector| {
            plan(
                collector,
                root,
                mode,
                asking,
                containment,
                machine,
                &mut unsaved_approvals,
            )
        })
        .collect::<Result<Vec<_>, OpError>>()?;
    for Planned {
        collector,
        report_at,
        capture,
        argv,
    } in planned
    {
        let mut exit_code = None;
        let mut since = None;
        let mut stdout = None;
        if let Mode::Run(output) = mode {
            announce(collector, &argv);
            let ran = run_command(machine, root, &report_at, capture.as_ref(), &argv, output)
                .map_err(|m| fail(OpErrorKind::SchemaMismatch, codes::E045, m))?;
            exit_code = ran.exit_code;
            since = Some(ran.started);
            stdout = capture.as_ref().and_then(read_capture);
        } else if let Mode::NoRun = mode {
            stdout = capture.as_ref().and_then(read_capture);
        }

        let files = match mode {
            Mode::Reports(paths) => paths
                .iter()
                .map(|p| read_report(&containment.resolve(p)?, root, None, containment))
                .collect::<Result<Vec<_>, OpError>>()
                .map(|files| files.into_iter().flatten().collect()),
            _ => read_report(&report_at, root, since, containment),
        }?;
        if files.is_empty() && stdout.as_deref().is_none_or(str::is_empty) {
            let message = match mode {
                Mode::Run(_) => format!(
                    "{} produced no report at {} (did the tests build?)",
                    collector.name,
                    report_at.spelled.display()
                ),
                Mode::NoRun => format!(
                    "no {} report at {}; run the tests to write one (`specforge collect` runs them)",
                    collector.name,
                    report_at.spelled.display()
                ),
                Mode::Reports(_) => "the report files given don't exist".to_string(),
            };
            return Err(fail(OpErrorKind::SchemaMismatch, codes::E045, message).into());
        }

        let mut collected = dispatch(
            view.runtime().map(|r| r.as_ref()),
            collector,
            &files,
            stdout.as_deref(),
        )
        .map_err(|error| OpError::of(OpErrorKind::Internal, error.diagnostic()))?;
        let (by_convention, diags) = convention::resolve(&collected.unlinked, known);
        diagnostics.extend(diags);
        let by_convention_count = by_convention.iter().map(|e| e.test_results.len()).sum();
        collected.entity_results.extend(by_convention);
        let (stats, diags) = merge(&mut report, &collector.name, &collected, known);
        diagnostics.extend(diags);
        runners.push(RunnerResult {
            name: collector.name.clone(),
            extension: collector.extension.clone(),
            ran: !parse_only,
            exit_code,
            files: files.len(),
            stats,
            by_convention: by_convention_count,
        });
    }

    let report = save_report(&report_file, &report)?;
    Ok(Outcome {
        root: root.to_path_buf(),
        runners,
        diagnostics,
        report,
        unsaved_approvals,
    })
}

/// What one selected collector will do, decided before any collector's command runs.
struct Planned<'a> {
    collector: &'a Collector,
    report_at: Held,
    capture: Option<Held>,
    argv: Vec<String>,
}

/// Decide `collector`'s part of the collect without running anything or writing a file: where its
/// report and captured output land, its command line, and under [`Mode::Run`] that it has a command
/// and that the command may run (asking whoever can be asked; E059 otherwise).
fn plan<'a>(
    collector: &'a Collector,
    root: &Path,
    mode: Mode<'_>,
    asking: Asking<'_>,
    containment: &Containment,
    machine: &dyn CollectorMachine,
    unsaved_approvals: &mut Vec<String>,
) -> Result<Planned<'a>, OpError> {
    let report_at = report_path(collector, root, containment)?;
    let capturing = captures_stdout(collector)
        .map_err(|m| fail(OpErrorKind::PreconditionFailed, codes::E058, m))?;
    let capture = if capturing {
        Some(capture_path(collector, &report_at, containment)?)
    } else {
        None
    };
    let argv = command_line(collector, &report_at.spelled);
    if let Mode::Run(_) = mode {
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
    }
    Ok(Planned {
        collector,
        report_at,
        capture,
        argv,
    })
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
            held.create()
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

/// The field of a collect request a refusal can be about (ADR 0072).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectField {
    /// `Request::runner`: the collector asked for.
    Runner,
}

/// The collectors to use: the one `runner` names, else every collector
/// whose detection files are present. A runner no collector answers to is
/// invalid input about the runner (E058).
fn select<'a>(
    available: &'a [Collector],
    runner: Option<&str>,
    root: &Path,
) -> Result<Vec<&'a Collector>, Refusal<CollectField>> {
    if available.is_empty() {
        return Err(fail(
            OpErrorKind::PreconditionFailed,
            codes::E058,
            "no enabled extension provides a test collector; enable a runner extension \
             (e.g. `specforge add @specforge/cargo-test`)",
        )
        .into());
    }
    let names = || {
        available
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(runner) = runner {
        return available
            .iter()
            .find(|c| c.name == runner || c.extension == runner)
            .map(|c| vec![c])
            .ok_or_else(|| {
                fail(
                    OpErrorKind::InvalidInput,
                    codes::E058,
                    format!("unknown runner '{runner}' (available: {})", names()),
                )
                .about(CollectField::Runner)
            });
    }
    let detected = detect(available, root);
    // A project that enabled a single runner chose it: detection only
    // decides between several (vitest configured inside vite.config.ts,
    // for one, has no file of its own to detect).
    if detected.is_empty() && available.len() == 1 {
        return Ok(vec![&available[0]]);
    }
    if detected.is_empty() {
        return Err(fail(
            OpErrorKind::PreconditionFailed,
            codes::E058,
            format!(
                "no test runner detected in {}; name the runner to use (available: {})",
                root.display(),
                names()
            ),
        )
        .into());
    }
    Ok(detected)
}

/// The report `collect` merges new results into: `specforge-report.json`,
/// or an empty report when it's missing or unreadable. Unlike the readers
/// that score coverage ([`specforge_project::coverage::read_report`]), collect is the
/// writer: it replaces a report it cannot read.
fn load_report(file: &Held) -> TestReport {
    specforge_project::coverage::read_report_file(&file.real)
        .ok()
        .unwrap_or(TestReport {
            runner: None,
            results: BTreeMap::new(),
        })
}

/// Write `specforge-report.json`, replacing it atomically where it is held.
fn save_report(file: &Held, report: &TestReport) -> Result<PathBuf, OpError> {
    let json = serde_json::to_string_pretty(report).expect("report serialization cannot fail");
    file.replace(json).map_err(|e| {
        OpError::coded(
            OpErrorKind::of_io(&e),
            codes::E056,
            format!("failed to write {}: {e}", file.spelled.display()),
        )
    })?;
    Ok(file.spelled.clone())
}

#[cfg(test)]
mod tests;
