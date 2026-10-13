//! `specforge analyze`: run the analysis passes over a [`ProjectView`].
//!
//! One operation serves the CLI and the MCP tool: it validates the pass
//! selection, reads the test report, runs the built-in passes, the
//! extension passes (in the view's runtime) and, when asked,
//! `prove`, then applies strictness once and computes `ok` once. The shape of
//! the JSON document lives in [`AnalyzeOutcome::document`] and nowhere else.
//!
//! Surfaces keep rendering, exit-code or error-channel mapping, and the
//! choice of which project to analyse.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use specforge_common::shape::Shape;

use crate::builtin_passes::{COVERAGE_PASS, PASS_NAMES};
use specforge_common::{Diagnostic, DiagnosticList, Severity, codes};
use specforge_graph::Graph;
use specforge_project::coverage;
use specforge_project::coverage::TestReport;
use specforge_project::passes::{self, AnalysisContext};
use specforge_registry::DeclaredPass;

use crate::solver::Solver;
use crate::{OpError, OpErrorKind, Refusal, RunVerdict};

pub use crate::view::ProjectView;

/// Where the test report comes from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ReportSource {
    /// The view's recorded report: what `specforge collect` last wrote at
    /// the root the project was compiled from, if anything (never an
    /// ancestor's: a sub-path does not inherit its parent's report).
    #[default]
    Recorded,
    /// A named report file, which must exist.
    File(PathBuf),
    /// No report.
    None,
}

/// The pass name that runs every pass: what an absent pass means on every
/// surface (`specforge analyze`, `specforge.analyze`).
pub const EVERY_PASS: &str = "all";
/// A pass name no pass answers to (`InvalidInput`).
pub const UNKNOWN_PASS: &str = "unknown_pass";
/// A coverage minimum outside 0 to 100 (`InvalidInput`).
pub const INVALID_MIN: &str = "invalid_min";

/// What to run. `Default` is a plain `analyze all`.
#[derive(Clone)]
pub struct AnalyzeOptions<'s> {
    /// `all`, `coverage`, a built-in pass, or a declared `<extension>:<pass>`.
    pub pass: String,
    pub strict: bool,
    pub report: ReportSource,
    /// The proof-coverage threshold the caller will gate on; it needs a report.
    pub min: Option<f64>,
    /// The solver to run the prove step with (`specforge analyze --prove`);
    /// `None` runs no prove step, and the extension passes receive no proved
    /// claims.
    pub prove: Option<&'s dyn Solver>,
}

impl std::fmt::Debug for AnalyzeOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalyzeOptions")
            .field("pass", &self.pass)
            .field("strict", &self.strict)
            .field("report", &self.report)
            .field("min", &self.min)
            .field("prove", &self.prove.is_some())
            .finish()
    }
}

impl Default for AnalyzeOptions<'_> {
    fn default() -> Self {
        Self {
            pass: EVERY_PASS.to_string(),
            strict: false,
            report: ReportSource::default(),
            min: None,
            prove: None,
        }
    }
}

/// The result of one pass (built-in, extension-owned or `prove`).
#[derive(Debug, Clone)]
pub struct PassOutcome {
    pub name: String,
    pub description: String,
    pub findings: Vec<Diagnostic>,
    pub summary: serde_json::Value,
}

/// W097: a stray test record (CONTEXT.md): a record of the recorded test
/// report naming an entity the graph does not have, with the closest known
/// id when one is near. Not a finding of any pass, so strict never promotes
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct StrayRecord {
    pub entity_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub near: Option<String>,
}

/// What an analysis found. Strictness is already applied.
#[derive(Debug, Clone)]
pub struct AnalyzeOutcome {
    /// Built-in passes, extension passes in their declared order, `prove` last.
    pub passes: Vec<PassOutcome>,
    /// The `min` proof-coverage gate: part of the run's verdict.
    pub gate: Gate,
    /// Stray test records, outside the pass reports.
    pub stray_records: Vec<StrayRecord>,
}

/// Where the proof-coverage gate landed.
#[derive(Debug, Clone, PartialEq)]
pub enum Gate {
    /// `min` was not set.
    NotRequested,
    /// Proof coverage is at or above `min` (nothing testable always is).
    Met {
        pct: f64,
        min: f64,
        proven: usize,
        total: usize,
    },
    /// Proof coverage is under `min`: the run fails (E048).
    Below {
        pct: f64,
        min: f64,
        proven: usize,
        total: usize,
    },
    /// The coverage pass gave no figure the gate reads (it did not run, or
    /// its summary is not the shape this specforge reads); carries why. The
    /// run is unjudged (E068).
    Unjudged { min: f64, reason: String },
}

impl Gate {
    fn of(min: Option<f64>, passes: &[PassOutcome]) -> Gate {
        let Some(min) = min else {
            return Gate::NotRequested;
        };
        let Some(pass) = passes.iter().find(|r| r.name == COVERAGE_PASS) else {
            return Gate::Unjudged {
                min,
                reason: "the coverage pass did not run".to_string(),
            };
        };
        // A missing or renamed key is an error, never a silent 0.
        let summary: coverage::Summary = match serde_json::from_value(pass.summary.clone()) {
            Ok(summary) => summary,
            Err(e) => {
                return Gate::Unjudged {
                    min,
                    reason: format!("the summary is not the shape this specforge reads ({e})"),
                };
            }
        };
        let pct = summary.proof_pct();
        let (proven, total) = (summary.testable_proven, summary.testable_total);
        if pct + f64::EPSILON < min {
            Gate::Below {
                pct,
                min,
                proven,
                total,
            }
        } else {
            Gate::Met {
                pct,
                min,
                proven,
                total,
            }
        }
    }

    /// `{status, min, pct?, proven?, total?, reason?}`; `None` when no
    /// minimum was requested.
    fn document(&self) -> Option<GateDocument> {
        match self {
            Gate::NotRequested => None,
            Gate::Met {
                pct,
                min,
                proven,
                total,
            } => Some(GateDocument::Met {
                min: *min,
                pct: *pct,
                proven: *proven,
                total: *total,
            }),
            Gate::Below {
                pct,
                min,
                proven,
                total,
            } => Some(GateDocument::Below {
                min: *min,
                pct: *pct,
                proven: *proven,
                total: *total,
            }),
            Gate::Unjudged { min, reason } => Some(GateDocument::Unjudged {
                min: *min,
                reason: reason.clone(),
            }),
        }
    }
}

impl AnalyzeOutcome {
    /// No finding is an error, after strict promotion.
    pub fn findings_ok(&self) -> bool {
        self.passes
            .iter()
            .all(|p| p.findings.iter().all(|d| d.severity != Severity::Error))
    }

    /// The run's verdict: `Unjudged` when the gate has no figure, `Failed`
    /// when a finding is an error or the gate is below its minimum, else
    /// `Passed`. `specforge analyze` exits by it; `specforge.analyze`
    /// returns `ok` (`verdict() == Passed`) and the gate.
    pub fn verdict(&self) -> RunVerdict {
        match &self.gate {
            Gate::Unjudged { .. } => RunVerdict::Unjudged,
            Gate::Below { .. } => RunVerdict::Failed,
            Gate::NotRequested | Gate::Met { .. } => RunVerdict::of(self.findings_ok()),
        }
    }

    /// `verdict() == RunVerdict::Passed`.
    pub fn ok(&self) -> bool {
        self.verdict() == RunVerdict::Passed
    }

    /// Why the gate failed or could not judge the run, as every operation
    /// reports a failure (the CLI prints it; MCP returns the outcome): E048
    /// when below, E068 when unjudged; `None` otherwise.
    pub fn gate_failure(&self) -> Option<OpError> {
        match &self.gate {
            Gate::NotRequested | Gate::Met { .. } => None,
            // The kind is never mapped: MCP returns the outcome, not an
            // error, and the CLI prints the lines only (ADR 0029 D3a).
            Gate::Below {
                pct,
                min,
                proven,
                total,
            } => Some(OpError::coded(
                OpErrorKind::InvalidInput,
                codes::E048,
                format!(
                    "proof coverage {pct:.1}% is below the required minimum {min:.1}% ({proven}/{total} testable entities proven)"
                ),
            )),
            Gate::Unjudged { reason, .. } => Some(
                OpError::coded(
                    OpErrorKind::SchemaMismatch,
                    codes::E068,
                    format!("the coverage pass gave no proof-coverage figure: {reason}"),
                )
                .with_suggestion("update @specforge/testing, or run without a minimum"),
            ),
        }
    }

    /// The JSON document `{ok, passes: [{pass, findings, summary}]}`, plus
    /// `gate` when `min` was set and `stray_records` when there are any.
    pub fn document(&self) -> AnalyzeDocument {
        AnalyzeDocument {
            ok: self.ok(),
            passes: self
                .passes
                .iter()
                .map(|r| PassDocument {
                    pass: r.name.clone(),
                    findings: DiagnosticList(r.findings.clone()),
                    summary: r.summary.clone(),
                })
                .collect(),
            gate: self.gate.document(),
            stray_records: self.stray_records.clone(),
        }
    }
}

/// The document both surfaces answer with (`specforge analyze --format json`,
/// `specforge.analyze`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct AnalyzeDocument {
    /// The run's verdict is `Passed`.
    pub ok: bool,
    pub passes: Vec<PassDocument>,
    /// Where the `min` proof-coverage gate landed; absent without `min`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<GateDocument>,
    /// Stray test records; absent when there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stray_records: Vec<StrayRecord>,
}

/// One pass of an analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct PassDocument {
    pub pass: String,
    pub findings: DiagnosticList,
    /// What the pass summarizes: an open value, the pass's own.
    pub summary: Value,
}

/// Where the proof-coverage gate landed.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum GateDocument {
    /// Proof coverage is at or above `min`.
    Met {
        min: f64,
        pct: f64,
        proven: usize,
        total: usize,
    },
    /// Proof coverage is under `min`.
    Below {
        min: f64,
        pct: f64,
        proven: usize,
        total: usize,
    },
    /// The coverage pass gave no figure the gate reads.
    Unjudged { min: f64, reason: String },
}

/// The field of an analysis request a refusal can be about (ADR 0072).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyzeField {
    /// `AnalyzeOptions::pass`.
    Pass,
    /// `AnalyzeOptions::min`.
    Min,
    /// `AnalyzeOptions::report`, when it names a file.
    Report,
}

/// Why no analysis ran. Raised before any pass runs, in this order.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalyzeError {
    UnknownPass {
        requested: String,
        available: Vec<String>,
    },
    /// `min` is not a percentage (0 to 100).
    MinOutOfRange(f64),
    /// `min` was set and the coverage pass will not run (not selected, its
    /// extension not loaded, or no runtime): E068.
    MinWithoutCoveragePass,
    /// The recorded test report is there but cannot be used.
    UnusableReport(OpError),
    /// The test results file the request named cannot be used.
    UnusableTestResults(OpError),
    /// `min` was set and there is no report to score.
    MinNeedsTestResults,
}

impl std::fmt::Display for AnalyzeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalyzeError::UnknownPass {
                requested,
                available,
            } => write!(
                f,
                "Unknown analysis pass '{requested}' (available: {})",
                available.join(", ")
            ),
            AnalyzeError::MinOutOfRange(min) => write!(
                f,
                "min must be a percentage between 0 and 100, got {min}"
            ),
            AnalyzeError::MinWithoutCoveragePass => f.write_str(
                "a proof-coverage minimum needs the coverage pass of @specforge/testing (pass coverage or all)",
            ),
            AnalyzeError::UnusableReport(e) | AnalyzeError::UnusableTestResults(e) => e.fmt(f),
            AnalyzeError::MinNeedsTestResults => f.write_str(
                "--min needs test results: run `specforge collect` or pass --test-results",
            ),
        }
    }
}

impl std::error::Error for AnalyzeError {}

/// What each way of not running is, for every surface: an unknown pass is
/// `invalid_input` (`unknown_pass`, with a did-you-mean when one is close),
/// a minimum outside 0 to 100 is `invalid_input` (`invalid_min`), a minimum
/// without the coverage pass is the classified E068, an unusable report is
/// the classified E045, `--min` without a report is `precondition_failed`
/// (`no_test_results`).
impl From<AnalyzeError> for OpError {
    fn from(error: AnalyzeError) -> Self {
        Refusal::<AnalyzeField>::from(error).into_error()
    }
}

/// [`OpError::from`], with the field the variant is about: an unknown pass is about `pass`, a
/// minimum out of range about `min`, and a named results file that is not there about the
/// report (by the kind rule an unusable one is a refusal of the file, not of the argument).
impl From<AnalyzeError> for Refusal<AnalyzeField> {
    fn from(error: AnalyzeError) -> Self {
        let message = error.to_string();
        match error {
            AnalyzeError::UnknownPass {
                requested,
                available,
            } => {
                let close = specforge_common::suggest::find_close_match(
                    &requested,
                    available.iter().map(String::as_str),
                );
                let error = OpError::new(OpErrorKind::InvalidInput, UNKNOWN_PASS, message);
                match close {
                    Some(close) => error.with_suggestion(format!("did you mean '{close}'?")),
                    None => error,
                }
                .about(AnalyzeField::Pass)
            }
            AnalyzeError::MinOutOfRange(_) => {
                OpError::new(OpErrorKind::InvalidInput, INVALID_MIN, message)
                    .about(AnalyzeField::Min)
            }
            AnalyzeError::MinWithoutCoveragePass => {
                OpError::coded(OpErrorKind::PreconditionFailed, codes::E068, message)
                    .with_suggestion("enable it with `specforge add @specforge/testing`")
                    .into()
            }
            AnalyzeError::UnusableReport(error) => error.into(),
            AnalyzeError::UnusableTestResults(error) => error.about(AnalyzeField::Report),
            AnalyzeError::MinNeedsTestResults => {
                OpError::new(OpErrorKind::PreconditionFailed, "no_test_results", message).into()
            }
        }
    }
}

/// Run the selected passes over `view`; the extension passes in the view's
/// runtime, when it has one and a root. With `options.prove`, the prove step
/// first, over that solver.
pub fn analyze(
    view: &ProjectView,
    options: &AnalyzeOptions<'_>,
) -> Result<AnalyzeOutcome, AnalyzeError> {
    let selection = select(view, &options.pass)?;
    if let Some(min) = options.min {
        if !(0.0..=100.0).contains(&min) {
            return Err(AnalyzeError::MinOutOfRange(min));
        }
        let coverage_runs = selection.runs(COVERAGE_PASS)
            && declared_pass_names(view).iter().any(|n| n == COVERAGE_PASS)
            && view.runtime().is_some()
            && view.root().is_some();
        if !coverage_runs {
            return Err(AnalyzeError::MinWithoutCoveragePass);
        }
    }
    let report = read_report(view, &options.report)?;
    if options.min.is_some() && report.is_none() {
        return Err(AnalyzeError::MinNeedsTestResults);
    }

    let stray_records = stray_records(view.graph(), report.as_deref());

    let registries = view.registries();
    let base = AnalysisContext {
        graph: view.graph(),
        kind_registry: registries.kinds(),
        field_registry: registries.fields(),
        entities: view.entities(),
        test_results: report.as_deref(),
        proved_claims: None,
    };

    // The proof pass runs first: its entailment verdicts feed the coverage
    // discharge funnel (a proved claim discharges `verify property`
    // obligations without executable tests).
    let proved = options.prove.map(|solver| crate::prove::run(&base, solver));
    let proved_claims: std::collections::HashSet<String> = proved
        .as_ref()
        .map(|r| r.proved_claim_ids.iter().cloned().collect())
        .unwrap_or_default();
    let input = AnalysisContext {
        proved_claims: proved.as_ref().map(|_| &proved_claims),
        ..base
    };

    let mut passes_run: Vec<PassOutcome> = Vec::new();
    for name in &selection.builtins {
        if let Some(r) = crate::builtin_passes::run_pass(&input, name) {
            passes_run.push(PassOutcome {
                name: r.name.to_string(),
                description: r.description.to_string(),
                findings: r.findings,
                summary: r.summary,
            });
        }
    }
    if view.root().is_some()
        && let Some(runtime) = view.runtime()
    {
        // Declared `after` constraints order a single extension's passes;
        // across extensions they are advisory.
        passes_run.extend(
            passes::run_extension_passes(
                registries
                    .analyze_passes()
                    .filter(|p| selection.runs(&p.full_name())),
                &input,
                runtime.as_ref(),
            )
            .into_iter()
            .map(|r| PassOutcome {
                name: r.name,
                description: "extension compiler pass".to_string(),
                findings: r.findings,
                summary: r.summary,
            }),
        );
    }
    if let Some(r) = proved {
        passes_run.push(PassOutcome {
            name: "prove".to_string(),
            description: "declared bounds and claims verified with an SMT solver".to_string(),
            findings: r.findings,
            summary: r.summary,
        });
    }

    // Strictness, once, over every report; the verdict reads the promoted
    // findings.
    let policy = specforge_project::DiagnosticPolicy::strict(options.strict);
    for r in &mut passes_run {
        policy.promote(&mut r.findings);
    }
    let gate = Gate::of(options.min, &passes_run);
    Ok(AnalyzeOutcome {
        passes: passes_run,
        gate,
        stray_records,
    })
}

/// Report entries for entities the graph does not know. Matching is exact;
/// a close match is only a hint.
fn stray_records(graph: &Graph, report: Option<&TestReport>) -> Vec<StrayRecord> {
    let Some(report) = report else {
        return Vec::new();
    };
    report
        .results
        .keys()
        .filter(|id| graph.node(id).is_none())
        .map(|id| StrayRecord {
            entity_id: id.clone(),
            near: specforge_common::suggest::find_close_match(
                id,
                graph.nodes().iter().map(|n| n.id.raw.as_str()),
            )
            .map(str::to_string),
        })
        .collect()
}

/// The extension passes a selection runs.
enum ExtensionPasses {
    /// Every analyze-phase pass the extensions declare.
    Every,
    /// The one pass of this full name (`<extension>:<pass>`).
    One(String),
    /// None: a builtin pass was asked for.
    None,
}

struct Selection {
    builtins: Vec<&'static str>,
    extensions: ExtensionPasses,
}

impl Selection {
    /// Whether the extension pass `full_name` is selected.
    fn runs(&self, full_name: &str) -> bool {
        match &self.extensions {
            ExtensionPasses::Every => true,
            ExtensionPasses::One(name) => name == full_name,
            ExtensionPasses::None => false,
        }
    }
}

fn select(view: &ProjectView, requested: &str) -> Result<Selection, AnalyzeError> {
    if requested == EVERY_PASS {
        return Ok(Selection {
            builtins: PASS_NAMES.to_vec(),
            extensions: ExtensionPasses::Every,
        });
    }
    // Coverage is an extension pass (ADR 0002).
    if requested == "coverage" {
        return Ok(Selection {
            builtins: Vec::new(),
            extensions: ExtensionPasses::One(COVERAGE_PASS.to_string()),
        });
    }
    if let Some(name) = PASS_NAMES.iter().find(|n| **n == requested) {
        return Ok(Selection {
            builtins: vec![name],
            extensions: ExtensionPasses::None,
        });
    }
    let declared = declared_pass_names(view);
    if declared.iter().any(|n| n == requested) {
        return Ok(Selection {
            builtins: Vec::new(),
            extensions: ExtensionPasses::One(requested.to_string()),
        });
    }
    let mut available: Vec<String> = [EVERY_PASS, "coverage"].map(String::from).to_vec();
    available.extend(PASS_NAMES.iter().map(|n| n.to_string()));
    available.extend(declared);
    Err(AnalyzeError::UnknownPass {
        requested: requested.to_string(),
        available,
    })
}

/// `<extension>:<pass>` of every analyze-phase pass the extensions declare.
fn declared_pass_names(view: &ProjectView) -> Vec<String> {
    if view.root().is_none() {
        return Vec::new();
    }
    view.registries()
        .analyze_passes()
        .map(DeclaredPass::full_name)
        .collect()
}

fn read_report(
    view: &ProjectView,
    source: &ReportSource,
) -> Result<Option<Arc<TestReport>>, AnalyzeError> {
    match source {
        ReportSource::None => Ok(None),
        ReportSource::File(path) => crate::report::named(path)
            .map(Some)
            .map_err(AnalyzeError::UnusableTestResults),
        ReportSource::Recorded => view.test_report().map_err(AnalyzeError::UnusableReport),
    }
}

#[cfg(test)]
mod tests {
    /// The document as JSON.
    fn json_of(outcome: &AnalyzeOutcome) -> serde_json::Value {
        serde_json::to_value(outcome.document()).expect("a document serializes")
    }

    use super::*;
    use crate::testing::ScriptedSolver;
    use serde_json::{Value, json};
    use specforge_extension_sdk::prelude::*;
    use specforge_test_macros::test as specforge_test;
    use specforge_wasm::runtime::{WasmCallResult, WasmTrapInfo};
    use specforge_wasm::testing::InProcessRuntime;

    const EXT: &str = "@t/x";
    const TESTING: &str = "@specforge/testing";

    /// `@t/x`, declaring `scan` (warns W900) and `hidden` (check phase);
    /// the runtime records the input every pass receives.
    fn scanning() -> ContributionsBuilder {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "1.0.0"));
        c.pass("scan", |p| {
            p.run(|_: &PassInput| vec![PassDiagnostic::warning("W900", "scanned")]);
        });
        c.pass("hidden", |p| {
            p.phase("check")
                .run(|_: &PassInput| Vec::<PassDiagnostic>::new());
        });
        c
    }

    /// The `proved_claims` each `scan` call received (`null` when absent).
    fn proved_seen(runtime: &InProcessRuntime) -> Vec<Value> {
        runtime
            .calls()
            .into_iter()
            .filter(|c| c.export == "__pass_scan")
            .map(|c| c.input.get("proved_claims").cloned().unwrap_or(Value::Null))
            .collect()
    }

    struct Project {
        graph: Graph,
        env: specforge_project::Environment,
        recorded: std::sync::OnceLock<coverage::RecordedCoverage>,
        dir: tempfile::TempDir,
        /// The runtime the environment loaded its extensions in: what it
        /// records is what a test then runs.
        runtime: std::sync::Arc<InProcessRuntime>,
    }

    impl Project {
        /// A project at a temp root whose `specforge.json` enables
        /// `entries`, each loaded from `runtime` through the production
        /// load (a named entry that is no builtin installed first, ADR 0028
        /// D3), with an empty graph. The runtime's calls are cleared after
        /// the load: what it records is what the test then runs.
        fn serving(runtime: InProcessRuntime, entries: &[&str]) -> Self {
            let dir = tempfile::TempDir::new().unwrap();
            std::fs::write(
                dir.path().join("specforge.json"),
                json!({ "extensions": entries }).to_string(),
            )
            .unwrap();
            let entries: Vec<String> = entries.iter().map(|e| e.to_string()).collect();
            specforge_installed::testing::install_enabled(
                dir.path(),
                &entries,
                &specforge_project::builtins(),
            );
            let runtime = std::sync::Arc::new(runtime);
            let env = specforge_project::Environment::load(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                Some(runtime.clone()),
            );
            assert!(
                env.diagnostics().all(|d| d.code != "E028"),
                "{:?}",
                env.diagnostics().collect::<Vec<_>>()
            );
            runtime.clear_calls();
            Self {
                graph: Graph::new(),
                env,
                recorded: std::sync::OnceLock::new(),
                dir,
                runtime,
            }
        }

        /// `@t/x` alone, as most tests serve it.
        fn new() -> Self {
            Self::serving(InProcessRuntime::new().with(scanning), &[EXT])
        }

        fn view(&self) -> ProjectView<'_> {
            self.view_at(Some(self.dir.path()))
        }

        /// The project's view rooted at `root`: what a view reads at its
        /// root, never in an ancestor.
        fn view_at<'a>(&'a self, root: Option<&'a std::path::Path>) -> ProjectView<'a> {
            ProjectView::new(
                &self.graph,
                &self.env,
                root,
                self.recorded
                    .get_or_init(|| coverage::RecordedCoverage::over(&self.graph, &self.env)),
            )
        }

        fn run(&self, options: &AnalyzeOptions) -> Result<AnalyzeOutcome, AnalyzeError> {
            analyze(&self.view(), options)
        }
    }

    fn names(outcome: &AnalyzeOutcome) -> Vec<&str> {
        outcome.passes.iter().map(|p| p.name.as_str()).collect()
    }

    fn pass(name: &str) -> AnalyzeOptions<'static> {
        AnalyzeOptions {
            pass: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn default_options_run_every_pass_in_order() {
        let outcome = Project::new().run(&AnalyzeOptions::default()).unwrap();
        assert_eq!(names(&outcome), vec!["contracts", "@t/x:scan"]);
        assert!(outcome.ok(), "a warning is not an error");
    }

    #[test]
    fn strict_promotes_warnings_and_ok_follows() {
        let outcome = Project::new()
            .run(&AnalyzeOptions {
                strict: true,
                ..Default::default()
            })
            .unwrap();
        assert!(!outcome.ok());
        assert_eq!(outcome.passes[1].findings[0].severity, Severity::Error);
    }

    #[test]
    fn json_shape_is_ok_and_passes() {
        let outcome = Project::new().run(&pass("contracts")).unwrap();
        assert_eq!(
            json_of(&outcome),
            json!({
                "ok": true,
                "passes": [{
                    "pass": "contracts",
                    "findings": [],
                    "summary": {
                        "contract_entities": 0,
                        "unconstrained_entities": 0,
                        "obligation_references": 0
                    }
                }]
            })
        );
    }

    #[specforge_test(
        behavior = "call_extension_exports",
        verify = "an analyze pass that traps is reported as an E028 finding of that pass"
    )]
    fn an_analyze_pass_that_traps_is_an_e028_finding_and_the_analysis_fails() {
        for answer in [
            WasmCallResult::Trap(WasmTrapInfo {
                kind: "call_failed".into(),
                message: "unreachable: the pass panicked".into(),
                export_name: "__pass_scan".into(),
            }),
            WasmCallResult::Ok(b"not diagnostics".to_vec()),
        ] {
            let project = Project::serving(
                InProcessRuntime::new()
                    .with(scanning)
                    .answer_raw(EXT, "__pass_scan", answer),
                &[EXT],
            );
            let outcome = analyze(&project.view(), &AnalyzeOptions::default()).unwrap();
            assert!(!outcome.ok(), "a failed pass fails the analysis");
            let scan = outcome
                .passes
                .iter()
                .find(|p| p.name == "@t/x:scan")
                .expect("the failed pass has its report");
            assert_eq!(scan.findings.len(), 1, "{:?}", scan.findings);
            assert_eq!(scan.findings[0].code, "E028");
            assert_eq!(scan.findings[0].severity, Severity::Error);
            assert!(
                scan.findings[0]
                    .message
                    .starts_with("compiler pass __pass_scan() of '@t/x' "),
                "{}",
                scan.findings[0].message
            );
            assert_eq!(scan.summary["failed"], true);
        }
    }

    #[test]
    fn a_declared_extension_pass_runs_alone_by_its_full_name() {
        let outcome = Project::new().run(&pass("@t/x:scan")).unwrap();
        assert_eq!(names(&outcome), vec!["@t/x:scan"]);
    }

    /// The `__pass_*` exports the runtime was called for.
    fn pass_calls(runtime: &InProcessRuntime) -> Vec<String> {
        runtime
            .calls()
            .into_iter()
            .map(|c| c.export)
            .filter(|e| e.starts_with("__pass_"))
            .collect()
    }

    #[test]
    fn a_builtin_pass_runs_no_extension_pass() {
        let project = Project::new();
        let outcome = project.run(&pass("contracts")).unwrap();
        assert_eq!(names(&outcome), vec!["contracts"]);
        assert!(pass_calls(&project.runtime).is_empty());

        project.runtime.clear_calls();
        let outcome = project.run(&pass("@t/x:scan")).unwrap();
        assert_eq!(names(&outcome), vec!["@t/x:scan"]);
        assert_eq!(pass_calls(&project.runtime), ["__pass_scan"]);

        project.runtime.clear_calls();
        let outcome = project.run(&pass("all")).unwrap();
        assert_eq!(names(&outcome), vec!["contracts", "@t/x:scan"]);
        assert_eq!(pass_calls(&project.runtime), ["__pass_scan"]);
    }

    #[test]
    fn coverage_is_an_alias_for_the_testing_pass() {
        let outcome = Project::new().run(&pass("coverage")).unwrap();
        assert!(outcome.passes.is_empty(), "no testing extension declared");
    }

    #[test]
    fn unknown_and_check_phase_passes_are_refused_with_the_available_ones() {
        for requested in ["nope", "@t/x:hidden", "@t/x:typo"] {
            let err = Project::new().run(&pass(requested)).unwrap_err();
            assert_eq!(
                err,
                AnalyzeError::UnknownPass {
                    requested: requested.to_string(),
                    available: ["all", "coverage", "contracts", "@t/x:scan"]
                        .map(String::from)
                        .to_vec(),
                }
            );
        }
    }

    #[test]
    fn extension_passes_are_skipped_without_a_root() {
        let project = Project::new();
        let view = project.view_at(None);
        let outcome = analyze(&view, &AnalyzeOptions::default()).unwrap();
        assert_eq!(names(&outcome), vec!["contracts"]);
    }

    #[test]
    fn an_unusable_report_is_refused_as_e045() {
        let project = Project::new();
        std::fs::write(
            project.dir.path().join("specforge-report.json"),
            "{not json",
        )
        .unwrap();
        let AnalyzeError::UnusableReport(e) = project.run(&AnalyzeOptions::default()).unwrap_err()
        else {
            panic!("expected UnusableReport");
        };
        assert_eq!(e.code, "E045");

        let missing = AnalyzeOptions {
            report: ReportSource::File(project.dir.path().join("absent.json")),
            ..Default::default()
        };
        assert!(matches!(
            project.run(&missing).unwrap_err(),
            AnalyzeError::UnusableTestResults(_)
        ));
    }

    #[test]
    fn a_report_source_of_none_ignores_a_broken_recorded_report() {
        let project = Project::new();
        std::fs::write(
            project.dir.path().join("specforge-report.json"),
            "{not json",
        )
        .unwrap();
        let options = AnalyzeOptions {
            report: ReportSource::None,
            ..Default::default()
        };
        assert!(project.run(&options).is_ok());
    }

    #[specforge_test(
        behavior = "read_views_over_the_project_view",
        verify = "the recorded test report is read at the view's root, never an ancestor's"
    )]
    fn a_sub_path_does_not_read_the_ancestors_report() {
        let project = Project::serving(
            InProcessRuntime::new()
                .with(scanning)
                .with(coverage(tally(0, 0))),
            &[EXT, TESTING],
        );
        std::fs::write(
            project.dir.path().join("specforge-report.json"),
            r#"{"results":{}}"#,
        )
        .unwrap();
        let sub = project.dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let view = project.view_at(Some(&sub));
        let min = AnalyzeOptions {
            min: Some(50.0),
            ..Default::default()
        };
        // The view rooted at the sub-path reads its root and nothing above.
        assert_eq!(
            analyze(&view, &min).unwrap_err(),
            AnalyzeError::MinNeedsTestResults
        );
        assert!(analyze(&project.view(), &min).is_ok());
    }

    #[test]
    fn min_needs_a_report_and_comes_after_the_other_checks() {
        let project = Project::serving(
            InProcessRuntime::new()
                .with(scanning)
                .with(coverage(tally(0, 0))),
            &[EXT, TESTING],
        );
        let min = AnalyzeOptions {
            min: Some(50.0),
            ..Default::default()
        };
        assert_eq!(
            project.run(&min).unwrap_err(),
            AnalyzeError::MinNeedsTestResults
        );

        let both = AnalyzeOptions {
            pass: "nope".to_string(),
            ..min.clone()
        };
        assert!(matches!(
            project.run(&both).unwrap_err(),
            AnalyzeError::UnknownPass { .. }
        ));

        std::fs::write(
            project.dir.path().join("specforge-report.json"),
            r#"{"results":{}}"#,
        )
        .unwrap();
        assert!(project.run(&min).is_ok());
    }

    #[test]
    fn prove_runs_last_and_tells_the_passes_it_ran() {
        let project = Project::new();
        analyze(&project.view(), &AnalyzeOptions::default()).unwrap();
        assert_eq!(proved_seen(&project.runtime), vec![Value::Null]);

        project.runtime.clear_calls();
        let solver = ScriptedSolver::ready();
        let options = AnalyzeOptions {
            prove: Some(&solver),
            ..Default::default()
        };
        let outcome = analyze(&project.view(), &options).unwrap();
        assert_eq!(names(&outcome), vec!["contracts", "@t/x:scan", "prove"]);
        assert_eq!(proved_seen(&project.runtime), vec![json!([])]);
    }

    /// A `@specforge/testing` whose `coverage` pass answers `summary`.
    fn coverage(summary: Value) -> impl Fn() -> ContributionsBuilder + Send + Sync + 'static {
        move || {
            let summary = summary.clone();
            let mut c = ContributionsBuilder::new(ExtensionMeta::new(TESTING, "1.0.0"));
            c.pass("coverage", |p| {
                p.run(move |_: &PassInput| PassOutput {
                    diagnostics: Vec::new(),
                    summary: summary.as_object().cloned().unwrap_or_default(),
                });
            });
            c
        }
    }

    /// A project with a (blank) recorded report, gated at `min`, whose
    /// testing extension reports `proven` of `total`.
    fn gate_of(pass_name: &str, min: Option<f64>, summary: Value) -> Gate {
        let project = Project::serving(InProcessRuntime::new().with(coverage(summary)), &[TESTING]);
        std::fs::write(
            project.dir.path().join("specforge-report.json"),
            r#"{"results":{}}"#,
        )
        .unwrap();
        let options = AnalyzeOptions {
            pass: pass_name.to_string(),
            min,
            ..Default::default()
        };
        analyze(&project.view(), &options).unwrap().gate
    }

    fn tally(proven: usize, total: usize) -> Value {
        serde_json::to_value(coverage::Summary {
            testable_proven: proven,
            testable_total: total,
            ..Default::default()
        })
        .unwrap()
    }

    #[specforge_test(
        behavior = "te_coverage_gate",
        verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
    )]
    fn the_gate_is_not_requested_without_min() {
        assert_eq!(gate_of("all", None, tally(0, 4)), Gate::NotRequested);
    }

    #[specforge_test(
        behavior = "te_coverage_gate",
        verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
    )]
    fn the_gate_is_met_at_or_above_the_minimum() {
        assert_eq!(
            gate_of("all", Some(50.0), tally(2, 4)),
            Gate::Met {
                pct: 50.0,
                min: 50.0,
                proven: 2,
                total: 4
            }
        );
        assert_eq!(
            gate_of("coverage", Some(50.0), tally(3, 4)),
            Gate::Met {
                pct: 75.0,
                min: 50.0,
                proven: 3,
                total: 4
            }
        );
    }

    #[specforge_test(
        behavior = "te_coverage_gate",
        verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
    )]
    fn nothing_testable_satisfies_any_minimum() {
        assert!(matches!(
            gate_of("all", Some(100.0), tally(0, 0)),
            Gate::Met { .. }
        ));
    }

    #[specforge_test(
        behavior = "te_coverage_gate",
        verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
    )]
    fn the_gate_is_below_with_the_numbers_to_print() {
        assert_eq!(
            gate_of("all", Some(75.0), tally(1, 4)),
            Gate::Below {
                pct: 25.0,
                min: 75.0,
                proven: 1,
                total: 4
            }
        );
    }

    #[specforge_test(
        behavior = "te_coverage_gate",
        verify = "the analysis's ok is the run verdict, the gate included, and its JSON says where the gate landed"
    )]
    fn the_verdict_folds_the_gate_in() {
        let outcome = |gate: Gate| AnalyzeOutcome {
            passes: Vec::new(),
            gate,
            stray_records: Vec::new(),
        };
        let landed = |pct: f64| (pct, 50.0, 1, 4);
        let (pct, min, proven, total) = landed(25.0);
        let below = outcome(Gate::Below {
            pct,
            min,
            proven,
            total,
        });
        assert!(below.findings_ok(), "the findings are fine");
        assert_eq!(below.verdict(), RunVerdict::Failed);
        assert!(!below.ok());
        assert_eq!(json_of(&below)["ok"], false);
        assert_eq!(below.gate_failure().unwrap().code, "E048");

        let unjudged = outcome(Gate::Unjudged {
            min: 50.0,
            reason: "no figure".into(),
        });
        assert_eq!(unjudged.verdict(), RunVerdict::Unjudged);
        assert_eq!(json_of(&unjudged)["gate"]["status"], "unjudged");
        assert_eq!(unjudged.gate_failure().unwrap().code, "E068");

        let met = outcome(Gate::Met {
            pct: 80.0,
            min: 50.0,
            proven: 4,
            total: 5,
        });
        assert_eq!(met.verdict(), RunVerdict::Passed);
        assert!(met.gate_failure().is_none());
        assert!(json_of(&outcome(Gate::NotRequested)).get("gate").is_none());
    }

    #[specforge_test(
        behavior = "te_coverage_gate",
        verify = "a gate without the coverage pass exits 2 with E068"
    )]
    fn a_minimum_without_the_coverage_pass_is_refused_before_any_pass_runs() {
        let project = Project::serving(
            InProcessRuntime::new()
                .with(scanning)
                .with(coverage(tally(0, 0))),
            &[TESTING, EXT],
        );
        std::fs::write(
            project.dir.path().join("specforge-report.json"),
            r#"{"results":{}}"#,
        )
        .unwrap();
        let options = AnalyzeOptions {
            min: Some(10.0),
            ..pass("contracts")
        };
        assert_eq!(
            analyze(&project.view(), &options).unwrap_err(),
            AnalyzeError::MinWithoutCoveragePass
        );
        assert!(project.runtime.calls().is_empty(), "no pass ran");

        // Not declared at all: the same refusal.
        let project = Project::new();
        let all = AnalyzeOptions {
            min: Some(10.0),
            ..Default::default()
        };
        assert_eq!(
            analyze(&project.view(), &all).unwrap_err(),
            AnalyzeError::MinWithoutCoveragePass
        );
        let error = OpError::from(AnalyzeError::MinWithoutCoveragePass);
        assert_eq!(error.code, "E068");
    }

    #[specforge_test(
        behavior = "te_coverage_gate",
        verify = "a minimum that is not a percentage is invalid input"
    )]
    fn a_minimum_outside_0_to_100_is_invalid_input() {
        let project = Project::new();
        for min in [-1.0, 100.5, f64::NAN] {
            let options = AnalyzeOptions {
                min: Some(min),
                ..Default::default()
            };
            let error = project.run(&options).unwrap_err();
            assert!(matches!(error, AnalyzeError::MinOutOfRange(_)), "{min}");
            let error = OpError::from(error);
            assert_eq!(error.kind, OpErrorKind::InvalidInput);
            assert_eq!(error.code, INVALID_MIN);
        }
    }

    #[specforge_test(
        behavior = "te_coverage_gate",
        verify = "a gate without a readable coverage pass is not met"
    )]
    fn a_summary_of_another_shape_is_unreadable_not_zero() {
        assert_eq!(
            gate_of("all", Some(10.0), json!({"testable_total": "many"})),
            Gate::Unjudged {
                min: 10.0,
                reason: "the summary is not the shape this specforge reads (invalid type: string \"many\", expected usize)".into()
            }
        );
    }

    #[test]
    fn prove_without_a_solver_is_a_last_w098_report_with_empty_proved_claims() {
        let project = Project::new();
        let missing = ScriptedSolver::not_found();
        let mut options = AnalyzeOptions {
            prove: Some(&missing),
            ..Default::default()
        };
        let lenient = analyze(&project.view(), &options).unwrap();
        assert_eq!(names(&lenient).last().copied(), Some("prove"));
        let report = lenient.passes.last().unwrap();
        assert!(report.findings.iter().any(|f| f.code == "W098"));
        assert!(lenient.ok());
        assert_eq!(proved_seen(&project.runtime), vec![json!([])]);

        options.strict = true;
        let strict = analyze(&project.view(), &options).unwrap();
        assert!(!strict.ok(), "strict promotes the prove report too");
    }

    /// `@t/x` as [`scanning`] declares it, plus a `claim` kind whose `holds`
    /// field is a claim.
    fn claiming() -> ContributionsBuilder {
        let mut c = scanning();
        c.kind("Claim", |k| {
            k.keyword("claim");
            k.field("holds", |f| {
                f.field_type(FieldType::String).proof_role("claim");
            });
        });
        c
    }

    #[specforge_test(
        behavior = "prove_declared_bounds_and_claims",
        verify = "a claim the bounds entail reaches the extension passes as a proved claim"
    )]
    fn a_claim_the_solver_entails_reaches_the_extension_passes() {
        let mut project = Project::serving(InProcessRuntime::new().with(claiming), &[EXT]);
        add_entity_with(
            &mut project,
            "fast",
            "claim",
            [(
                "holds",
                specforge_parser::FieldValue::String("latency <= latency".into()),
            )],
        );
        let solver = ScriptedSolver::answering([Ok("unsat\n".to_string())]);
        let outcome = analyze(
            &project.view(),
            &AnalyzeOptions {
                prove: Some(&solver),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(names(&outcome).last().copied(), Some("prove"));
        assert_eq!(proved_seen(&project.runtime), vec![json!(["fast"])]);
        assert_eq!(
            solver.scripts().len(),
            1,
            "no bounds: one entailment check, no consistency check"
        );
    }

    fn write_report(project: &Project, ids: &[&str]) {
        let results: serde_json::Map<String, Value> = ids
            .iter()
            .map(|id| {
                (
                    id.to_string(),
                    json!({"tests": [{"name": "t", "status": "passed"}]}),
                )
            })
            .collect();
        std::fs::write(
            project.dir.path().join("specforge-report.json"),
            json!({"runner": "r", "results": results}).to_string(),
        )
        .unwrap();
    }

    fn add_entity(project: &mut Project, id: &str) {
        add_entity_with(project, id, "behavior", []);
    }

    /// A node of `kind` carrying `fields`.
    fn add_entity_with(
        project: &mut Project,
        id: &str,
        kind: &str,
        fields: impl IntoIterator<Item = (&'static str, specforge_parser::FieldValue)>,
    ) {
        use specforge_common::{SourceSpan, Sym};
        use specforge_parser::{EntityId, EntityKind, FieldMap};
        let mut map = FieldMap::new();
        for (name, value) in fields {
            map.push(Sym::new(name), value);
        }
        project.graph.add_node(specforge_graph::Node {
            id: EntityId { raw: Sym::new(id) },
            kind: EntityKind {
                raw: Sym::new(kind),
            },
            title: None,
            fields: map,
            source_span: SourceSpan {
                file: Sym::new("t.spec"),
                start_line: 1,
                start_col: 1,
                end_line: 1,
                end_col: 1,
            },
            methods: Vec::new(),
        });
    }

    #[test]
    fn a_record_for_an_unknown_entity_is_stray_with_a_close_match() {
        let mut project = Project::new();
        add_entity(&mut project, "widget");
        write_report(&project, &["widget", "wodget", "zzzzzzzz"]);
        let outcome = project.run(&pass("contracts")).unwrap();
        assert_eq!(
            outcome.stray_records,
            vec![
                StrayRecord {
                    entity_id: "wodget".to_string(),
                    near: Some("widget".to_string()),
                },
                StrayRecord {
                    entity_id: "zzzzzzzz".to_string(),
                    near: None,
                },
            ]
        );
        assert_eq!(
            json_of(&outcome)["stray_records"],
            json!([
                {"entity_id": "wodget", "near": "widget"},
                {"entity_id": "zzzzzzzz"}
            ])
        );
    }

    #[test]
    fn stray_records_are_never_promoted_by_strict_and_change_neither_ok_nor_the_list() {
        let mut project = Project::new();
        add_entity(&mut project, "widget");
        write_report(&project, &["wodget"]);
        let lax = project.run(&pass("contracts")).unwrap();
        let strict = project
            .run(&AnalyzeOptions {
                strict: true,
                ..pass("contracts")
            })
            .unwrap();
        assert!(lax.ok() && strict.ok());
        assert_eq!(lax.stray_records, strict.stray_records);
        assert_eq!(strict.stray_records.len(), 1);
        assert!(strict.passes.iter().all(|p| p.findings.is_empty()));
    }

    #[test]
    fn the_stray_records_key_is_absent_when_there_are_none() {
        let mut project = Project::new();
        add_entity(&mut project, "widget");
        write_report(&project, &["widget"]);
        let outcome = project.run(&pass("contracts")).unwrap();
        assert!(outcome.stray_records.is_empty());
        assert!(json_of(&outcome).get("stray_records").is_none());

        let none = Project::new().run(&pass("contracts")).unwrap();
        assert!(json_of(&none).get("stray_records").is_none());
    }

    #[specforge_test(
        invariant = "mcp_structured_error_responses",
        verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
    )]
    fn analyze_states_which_field_a_refusal_is_about() {
        let project = Project::new();
        let subject = |options: AnalyzeOptions| {
            Refusal::<AnalyzeField>::from(project.run(&options).unwrap_err()).subject
        };
        assert_eq!(subject(pass("nope")), Some(AnalyzeField::Pass));
        let min = AnalyzeOptions {
            min: Some(150.0),
            ..Default::default()
        };
        assert_eq!(subject(min), Some(AnalyzeField::Min));
        // A named results file that is not there is about the report; one that is unusable is a
        // refusal of the file, and so is an unusable recorded report.
        let report = |path: std::path::PathBuf| AnalyzeOptions {
            report: ReportSource::File(path),
            ..Default::default()
        };
        let missing = project.dir.path().join("missing.json");
        assert_eq!(subject(report(missing)), Some(AnalyzeField::Report));
        let unusable = project.dir.path().join("unusable.json");
        std::fs::write(&unusable, "{not json").unwrap();
        assert_eq!(subject(report(unusable)), None);
        std::fs::write(
            project.dir.path().join("specforge-report.json"),
            "{not json",
        )
        .unwrap();
        assert_eq!(subject(AnalyzeOptions::default()), None);
        // A refusal of the project is about no field.
        let no_report = AnalyzeOptions {
            min: Some(50.0),
            report: ReportSource::None,
            ..Default::default()
        };
        let refusal = Refusal::<AnalyzeField>::from(project.run(&no_report).unwrap_err());
        assert_eq!(refusal.subject, None, "{refusal:?}");
    }
}
