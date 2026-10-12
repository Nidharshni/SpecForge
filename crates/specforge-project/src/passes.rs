//! Extension compiler passes: the input every `__pass_<name>` export
//! receives and how its answer is read. What an extension declares, and in
//! which order its passes run, is the registry build's
//! (`specforge_registry::RegistryBuild::passes`). Check-phase passes run with
//! every compile ([`crate::check_passes`]); the others under
//! `specforge analyze` (`specforge_ops::analyze`). Findings are standard host
//! diagnostics.

use specforge_common::{Diagnostic, codes};
use specforge_diagnostics::{Level, check_extension_code};
use specforge_graph::Graph;
use specforge_protocol_types::{PassInput, PassOutput, PassSeverity, PassTestResults};
use specforge_registry::{FieldRegistry, KindRegistry};
use specforge_wasm::ExtensionCalls;
use std::collections::HashSet;

use crate::coverage::TestReport;
use crate::snapshot::EntitySnapshot;

/// Everything a pass may inspect. Built once per analysis invocation.
pub struct AnalysisContext<'a> {
    pub graph: &'a Graph,
    pub kind_registry: &'a KindRegistry,
    pub field_registry: &'a FieldRegistry,
    /// The graph's entity snapshot (ADR 0019): what the passes receive,
    /// each entity with its standing (who is exempt).
    pub entities: &'a EntitySnapshot,
    /// Parsed `--test-results` report, when provided; forwarded to extension
    /// passes (the coverage pass scores proof from it).
    pub test_results: Option<&'a TestReport>,
    /// Entity ids whose formal claims the prove pass ENTAILED from the
    /// declared bounds (RES-25). `None` when the prove pass did not run;
    /// a proved claim discharges `verify property` obligations without
    /// executable tests.
    pub proved_claims: Option<&'a std::collections::HashSet<String>>,
}

/// A diagnostic returned by an analysis pass, ready for host rendering.
pub type Finding = Diagnostic;

// ── Extension-owned compiler passes (WASM-only migration, Phase 4) ─────────

/// Result of one extension-owned compiler pass.
pub struct ExtensionPassReport {
    /// `<extension>:<pass>` identifier.
    pub name: String,
    pub findings: Vec<Finding>,
    pub summary: serde_json::Value,
}

/// The input every `__pass_<name>` export receives (the protocol's
/// `PassInput`): the entity snapshot's adapters (its entities and edges,
/// ADR 0019), and the test results and proved claims when the caller has
/// them. Each entity carries how the coverage rule sees it: `testable` is
/// its kind's flag (a kind that merely accepts `verify` statements, a
/// formal `property`, does not count), and `owes_none` says it owes no
/// obligations of its own (ADR 0004, D2-b), its standing's.
pub fn pass_input(input: &AnalysisContext) -> PassInput {
    PassInput {
        entities: input.entities.pass_entities(),
        edges: input.entities.pass_edges(),
        test_results: input.test_results.map(PassTestResults::from),
        proved_claims: input.proved_claims.map(sorted_ids),
        previous: None,
    }
}

/// The ids of `claims`, sorted.
fn sorted_ids(claims: &std::collections::HashSet<String>) -> Vec<String> {
    let mut ids: Vec<String> = claims.iter().cloned().collect();
    ids.sort();
    ids
}

/// The host diagnostics of `extension`'s pass `pass` answering `output`: in
/// canonical order, a span-less one naming an entity of `entities` given
/// that entity's span. A code the extension may not report at the severity
/// it gave (`check_extension_code`) is reported once, as W150 after the
/// findings; the findings themselves are kept as the pass gave them.
pub fn pass_findings(
    extension: &str,
    pass: &str,
    output: PassOutput,
    entities: &EntitySnapshot,
) -> Vec<Diagnostic> {
    let mut misused = code_misuse(extension, pass, &output);
    let mut findings = specforge_wasm::pass_diagnostics(extension, output, |id| {
        entities.get(id).map(|(record, _)| record.span.clone())
    });
    findings.append(&mut misused);
    findings
}

/// W150 for each distinct (code, severity) of `output` that `extension`
/// may not report.
fn code_misuse(extension: &str, pass: &str, output: &PassOutput) -> Vec<Diagnostic> {
    let mut seen = HashSet::new();
    let mut diagnostics = Vec::new();
    for finding in &output.diagnostics {
        let level = match finding.severity {
            PassSeverity::Error => Level::Error,
            PassSeverity::Warning => Level::Warning,
            PassSeverity::Info => Level::Info,
        };
        let Err(misuse) = check_extension_code(extension, &finding.code, level) else {
            continue;
        };
        if !seen.insert((finding.code.as_str(), level)) {
            continue;
        }
        diagnostics.push(
            Diagnostic::new(
                codes::W150,
                format!(
                    "extension '{extension}' pass '{pass}' reported '{}': {misuse}",
                    finding.code
                ),
            )
            .with_suggestion(
                "renumber the diagnostic in the extension's range, or, for a first-party \
                 extension, catalogue the code",
            ),
        );
    }
    diagnostics.sort_by(|a, b| a.message.cmp(&b.message));
    diagnostics
}

/// Run the extensions' analyze passes (`passes` as the registry build
/// ordered them; check-phase passes run with every compile instead)
/// through the wasm runtime: each `__pass_<name>` export receives the
/// entity snapshot and returns host diagnostics. A pass that fails (it
/// traps, or answers what does not parse) has a report of its own with one
/// E028 finding, so the analysis fails.
pub fn run_extension_passes(
    passes: &[specforge_registry::DeclaredPass],
    input: &AnalysisContext,
    runtime: &dyn specforge_wasm::runtime::WasmRuntime,
    requested: &str,
) -> Vec<ExtensionPassReport> {
    if passes.is_empty() {
        return Vec::new();
    }
    // Only the "all" sweep and exact `<extension>:<pass>` selections run
    // extension passes.
    let wants = |name: &str| requested == "all" || requested == name;

    let payload = pass_input(input);
    let entities_analyzed = payload.entities.len();
    let encoded = ExtensionCalls::encode(&payload);
    let calls = ExtensionCalls::new(runtime);

    let mut reports = Vec::new();
    for declared in passes.iter().filter(|p| !p.is_check_phase()) {
        let report_name = declared.full_name();
        if !wants(&report_name) {
            continue;
        }
        let pass = &declared.pass;
        let mut summary = serde_json::Map::new();
        summary.insert("extension".into(), declared.extension.clone().into());
        summary.insert("pass".into(), pass.name.clone().into());
        summary.insert("entities_analyzed".into(), entities_analyzed.into());
        let answer = calls.run_pass(&declared.extension, &pass.name, &encoded);
        let findings = match answer {
            Ok(output) => {
                let extra = output.summary.clone();
                summary.extend(extra);
                pass_findings(&declared.extension, &pass.name, output, input.entities)
            }
            Err(error) => {
                summary.insert("failed".into(), true.into());
                vec![error.diagnostic()]
            }
        };
        reports.push(ExtensionPassReport {
            name: report_name,
            findings,
            summary: serde_json::Value::Object(summary),
        });
    }
    reports
}

/// Keys an extension pass adds to its report summary.
pub type PassSummary = serde_json::Map<String, serde_json::Value>;
