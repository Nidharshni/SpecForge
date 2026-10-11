use crate::OutputFormat;
use crate::outcome::{Exit, print_json};
use serde::Serialize;
use specforge_common::codes;
use specforge_ops::doctor::{
    About, BinaryIssue, DoctorDocument, DoctorReport, FindingStatus, Part, diagnose,
};
use specforge_ops::extension::Status;
use specforge_ops::view::ProjectView;
use specforge_registry_client::credential_health::{
    CredentialHealth, CredentialLevel, CredentialLine, user_credential_health,
};
use std::path::Path;

/// What `specforge doctor --format json` prints: the project's report and
/// the user's registry credentials, which only the CLI shows (ADR 0029 D3a).
#[derive(Serialize)]
struct Printed<'a> {
    #[serde(flatten)]
    report: DoctorDocument,
    credentials: &'a [CredentialLine],
    credentials_failures: usize,
}

/// `specforge doctor`: the shared project health report (extensions and
/// their enhancements, conflicts, shadowed keywords, installed binaries)
/// plus registry credential health. Exit 1 on any error-level finding or
/// credential failure.
pub fn run(path: &Path, format: OutputFormat) -> Exit {
    let project = crate::pipeline::compile_project(path);
    let report = diagnose(&ProjectView::of(&project));
    let credentials = user_credential_health();

    match format {
        OutputFormat::Json => {
            print_json(&Printed {
                report: report.document(),
                credentials: &credentials.lines,
                credentials_failures: credentials.failures,
            });
        }
        OutputFormat::Human => print!("{}", render_human(&report, &credentials)),
    }

    // Doctor checks the project and the user's credentials: both must pass.
    Exit::of_verdict(report.ok() && credentials.failures == 0)
}

/// The human report: one section per report part, then credentials.
fn render_human(report: &DoctorReport, credentials: &CredentialHealth) -> String {
    let mut out = String::new();
    macro_rules! line {
        () => { out.push('\n') };
        ($($arg:tt)*) => {{ out.push_str(&format!($($arg)*)); out.push('\n'); }};
    }
    // The config itself, when doctor has something to say about it (E069,
    // or no specforge.json at the root).
    let config: Vec<_> = report.about(Part::Config).collect();
    if !config.is_empty() {
        line!("Configuration:");
        for finding in config {
            let tag = match finding.status {
                FindingStatus::Error => "ERROR",
                FindingStatus::Warn => "WARN",
                FindingStatus::Ok => "ok",
            };
            line!("  [{tag}] [{}] {}", finding.code, finding.check);
            line!("    fix: {}", finding.remediation);
        }
        line!();
    }

    // The lock file, when it exists and cannot be read (E033).
    let lock: Vec<_> = report.about(Part::Lock).collect();
    if !lock.is_empty() {
        line!("Lock file:");
        for finding in lock {
            line!("  [ERROR] [{}] {}", finding.code, finding.check);
            line!("    fix: {}", finding.remediation);
        }
        line!();
    }

    line!("Extensions ({}):", report.extensions.len());
    if report.extensions.is_empty() {
        line!("  none");
    }
    for ext in &report.extensions {
        let version = ext
            .version
            .as_deref()
            .map(|v| format!(" {v}"))
            .unwrap_or_default();
        let status = match ext.status {
            Status::Loaded => String::new(),
            other => format!(" [{}]", other.as_str()),
        };
        line!(
            "  {}{version} ({}){status} — {} enhancement(s)",
            ext.name,
            ext.source,
            ext.enhancement_count
        );
    }

    line!();
    line!("Enhancements by entity kind:");
    if report.enhancements.is_empty() {
        line!("  none");
    }
    for (kind, entries) in &report.enhancements {
        line!("  {kind}:");
        for entry in entries {
            let mut contributed = entry.fields.clone();
            contributed.extend(entry.edge_types.iter().map(|e| format!("edge {e}")));
            if let Some(kinds) = &entry.verify_kinds {
                contributed.push(format!("verify [{}]", kinds.join(", ")));
            }
            line!("    {}: {}", entry.extension, contributed.join(", "));
        }
    }

    line!();
    line!("Conflicts:");
    let mut conflicts = report.conflicts().peekable();
    if conflicts.peek().is_none() {
        line!("  none");
    }
    for conflict in conflicts {
        line!("  [{}] {}", conflict.code, conflict.check);
        line!("    fix: {}", conflict.remediation);
    }

    line!();
    line!("Shadowed constructs:");
    let mut shadowed = report.about(Part::Shadowing).peekable();
    if shadowed.peek().is_none() {
        line!("  none");
    }
    for shadow in shadowed {
        let About::Shadowing { keyword } = &shadow.about else {
            continue;
        };
        line!("  '{keyword}' [{}] {}", shadow.code, shadow.check);
    }

    line!();
    line!("Peer requirements:");
    let mut peers = report.about(Part::Peer).peekable();
    if peers.peek().is_none() {
        line!("  all satisfied");
    }
    for problem in peers {
        line!("  [{}] {}", problem.code, problem.check);
        line!("    fix: {}", problem.remediation);
    }

    line!();
    line!(
        "Installed binaries ({} lock entr{} checked):",
        report.installed_count,
        if report.installed_count == 1 {
            "y"
        } else {
            "ies"
        }
    );
    let mut binaries = report.about(Part::Binary).peekable();
    if binaries.peek().is_none() {
        line!("  All installed binaries healthy.");
    }
    for finding in binaries {
        let About::Binary { issue } = &finding.about else {
            continue;
        };
        match issue {
            BinaryIssue::MissingBinary { name } => {
                line!("  [MISSING] {name} — .wasm binary not found");
            }
            BinaryIssue::StaleHash {
                name,
                expected,
                actual,
            } => line!(
                "  [STALE] {} — hash mismatch (expected {}, got {})",
                name,
                &expected[..8.min(expected.len())],
                &actual[..8.min(actual.len())]
            ),
        }
    }

    line!();
    line!("Extension load failures:");
    // A missing or changed binary is listed above, once, with its remedy.
    let mut failures = report.about(Part::Load).peekable();
    if failures.peek().is_none() {
        line!("  none");
    }
    for failure in failures {
        line!("  [{}] {}", failure.code, failure.check);
        line!("    fix: {}", failure.remediation);
    }
    if !report.z3_available {
        line!();
        line!(
            "[WARN] z3 not on PATH — `specforge analyze --prove` skips SMT checks ({})",
            codes::W098
        );
    }

    let errors = report
        .findings
        .iter()
        .filter(|f| f.status == FindingStatus::Error)
        .count();
    line!();
    if errors == 0 {
        line!("No issues found.");
    } else {
        line!("{errors} issue(s) found.");
    }

    if !credentials.lines.is_empty() || credentials.signing_key_present {
        line!();
        line!("Registry credentials:");
        for credential in &credentials.lines {
            let tag = match credential.level {
                CredentialLevel::Error => "ERROR",
                CredentialLevel::Warning => "WARN",
                CredentialLevel::Ok => "ok",
            };
            line!("  [{tag}] {}", credential.text);
        }
        if credentials.signing_key_present {
            line!("  [ok] signing key present");
        } else {
            line!("  [ok] signing key: not created yet (generated on first publish)");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::{Diagnostic, Severity};
    use specforge_test_macros::test as specforge_test;

    fn conflict(
        code: &str,
        severity: Severity,
        message: &str,
        suggestion: Option<&str>,
    ) -> Diagnostic {
        let mut diagnostic = Diagnostic::untyped(code, severity, message);
        diagnostic.suggestion = suggestion.map(String::from);
        diagnostic
    }

    // No shipped builtin set produces an extension conflict (all nine
    // together compile clean), so the conflicts come in at the shared
    // report's seam: the diagnostics a compile hands `diagnose`.
    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor reports conflicts with resolution suggestions"
    )]
    fn doctor_reports_conflicts_with_a_resolution_suggestion() {
        let dir = tempfile::TempDir::new().unwrap();
        let diagnostics = [
            conflict(
                "E026",
                Severity::Error,
                "entity kind 'feature' registered by 'acme' conflicts with '@specforge/product' (first registration wins)",
                None,
            ),
            conflict(
                "W018",
                Severity::Warning,
                "edge type 'uses' declared by both 'acme' and '@specforge/software'",
                Some("rename one of the edge types"),
            ),
            conflict("W001", Severity::Warning, "an unrelated warning", None),
        ];

        let graph = specforge_graph::Graph::new();
        let env = specforge_project::Environment::with_registries(Default::default(), None);
        let recorded = specforge_project::coverage::RecordedCoverage::over(&graph, &env);
        let view =
            ProjectView::new(&graph, &env, Some(dir.path()), &recorded).reporting(&diagnostics);
        let report = specforge_ops::doctor::diagnose_with(&view, true);

        let conflicts: Vec<_> = report.about(Part::Conflict).collect();
        let codes: Vec<&str> = conflicts.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, ["E026", "W018"]);
        // No suggestion of its own: the catalogued explanation stands in.
        assert!(
            conflicts[0]
                .remediation
                .ends_with("Rename the conflicting kind keyword."),
            "{:?}",
            conflicts[0]
        );
        // The diagnostic's own suggestion wins.
        assert_eq!(conflicts[1].remediation, "rename one of the edge types");
        assert!(!report.ok(), "an error-level conflict fails doctor");

        let none = CredentialHealth {
            lines: Vec::new(),
            failures: 0,
            signing_key_present: false,
        };
        let human = render_human(&report, &none);
        let section = &human[human.find("Conflicts:").expect("conflicts section")..];
        assert!(section.contains("[E026] entity kind 'feature'"), "{human}");
        assert!(
            section.contains("fix: Two extensions register the same entity kind keyword"),
            "{human}"
        );
        assert!(section.contains("[W018]"), "{human}");
        assert!(
            section.contains("fix: rename one of the edge types"),
            "{human}"
        );
        assert!(human.contains("1 issue(s) found."), "{human}");
    }
}
