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

use crate::analysis::builtin_passes::{COVERAGE_PASS, PASS_NAMES};
use specforge_common::{Diagnostic, DiagnosticList, Severity, codes};
use specforge_graph::Graph;
use specforge_project::coverage;
use specforge_project::coverage::TestReport;
use specforge_project::passes::{self, PassContext};
use specforge_registry::DeclaredPass;

use crate::Containment;
use crate::solver::Solver;
use crate::{OpError, OpErrorKind, Refusal, RunVerdict};

use crate::view::ProjectView;

/// Where the test report comes from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ReportSource {
    /// The view's recorded report: what `specforge collect` last wrote at
    /// the root the project was compiled from, if anything (never an
    /// ancestor's: a sub-path does not inherit its parent's report).
    #[default]
    Recorded,
    /// A named report file, which must exist. It is read through the bound it came with:
    /// one that lands outside is refused before it is read (ADR 0112 D2).
    Named {
        path: PathBuf,
        containment: Containment,
    },
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
/// A minimum with no test results to score (`PreconditionFailed`).
pub const NO_TEST_RESULTS: &str = "no_test_results";

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
    /// The `min` proof-coverage gate, `None` when `min` was not set: part of
    /// the run's verdict.
    pub gate: Option<Gate>,
    /// Stray test records, outside the pass reports.
    pub stray_records: Vec<StrayRecord>,
}

/// Where the proof-coverage gate landed, when `min` was set: part of the
/// run's verdict, and the document's `gate` (`McpAnalyzeGate`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Gate {
    /// Proof coverage is at or above `min` (nothing testable always is).
    Met {
        min: f64,
        pct: f64,
        proven: usize,
        total: usize,
    },
    /// Proof coverage is under `min`: the run fails (E048).
    Below {
        min: f64,
        pct: f64,
        proven: usize,
        total: usize,
    },
    /// The coverage pass gave no figure the gate reads (it did not run, or
    /// its summary is not the shape this specforge reads); carries why. The
    /// run is unjudged (E068).
    Unjudged { min: f64, reason: String },
}

impl Gate {
    /// The gate at `min` over `passes`; `None` without a minimum.
    fn of(min: Option<f64>, passes: &[PassOutcome]) -> Option<Gate> {
        let min = min?;
        let Some(pass) = passes.iter().find(|r| r.name == COVERAGE_PASS) else {
            return Some(Gate::Unjudged {
                min,
                reason: "the coverage pass did not run".to_string(),
            });
        };
        // A missing or renamed key is an error, never a silent 0.
        let summary: coverage::Summary = match serde_json::from_value(pass.summary.clone()) {
            Ok(summary) => summary,
            Err(e) => {
                return Some(Gate::Unjudged {
                    min,
                    reason: format!("the summary is not the shape this specforge reads ({e})"),
                });
            }
        };
        let pct = summary.proof_pct();
        let (proven, total) = (summary.testable_proven, summary.testable_total);
        Some(if pct + f64::EPSILON < min {
            Gate::Below {
                min,
                pct,
                proven,
                total,
            }
        } else {
            Gate::Met {
                min,
                pct,
                proven,
                total,
            }
        })
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
            Some(Gate::Unjudged { .. }) => RunVerdict::Unjudged,
            Some(Gate::Below { .. }) => RunVerdict::Failed,
            None | Some(Gate::Met { .. }) => RunVerdict::of(self.findings_ok()),
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
            None | Some(Gate::Met { .. }) => None,
            // The kind is never mapped: MCP returns the outcome, not an
            // error, and the CLI prints the lines only (ADR 0029 D3a).
            Some(Gate::Below {
                pct,
                min,
                proven,
                total,
            }) => Some(OpError::coded(
                OpErrorKind::InvalidInput,
                codes::E048,
                format!(
                    "proof coverage {pct:.1}% is below the required minimum {min:.1}% ({proven}/{total} testable entities proven)"
                ),
            )),
            Some(Gate::Unjudged { reason, .. }) => Some(
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
            gate: self.gate.clone(),
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
    pub gate: Option<Gate>,
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

/// Run the selected passes over `view`; the extension passes in the view's
/// runtime, when it has one. With `options.prove`, the prove step
/// first, over that solver.
///
/// Refused before any pass runs, in this order: an unknown pass
/// ([`UNKNOWN_PASS`], about `pass`), a minimum outside 0 to 100
/// ([`INVALID_MIN`], about `min`), a minimum the coverage pass will not answer
/// (E068), a report that cannot be used (E045; a named file that is not there
/// is about `report`), a minimum with no report ([`NO_TEST_RESULTS`]).
pub fn analyze(
    view: &ProjectView,
    options: &AnalyzeOptions<'_>,
) -> Result<AnalyzeOutcome, Refusal<AnalyzeField>> {
    let selection = select(view, &options.pass)?;
    if let Some(min) = options.min {
        if !(0.0..=100.0).contains(&min) {
            return Err(OpError::new(
                OpErrorKind::InvalidInput,
                INVALID_MIN,
                format!("min must be a percentage between 0 and 100, got {min}"),
            )
            .about(AnalyzeField::Min));
        }
        let coverage_runs = selection.runs(COVERAGE_PASS)
            && declared_pass_names(view).iter().any(|n| n == COVERAGE_PASS)
            && view.runtime().is_some();
        if !coverage_runs {
            return Err(OpError::coded(
                OpErrorKind::PreconditionFailed,
                codes::E068,
                "a proof-coverage minimum needs the coverage pass of @specforge/testing (pass coverage or all)",
            )
            .with_suggestion("enable it with `specforge add @specforge/testing`")
            .into());
        }
    }
    let report = read_report(view, &options.report)?;
    if options.min.is_some() && report.is_none() {
        return Err(OpError::new(
            OpErrorKind::PreconditionFailed,
            NO_TEST_RESULTS,
            "--min needs test results: run `specforge collect` or pass --test-results",
        )
        .into());
    }

    let stray_records = stray_records(view.graph(), report.as_deref());

    let graph = view.graph();
    let registries = view.registries();

    // The proof pass runs first: its entailment verdicts feed the coverage
    // discharge funnel (a proved claim discharges `verify property`
    // obligations without executable tests).
    let proved = options
        .prove
        .map(|solver| crate::analysis::prove::run(graph, registries.fields(), solver));
    let proved_claims: std::collections::HashSet<String> = proved
        .as_ref()
        .map(|r| r.proved_claim_ids.iter().cloned().collect())
        .unwrap_or_default();

    let mut passes_run: Vec<PassOutcome> = Vec::new();
    for name in &selection.builtins {
        if let Some(r) = crate::analysis::builtin_passes::run_pass(graph, registries, name) {
            passes_run.push(PassOutcome {
                name: r.name.to_string(),
                description: r.description.to_string(),
                findings: r.findings,
                summary: r.summary,
            });
        }
    }
    if let Some(runtime) = view.runtime() {
        let context = PassContext {
            test_results: report.as_deref(),
            proved_claims: proved.as_ref().map(|_| &proved_claims),
            ..PassContext::of(view.entities())
        };
        // Declared `after` constraints order a single extension's passes;
        // across extensions they are advisory.
        passes_run.extend(
            passes::run_extension_passes(
                registries
                    .analyze_passes()
                    .filter(|p| selection.runs(&p.full_name())),
                &context,
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

fn select(view: &ProjectView, requested: &str) -> Result<Selection, Refusal<AnalyzeField>> {
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
    let error = OpError::new(
        OpErrorKind::InvalidInput,
        UNKNOWN_PASS,
        format!(
            "Unknown analysis pass '{requested}' (available: {})",
            available.join(", ")
        ),
    );
    // The name as given: a pass name is matched exactly, never folded.
    let error = match specforge_common::suggest::find_close_match(
        requested,
        available.iter().map(String::as_str),
    ) {
        Some(close) => error.with_suggestion(format!("did you mean '{close}'?")),
        None => error,
    };
    Err(error.about(AnalyzeField::Pass))
}

/// `<extension>:<pass>` of every analyze-phase pass the extensions declare.
fn declared_pass_names(view: &ProjectView) -> Vec<String> {
    view.registries()
        .analyze_passes()
        .map(DeclaredPass::full_name)
        .collect()
}

fn read_report(
    view: &ProjectView,
    source: &ReportSource,
) -> Result<Option<Arc<TestReport>>, Refusal<AnalyzeField>> {
    match source {
        ReportSource::None => Ok(None),
        // A named file that is not there is about the report; an unusable one
        // is a refusal of the file (ADR 0072 D2).
        ReportSource::Named { path, containment } => crate::report::named(path, containment)
            .map(Some)
            .map_err(|error| error.about(AnalyzeField::Report)),
        ReportSource::Recorded => Ok(view.test_report()?),
    }
}

#[cfg(test)]
mod tests;
