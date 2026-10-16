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
//! The CLI and MCP are adapters: they choose the [`Consent`] (a terminal
//! prompt, `--yes`, or only what was approved before) and present the
//! [`Outcome`].

mod collector;
mod consent;
mod convention;
mod dispatch;
mod merge;
mod runner;

pub use collector::{Collector, collectors};
use collector::{capture_path, captures_stdout, command_line, detect, read_capture, report_path};
use consent::{approve, consent_path, is_approved};
pub use dispatch::dispatch;
use dispatch::read_report;
pub use merge::MergeStats;
use merge::{KnownEntities, merge};
pub use runner::RunnerOutput;
use runner::run;

use crate::view::ProjectView;
use crate::{OpError, OpErrorKind, Refusal};
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
    /// Who decides whether a collector's command may run.
    pub consent: Consent<'a>,
    /// Told just before a collector's command runs.
    pub announce: &'a mut dyn FnMut(&Collector, &[String]),
}

/// Who decides whether a collector's command may run (ADR 0002: consent is
/// given per project, extension and command).
pub enum Consent<'a> {
    /// Every command may run (`--yes`, for CI).
    Yes,
    /// Only a command the user approved for this project before: MCP,
    /// which never prompts, and a CLI with nobody to ask.
    Approved,
    /// A command not approved yet is put to `ask`; a yes is remembered in
    /// the consent store (`consent_path`).
    Prompt(&'a mut dyn FnMut(&Collector, &[String]) -> bool),
}

impl Consent<'_> {
    /// Whether `collector`'s command `argv` may run in `root`, with the
    /// approvals in `store`. An approval that could not be remembered is
    /// noted in `unsaved`.
    fn allows(
        &mut self,
        store: &Path,
        collector: &Collector,
        argv: &[String],
        root: &Path,
        unsaved: &mut Vec<String>,
    ) -> bool {
        match self {
            Consent::Yes => true,
            _ if is_approved(store, collector, root) => true,
            Consent::Approved => false,
            Consent::Prompt(ask) => {
                if !ask(collector, argv) {
                    return false;
                }
                if let Err(e) = approve(store, collector, root) {
                    unsaved.push(e);
                }
                true
            }
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
/// and merge the answer into `<root>/specforge-report.json`. The request's
/// `consent` decides whether a collector's command may run; its `announce`
/// is told just before it runs. Without a root, or at a root that holds no
/// project: `no_project`.
pub fn collect(view: &ProjectView, request: Request) -> Result<Outcome, Refusal<CollectField>> {
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
        mut consent,
        announce,
    } = request;
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
                "--report needs one collector, but {} apply here; pick one with --runner",
                names.join(", ")
            ),
        )
        .into());
    }

    let mut report = load_report(root);
    let mut runners = Vec::new();
    let mut diagnostics = Vec::new();
    let mut unsaved_approvals = Vec::new();
    let store = consent_path();
    for collector in selected {
        let report_at = report_path(collector, root)
            .map_err(|m| fail(OpErrorKind::PreconditionFailed, codes::E058, m))?;
        let capturing = captures_stdout(collector)
            .map_err(|m| fail(OpErrorKind::PreconditionFailed, codes::E058, m))?;
        let argv = command_line(collector, &report_at);
        let mut exit_code = None;
        let mut since = None;
        let mut stdout = None;
        if let Mode::Run(output) = mode {
            if !consent.allows(&store, collector, &argv, root, &mut unsaved_approvals) {
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
                )
                .into());
            }
            announce(collector, &argv);
            let ran = run(collector, root, &report_at, output)
                .map_err(|m| fail(OpErrorKind::SchemaMismatch, codes::E045, m))?;
            exit_code = ran.exit_code;
            since = Some(ran.started);
            stdout = ran.stdout.as_deref().and_then(read_capture);
        } else if let Mode::NoRun = mode
            && capturing
        {
            stdout = read_capture(&capture_path(collector, &report_at));
        }

        let files = match mode {
            Mode::Reports(paths) => paths
                .iter()
                .map(|p| read_report(p, root, None))
                .collect::<Result<Vec<_>, _>>()
                .map(|files| files.into_iter().flatten().collect()),
            _ => read_report(&report_at, root, since),
        }
        .map_err(|m| fail(OpErrorKind::SchemaMismatch, codes::E045, m))?;
        if files.is_empty() && stdout.as_deref().is_none_or(str::is_empty) {
            let message = match mode {
                Mode::Run(_) => format!(
                    "{} produced no report at {} (did the tests build?)",
                    collector.name,
                    report_at.display()
                ),
                Mode::NoRun => format!(
                    "no {} report at {}; run without --no-run to produce one",
                    collector.name,
                    report_at.display()
                ),
                Mode::Reports(_) => "the --report files don't exist".to_string(),
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

    let report = save_report(root, &report)?;
    Ok(Outcome {
        root: root.to_path_buf(),
        runners,
        diagnostics,
        report,
        unsaved_approvals,
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
                "no test runner detected in {}; pick one with --runner (available: {})",
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
fn load_report(root: &Path) -> TestReport {
    specforge_project::coverage::read_report(root)
        .ok()
        .flatten()
        .unwrap_or(TestReport {
            runner: None,
            results: BTreeMap::new(),
        })
}

/// Write `specforge-report.json`.
fn save_report(root: &Path, report: &TestReport) -> Result<PathBuf, OpError> {
    let path = root.join(REPORT_FILE);
    let json = serde_json::to_string_pretty(report).expect("report serialization cannot fail");
    std::fs::write(&path, json).map_err(|e| {
        OpError::coded(
            OpErrorKind::of_io(&e),
            codes::E056,
            format!("failed to write {}: {e}", path.display()),
        )
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests;
