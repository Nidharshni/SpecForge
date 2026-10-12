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

mod convention;

use crate::view::ProjectView;
use crate::{OpError, OpErrorKind};
use serde::{Deserialize, Serialize};
use specforge_common::shape::Shape;
use specforge_common::{Code, Diagnostic, DiagnosticList, codes};
use specforge_project::coverage::{ReportedEntity, ReportedTest, TestReport};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_protocol_types::{CollectInput, CollectOutput, CollectReportFile};
use specforge_wasm::{CallError, CallFailure, ExtensionCalls, Operation};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

/// Where `collect` writes the merged results `analyze` reads.
pub use specforge_project::coverage::REPORT_FILE;

/// Environment variable the host sets to the absolute report path when it
/// runs a collector, so a runner integration can write there directly.
pub const REPORT_ENV: &str = "SPECFORGE_REPORT";

/// Overrides the user-level consent store (tests, custom homes).
pub const CONSENT_FILE_ENV: &str = "SPECFORGE_CONSENT_FILE";

/// A collector contributed by an enabled extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collector {
    pub extension: String,
    pub name: String,
    pub export: String,
    pub detect: Vec<String>,
    pub run: Vec<String>,
    /// Report file or directory, relative to the project root.
    pub report: String,
    /// What else the host keeps from the run: `"stdout"` or nothing.
    pub capture: Option<String>,
}

/// The only `capture` value: the command's standard output.
pub const CAPTURE_STDOUT: &str = "stdout";

/// Every collector the enabled extensions declare, in declaration order.
pub fn collectors(declarations: &[ExtensionDeclaration]) -> Vec<Collector> {
    declarations
        .iter()
        .flat_map(|d| {
            d.collectors.iter().map(|c| Collector {
                extension: d.name().to_string(),
                name: c.name.clone(),
                export: c.export.clone(),
                detect: c
                    .auto_detect
                    .as_ref()
                    .map(|d| d.file_patterns.clone())
                    .unwrap_or_default(),
                run: c.run.clone(),
                report: c
                    .report
                    .clone()
                    .unwrap_or_else(|| format!(".specforge/reports/{}.json", c.name)),
                capture: c.capture.clone(),
            })
        })
        .collect()
}

/// Collectors whose detection files are present at the project root.
pub fn detect<'a>(collectors: &'a [Collector], root: &Path) -> Vec<&'a Collector> {
    collectors
        .iter()
        .filter(|c| c.detect.iter().any(|p| pattern_matches(root, p)))
        .collect()
}

/// A detection pattern is a path relative to the project root whose last
/// segment may use `*` wildcards (`vitest.config.*`).
fn pattern_matches(root: &Path, pattern: &str) -> bool {
    let path = Path::new(pattern);
    let Some(file) = path.file_name().and_then(|f| f.to_str()) else {
        return false;
    };
    if !file.contains('*') {
        return root.join(path).exists();
    }
    let dir = root.join(path.parent().unwrap_or(Path::new("")));
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_str().is_some_and(|n| wildcard(file, n)))
}

fn wildcard(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || !name.ends_with(last) || name.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

/// The absolute report location. Extensions may only name paths inside the
/// project: absolute paths and `..` are refused.
pub fn report_path(collector: &Collector, root: &Path) -> Result<PathBuf, String> {
    let relative = Path::new(&collector.report);
    let inside = relative
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if !inside {
        return Err(format!(
            "collector '{}' of {} declares report path '{}' outside the project",
            collector.name, collector.extension, collector.report
        ));
    }
    Ok(root.join(relative))
}

/// Whether the collector keeps its command's standard output. Any value
/// other than `"stdout"` is refused.
pub fn captures_stdout(collector: &Collector) -> Result<bool, String> {
    match collector.capture.as_deref() {
        None => Ok(false),
        Some(CAPTURE_STDOUT) => Ok(true),
        Some(other) => Err(format!(
            "collector '{}' of {} declares unknown capture '{other}' (only \"stdout\")",
            collector.name, collector.extension
        )),
    }
}

/// Where a captured standard output is kept: inside a report directory
/// (a report path without an extension), else next to the report file.
pub fn capture_path(collector: &Collector, report: &Path) -> PathBuf {
    if report.is_dir() || report.extension().is_none() {
        return report.join(format!("{}.stdout.txt", collector.name));
    }
    let mut name = report.file_name().unwrap_or_default().to_os_string();
    name.push(".stdout.txt");
    report.with_file_name(name)
}

/// The captured standard output at `path`, if there is one. Runner output
/// isn't always UTF-8 (tests print what they like), so it's read lossily.
pub fn read_capture(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// The declared command with `{report}` expanded.
pub fn command_line(collector: &Collector, report: &Path) -> Vec<String> {
    let report = report.display().to_string();
    collector
        .run
        .iter()
        .map(|arg| arg.replace("{report}", &report))
        .collect()
}

// ── consent ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConsentStore {
    #[serde(default)]
    approved: Vec<Approval>,
}

/// One approval: this project may run this extension's collector command.
/// A changed command is a different approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Approval {
    project: String,
    extension: String,
    collector: String,
    run: Vec<String>,
}

/// The user-level consent store. It lives outside the project so a cloned
/// repository can't pre-approve its own commands.
pub fn consent_path() -> PathBuf {
    if let Some(path) = std::env::var_os(CONSENT_FILE_ENV) {
        return PathBuf::from(path);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    home.join(".specforge").join("collector-consent.json")
}

fn consent_for(collector: &Collector, root: &Path) -> Approval {
    let project = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    Approval {
        project: project.display().to_string(),
        extension: collector.extension.clone(),
        collector: collector.name.clone(),
        run: collector.run.clone(),
    }
}

fn load_consent(path: &Path) -> ConsentStore {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Whether the user already approved this collector's command here.
pub fn is_approved(store: &Path, collector: &Collector, root: &Path) -> bool {
    let wanted = consent_for(collector, root);
    load_consent(store).approved.contains(&wanted)
}

/// Record the user's approval of this collector's command here.
pub fn approve(store: &Path, collector: &Collector, root: &Path) -> Result<(), String> {
    let mut consents = load_consent(store);
    let wanted = consent_for(collector, root);
    consents.approved.retain(|c| {
        !(c.project == wanted.project
            && c.extension == wanted.extension
            && c.collector == wanted.collector)
    });
    consents.approved.push(wanted);
    if let Some(dir) = store.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(&consents).expect("consent serialization cannot fail");
    std::fs::write(store, json).map_err(|e| format!("{}: {e}", store.display()))
}

// ── running ─────────────────────────────────────────────────────────────────

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

/// What running a collector's command produced.
#[derive(Debug, Clone)]
pub struct Ran {
    /// The runner's exit code; non-zero is normal when tests fail.
    pub exit_code: Option<i32>,
    /// When the command started: a report directory is only read for
    /// files written since, so an old report can never pass for a new run.
    pub started: SystemTime,
    /// The file holding the command's standard output, when the collector
    /// captures it.
    pub stdout: Option<PathBuf>,
}

/// Run the collector's declared command in the project root. A report
/// *file* left by an earlier run is removed first; a report *directory* is
/// left alone (other tools may keep files there) and filtered by
/// modification time when it's read. A captured standard output still
/// reaches the chosen output as it's produced, and is also written to
/// [`capture_path`].
pub fn run(
    collector: &Collector,
    root: &Path,
    report: &Path,
    output: RunnerOutput,
) -> Result<Ran, String> {
    let argv = command_line(collector, report);
    let Some((program, args)) = argv.split_first() else {
        return Err(format!(
            "collector '{}' of {} declares no command; use --no-run with its report",
            collector.name, collector.extension
        ));
    };
    let capture = captures_stdout(collector)?.then(|| capture_path(collector, report));
    if report.is_file() {
        std::fs::remove_file(report).map_err(|e| format!("{}: {e}", report.display()))?;
    }
    for dir in report
        .parent()
        .into_iter()
        .chain(capture.as_deref().and_then(Path::parent))
    {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .current_dir(root)
        .env(REPORT_ENV, report)
        .stdin(std::process::Stdio::null());
    match output {
        RunnerOutput::Inherit => {}
        RunnerOutput::Stderr => {
            cmd.stdout(std::io::stderr());
        }
        RunnerOutput::Discard => {
            cmd.stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
        }
    }
    let started = SystemTime::now();
    let failed = |e: std::io::Error| format!("failed to run `{}`: {e}", argv.join(" "));
    let Some(capture) = capture else {
        let status = cmd.status().map_err(failed)?;
        return Ok(Ran {
            exit_code: status.code(),
            started,
            stdout: None,
        });
    };
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(failed)?;
    let pipe = child.stdout.take().expect("stdout is piped");
    let tee = tee(pipe, &capture, output);
    let status = child.wait().map_err(failed)?;
    tee.map_err(|e| format!("{}: {e}", capture.display()))?;
    Ok(Ran {
        exit_code: status.code(),
        started,
        stdout: Some(capture),
    })
}

/// Copy the child's standard output to `file` and to where `output` sends
/// the runner's output, as it arrives.
fn tee(mut pipe: impl std::io::Read, file: &Path, output: RunnerOutput) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(file)?;
    let mut shown: Box<dyn Write> = match output {
        RunnerOutput::Inherit => Box::new(std::io::stdout()),
        RunnerOutput::Stderr => Box::new(std::io::stderr()),
        RunnerOutput::Discard => Box::new(std::io::sink()),
    };
    let mut buf = [0u8; 8192];
    loop {
        let n = match pipe.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        file.write_all(&buf[..n])?;
        // The terminal going away mustn't lose the capture.
        let _ = shown.write_all(&buf[..n]).and_then(|()| shown.flush());
    }
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files
}

// ── reading and dispatch ────────────────────────────────────────────────────

/// Read the report at `report`: the file itself, or every `*.json` file
/// directly inside a directory, only those modified at or after `since`
/// when given. Paths are reported relative to `root`.
pub fn read_report(
    report: &Path,
    root: &Path,
    since: Option<SystemTime>,
) -> Result<Vec<CollectReportFile>, String> {
    let fresh = |path: &PathBuf| {
        since.is_none_or(|since| {
            std::fs::metadata(path)
                .and_then(|m| m.modified())
                .is_ok_and(|modified| modified >= since)
        })
    };
    let paths = if report.is_dir() {
        json_files(report).into_iter().filter(fresh).collect()
    } else if report.is_file() {
        vec![report.to_path_buf()]
    } else {
        Vec::new()
    };
    paths
        .iter()
        .map(|path| {
            let content = std::fs::read_to_string(path)
                .map_err(|e| format!("failed to read report {}: {e}", path.display()))?;
            let shown = path.strip_prefix(root).unwrap_or(path);
            Ok(CollectReportFile {
                path: shown.display().to_string(),
                content,
            })
        })
        .collect()
}

/// Hand the report files, and the captured standard output if any, to the
/// collector's pure export: the protocol's `CollectInput` in, its
/// `CollectOutput` out. Err: the export trapped, or answered something
/// that is not a `CollectOutput` (E028 naming the collector).
pub fn dispatch(
    runtime: Option<&dyn specforge_wasm::runtime::WasmRuntime>,
    collector: &Collector,
    reports: &[CollectReportFile],
    stdout: Option<&str>,
) -> Result<CollectOutput, CallError> {
    let input = CollectInput {
        reports: reports.to_vec(),
        stdout: stdout.map(str::to_string),
    };
    let Some(runtime) = runtime else {
        return Err(CallError::new(
            Operation::Collect,
            &collector.extension,
            &collector.export,
            CallFailure::NotLoaded,
        ));
    };
    ExtensionCalls::new(runtime).collect(&collector.extension, &collector.export, &input)
}

// ── merging ─────────────────────────────────────────────────────────────────

/// The entities results may name, each with its obligation texts (the
/// `verify` descriptions a convention-linked test may prove).
#[derive(Debug, Clone, Default)]
pub struct KnownEntities(BTreeMap<String, Vec<String>>);

impl KnownEntities {
    /// Every entity of the view's entity snapshot, with its obligation texts.
    pub(crate) fn of(entities: &specforge_project::snapshot::EntitySnapshot) -> Self {
        entities
            .records()
            .iter()
            .map(|record| {
                let texts = record.obligations.iter().map(|o| o.text.clone()).collect();
                (record.id.clone(), texts)
            })
            .collect()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.0.contains_key(id)
    }

    /// The entity's obligation texts, empty for an unknown entity.
    pub fn obligations(&self, id: &str) -> &[String] {
        self.0.get(id).map(Vec::as_slice).unwrap_or_default()
    }
}

impl FromIterator<(String, Vec<String>)> for KnownEntities {
    fn from_iter<I: IntoIterator<Item = (String, Vec<String>)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Counts from one merge.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Shape)]
pub struct MergeStats {
    pub entities: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Replace `runner`'s results in `report` with `collected`. Other runners'
/// results are kept, so several runners can report on one project. Tests
/// that name an unknown entity become W115 warnings, and skipped tests are
/// counted but not recorded: a skipped test proves nothing.
pub fn merge(
    report: &mut TestReport,
    runner: &str,
    collected: &CollectOutput,
    known: &KnownEntities,
) -> (MergeStats, Vec<Diagnostic>) {
    for entity in report.results.values_mut() {
        entity
            .tests
            .retain(|t| t.runner.as_deref().is_some_and(|r| r != runner));
    }
    report.results.retain(|_, e| !e.tests.is_empty());

    let mut stats = MergeStats::default();
    let mut diagnostics = Vec::new();
    let mut unknown = BTreeSet::new();
    for entity in &collected.entity_results {
        if !known.contains(&entity.entity_id) {
            unknown.insert(entity.entity_id.as_str());
            continue;
        }
        let mut recorded = Vec::new();
        for test in &entity.test_results {
            let status = match test.status.as_str() {
                "passed" | "pass" => "pass",
                "failed" | "fail" => "fail",
                _ => {
                    stats.skipped += 1;
                    continue;
                }
            };
            if status == "pass" {
                stats.passed += 1;
            } else {
                stats.failed += 1;
            }
            recorded.push(ReportedTest {
                name: Some(test.name.clone()),
                status: status.to_string(),
                duration_ms: test.duration_ms,
                verify: test.verify.clone(),
                runner: Some(runner.to_string()),
            });
        }
        if recorded.is_empty() {
            continue;
        }
        stats.entities += 1;
        report
            .results
            .entry(entity.entity_id.clone())
            .or_insert_with(|| ReportedEntity {
                file: None,
                tests: Vec::new(),
            })
            .tests
            .extend(recorded);
    }
    for id in unknown {
        diagnostics.push(
            Diagnostic::new(
                codes::W115,
                format!("{runner} reported tests for unknown entity '{id}'"),
            )
            .with_suggestion("check the test's entity annotation for a rename or typo".to_string()),
        );
    }

    let runners: BTreeSet<&str> = report
        .results
        .values()
        .flat_map(|e| e.tests.iter().filter_map(|t| t.runner.as_deref()))
        .collect();
    report.runner =
        (!runners.is_empty()).then(|| runners.into_iter().collect::<Vec<_>>().join(", "));
    (stats, diagnostics)
}

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
    /// the consent store ([`consent_path`]).
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
            report: self.report.display().to_string(),
        }
    }
}

/// What a collect answers with, on both surfaces.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct CollectDocument {
    pub status: CollectStatus,
    pub runners: Vec<RunnerResult>,
    pub diagnostics: DiagnosticList,
    /// The merged test report the collect wrote.
    pub report: String,
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
pub fn collect(view: &ProjectView, request: Request) -> Result<Outcome, OpError> {
    let root = view.project_root()?;
    if !view.env().config_found() {
        return Err(OpError::no_project(format!(
            "no specforge project at {} (no specforge.json at or above it)",
            root.display()
        )));
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
        ));
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
                        "running `{}` needs your approval: run `specforge collect` in a \
                         terminal, pass --yes, or parse an existing report with --no-run",
                        argv.join(" ")
                    ),
                ));
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
            return Err(fail(OpErrorKind::SchemaMismatch, codes::E045, message));
        }

        let mut collected = dispatch(
            view.runtime().map(|r| r.as_ref()),
            collector,
            &files,
            stdout.as_deref(),
        )
        .map_err(|error| OpError::from(error.diagnostic()))?;
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
        runners,
        diagnostics,
        report,
        unsaved_approvals,
    })
}

/// The collectors to use: the one `runner` names, else every collector
/// whose detection files are present.
fn select<'a>(
    available: &'a [Collector],
    runner: Option<&str>,
    root: &Path,
) -> Result<Vec<&'a Collector>, OpError> {
    if available.is_empty() {
        return Err(fail(
            OpErrorKind::PreconditionFailed,
            codes::E058,
            "no enabled extension provides a test collector; enable a runner extension \
             (e.g. `specforge add @specforge/cargo-test`)",
        ));
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
                    OpErrorKind::PreconditionFailed,
                    codes::E058,
                    format!("unknown runner '{runner}' (available: {})", names()),
                )
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
        ));
    }
    Ok(detected)
}

/// The report `collect` merges new results into: `specforge-report.json`,
/// or an empty report when it's missing or unreadable. Unlike the readers
/// that score coverage ([`specforge_project::coverage::read_report`]), collect is the
/// writer: it replaces a report it cannot read.
pub fn load_report(root: &Path) -> TestReport {
    specforge_project::coverage::read_report(root)
        .ok()
        .flatten()
        .unwrap_or(TestReport {
            runner: None,
            results: BTreeMap::new(),
        })
}

/// Write `specforge-report.json`.
pub fn save_report(root: &Path, report: &TestReport) -> Result<PathBuf, OpError> {
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
mod tests {
    use super::*;
    use specforge_common::Severity;
    use specforge_protocol_types::{CollectEntityResult, CollectTestResult};
    use specforge_test_macros::test as specforge_test;

    fn collector(report: &str) -> Collector {
        Collector {
            extension: "@acme/runner".into(),
            name: "acme".into(),
            export: "collect__acme".into(),
            detect: vec!["acme.config.*".into()],
            run: vec!["acme".into(), "--out={report}".into()],
            report: report.into(),
            capture: None,
        }
    }

    #[specforge_test(
        behavior = "auto_detect_collector",
        verify = "wildcards match within the last path segment"
    )]
    fn wildcard_matches_segments() {
        assert!(wildcard("vitest.config.*", "vitest.config.ts"));
        assert!(wildcard("*.json", "a.json"));
        assert!(wildcard("a*b*c", "a-b-c"));
        assert!(!wildcard("vitest.config.*", "vite.config.ts"));
        assert!(!wildcard("a*a", "a"));
    }

    #[specforge_test(
        behavior = "auto_detect_collector",
        verify = "file pattern match selects collector"
    )]
    fn detects_by_root_files() {
        let dir = tempfile::tempdir().unwrap();
        let all = vec![collector("r.json")];
        assert!(detect(&all, dir.path()).is_empty());
        std::fs::write(dir.path().join("acme.config.js"), "").unwrap();
        assert_eq!(detect(&all, dir.path()).len(), 1);
    }

    #[specforge_test(
        behavior = "run_collector_command",
        verify = "report path must stay inside the project"
    )]
    fn report_path_must_stay_inside_the_project() {
        let root = Path::new("/p");
        assert_eq!(
            report_path(&collector("out/r.json"), root).unwrap(),
            Path::new("/p/out/r.json")
        );
        assert!(report_path(&collector("../r.json"), root).is_err());
        assert!(report_path(&collector("/etc/r.json"), root).is_err());
    }

    #[specforge_test(
        behavior = "run_collector_command",
        verify = "command line expands the report placeholder"
    )]
    fn command_line_expands_the_report_placeholder() {
        let argv = command_line(&collector("r.json"), Path::new("/p/r.json"));
        assert_eq!(argv, vec!["acme", "--out=/p/r.json"]);
    }

    #[specforge_test(
        behavior = "approve_collector_command",
        verify = "consent is keyed by project, extension and command"
    )]
    fn consent_is_keyed_by_project_extension_and_command() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("consent.json");
        let root = dir.path();
        let mut c = collector("r.json");
        assert!(!is_approved(&store, &c, root));
        approve(&store, &c, root).unwrap();
        assert!(is_approved(&store, &c, root));
        assert!(!is_approved(&store, &c, &root.join("other")));
        c.run.push("--extra".into());
        assert!(
            !is_approved(&store, &c, root),
            "a changed command re-prompts"
        );
        approve(&store, &c, root).unwrap();
        assert_eq!(load_consent(&store).approved.len(), 1, "approval replaced");
    }

    #[test]
    fn consent_yes_approved_and_prompt_decide_as_documented() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = dir.path().join("consent.json");
        let c = collector("r.json");
        let argv = c.run.clone();
        let mut unsaved = Vec::new();

        // --yes runs it and remembers nothing.
        assert!(Consent::Yes.allows(&store, &c, &argv, dir.path(), &mut unsaved));
        assert!(!is_approved(&store, &c, dir.path()));
        // Nothing approved yet: refused without asking.
        assert!(!Consent::Approved.allows(&store, &c, &argv, dir.path(), &mut unsaved));
        // A prompt that says no is not remembered...
        let mut no = |_: &Collector, _: &[String]| false;
        assert!(!Consent::Prompt(&mut no).allows(&store, &c, &argv, dir.path(), &mut unsaved));
        assert!(!is_approved(&store, &c, dir.path()));
        // ...a yes is, and is not asked again.
        let mut asked = 0;
        let mut yes = |_: &Collector, _: &[String]| {
            asked += 1;
            true
        };
        {
            let mut prompt = Consent::Prompt(&mut yes);
            assert!(prompt.allows(&store, &c, &argv, dir.path(), &mut unsaved));
            assert!(prompt.allows(&store, &c, &argv, dir.path(), &mut unsaved));
        }
        assert_eq!(asked, 1);
        assert!(Consent::Approved.allows(&store, &c, &argv, dir.path(), &mut unsaved));
        assert!(unsaved.is_empty(), "{unsaved:?}");
    }

    #[specforge_test(
        behavior = "dispatch_collector",
        verify = "reads a file or every json file in a directory"
    )]
    fn reads_a_file_or_every_json_file_in_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let reports = dir.path().join("out");
        std::fs::create_dir(&reports).unwrap();
        std::fs::write(reports.join("b.json"), "{}").unwrap();
        std::fs::write(reports.join("a.json"), "[]").unwrap();
        std::fs::write(reports.join("notes.txt"), "x").unwrap();
        let files = read_report(&reports, dir.path(), None).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["out/a.json", "out/b.json"]);
        assert!(
            read_report(&dir.path().join("none.json"), dir.path(), None)
                .unwrap()
                .is_empty()
        );
    }

    #[specforge_test(
        behavior = "run_collector_command",
        verify = "run ignores stale reports and sets the report env"
    )]
    fn run_ignores_stale_reports_and_sets_the_report_env() {
        let dir = tempfile::tempdir().unwrap();
        let report = dir.path().join("r.json");
        std::fs::write(&report, "stale").unwrap();
        let mut c = collector("r.json");
        c.run = vec![
            "sh".into(),
            "-c".into(),
            format!("test ! -e {{report}} && printf fresh > \"${REPORT_ENV}\""),
        ];
        let ran = run(&c, dir.path(), &report, RunnerOutput::Discard).unwrap();
        assert_eq!(ran.exit_code, Some(0));
        assert_eq!(std::fs::read_to_string(&report).unwrap(), "fresh");

        // A report directory keeps its other files; only files the run
        // wrote are read.
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        std::fs::write(out.join("old.json"), "{}").unwrap();
        std::fs::write(out.join("graph.json"), "{}").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        c.run = vec![
            "sh".into(),
            "-c".into(),
            format!("printf '{{}}' > \"${REPORT_ENV}/new.json\""),
        ];
        let ran = run(&c, dir.path(), &out, RunnerOutput::Discard).unwrap();
        assert!(out.join("old.json").exists() && out.join("graph.json").exists());
        let read = read_report(&out, dir.path(), Some(ran.started)).unwrap();
        let paths: Vec<&str> = read.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["out/new.json"]);
    }

    #[specforge_test(
        behavior = "run_collector_command",
        verify = "a captured stdout is kept in every output mode"
    )]
    fn a_captured_stdout_is_kept_in_every_output_mode() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let mut c = collector("out");
        c.run = vec!["sh".into(), "-c".into(), "echo 'test a ... ok'".into()];
        c.capture = Some(CAPTURE_STDOUT.into());
        for mode in [
            RunnerOutput::Discard,
            RunnerOutput::Stderr,
            RunnerOutput::Inherit,
        ] {
            let ran = run(&c, dir.path(), &out, mode).unwrap();
            let kept = ran.stdout.expect("captured");
            assert_eq!(kept, out.join("acme.stdout.txt"));
            assert_eq!(read_capture(&kept).as_deref(), Some("test a ... ok\n"));
        }
        // Not captured without the declaration.
        c.capture = None;
        std::fs::remove_file(out.join("acme.stdout.txt")).unwrap();
        let ran = run(&c, dir.path(), &out, RunnerOutput::Discard).unwrap();
        assert!(ran.stdout.is_none());
        assert!(!out.join("acme.stdout.txt").exists());
    }

    #[specforge_test(
        behavior = "run_collector_command",
        verify = "an unknown capture is refused"
    )]
    fn an_unknown_capture_is_refused() {
        let mut c = collector("r.json");
        assert_eq!(captures_stdout(&c), Ok(false));
        c.capture = Some("stderr".into());
        let err = captures_stdout(&c).unwrap_err();
        assert!(err.contains("unknown capture 'stderr'"), "{err}");
        // A file report keeps its capture next to it.
        assert_eq!(
            capture_path(&c, Path::new("/p/out/r.json")),
            Path::new("/p/out/r.json.stdout.txt")
        );
    }

    fn collected(entries: &[(&str, &str, &str)]) -> CollectOutput {
        let mut by_entity: BTreeMap<&str, Vec<CollectTestResult>> = BTreeMap::new();
        for (id, name, status) in entries {
            by_entity.entry(id).or_default().push(CollectTestResult {
                name: name.to_string(),
                status: status.to_string(),
                verify: None,
                duration_ms: None,
            });
        }
        CollectOutput {
            entity_results: by_entity
                .into_iter()
                .map(|(id, tests)| CollectEntityResult {
                    entity_id: id.to_string(),
                    test_results: tests,
                })
                .collect(),
            unlinked: Vec::new(),
        }
    }

    #[specforge_test(
        behavior = "ingest_collector_report",
        verify = "the entities results may name are the entity snapshot's, with their obligation texts"
    )]
    fn known_entities_are_the_snapshots_records() {
        use specforge_common::{SourceSpan, Sym};
        use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue, VerifyStatement};

        let mut fixture = crate::view::testing::Fixture::new();
        let mut fields = FieldMap::new();
        fields.push(
            Sym::new("verify"),
            FieldValue::VerifyList(vec![
                VerifyStatement {
                    kind: "unit".into(),
                    description: "first".into(),
                },
                VerifyStatement {
                    kind: "integration".into(),
                    description: "second".into(),
                },
            ]),
        );
        fixture.graph.add_node(specforge_graph::Node {
            id: EntityId {
                raw: Sym::new("widget"),
            },
            kind: EntityKind {
                raw: Sym::new("behavior"),
            },
            title: None,
            fields,
            source_span: SourceSpan {
                file: Sym::new("t.spec"),
                start_line: 1,
                start_col: 1,
                end_line: 1,
                end_col: 1,
            },
            methods: Vec::new(),
        });
        let view = fixture.view();

        let known = KnownEntities::of(view.entities());

        assert!(known.contains("widget"));
        assert!(!known.contains("gadget"));
        assert_eq!(known.obligations("widget"), ["first", "second"]);
    }

    #[specforge_test(
        behavior = "ingest_collector_report",
        verify = "merge replaces only the same runner"
    )]
    fn merge_replaces_only_the_same_runner() {
        let known = KnownEntities::from_iter(["a", "b"].map(|s| (s.to_string(), Vec::new())));
        let mut report = TestReport {
            runner: None,
            results: BTreeMap::new(),
        };
        merge(
            &mut report,
            "vitest",
            &collected(&[("a", "t1", "passed")]),
            &known,
        );
        let (stats, diags) = merge(
            &mut report,
            "cargo-test",
            &collected(&[
                ("a", "t2", "failed"),
                ("b", "t3", "skipped"),
                ("zz", "t4", "passed"),
            ]),
            &known,
        );
        assert_eq!(
            stats,
            MergeStats {
                entities: 1,
                passed: 0,
                failed: 1,
                skipped: 1
            }
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "W115");
        assert_eq!(report.results["a"].tests.len(), 2);
        assert!(!report.results.contains_key("b"));
        assert_eq!(report.runner.as_deref(), Some("cargo-test, vitest"));

        // Re-collecting cargo-test drops its old results, keeps vitest's.
        merge(&mut report, "cargo-test", &collected(&[]), &known);
        assert_eq!(report.results["a"].tests.len(), 1);
        assert_eq!(report.results["a"].tests[0].status, "pass");
        assert_eq!(report.runner.as_deref(), Some("vitest"));
    }

    fn empty_report() -> TestReport {
        TestReport {
            runner: None,
            results: BTreeMap::new(),
        }
    }

    #[specforge_test(
        invariant = "collector_output_conformance",
        verify = "unknown entity ID in collector entry produces W115"
    )]
    fn unknown_entities_are_dropped_with_w115() {
        let known = KnownEntities::from_iter([("a".to_string(), Vec::new())]);
        let mut report = empty_report();
        let (_, diags) = merge(
            &mut report,
            "r",
            &collected(&[("zz", "t", "passed")]),
            &known,
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "W115");
        assert_eq!(diags[0].severity, Severity::Warning);
        assert!(report.results.is_empty());
    }

    #[specforge_test(
        invariant = "collector_output_conformance",
        verify = "skipped tests are not recorded as proof"
    )]
    fn skipped_tests_are_not_recorded() {
        let known = KnownEntities::from_iter([("a".to_string(), Vec::new())]);
        let mut report = empty_report();
        let (stats, _) = merge(
            &mut report,
            "r",
            &collected(&[("a", "t", "skipped")]),
            &known,
        );
        assert_eq!(stats.skipped, 1);
        assert!(!report.results.contains_key("a"));
    }

    #[test]
    fn collect_without_a_root_is_no_project() {
        let fixture = crate::view::testing::Fixture::new();

        let error = collect(
            &fixture.rootless_view(),
            Request {
                runner: None,
                mode: Mode::NoRun,
                consent: Consent::Approved,
                announce: &mut |_, _| {},
            },
        )
        .unwrap_err();

        assert_eq!(error.code, "no_project");
        assert!(
            !fixture.dir.path().join("specforge-report.json").exists(),
            "nothing is written"
        );
    }
}
