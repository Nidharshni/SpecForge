use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, out};
use serde::Serialize;
use specforge_ops::analysis::doctor::{
    About, BinaryIssue, DoctorDocument, DoctorReport, Finding, FindingStatus, Part, diagnose,
};
use specforge_ops::extension::Status;
use specforge_ops::solver::Z3;
use specforge_ops::view::ProjectView;
use specforge_registry_client::credential_health::{
    CredentialHealth, CredentialLevel, CredentialLine, user_credential_health,
};

// `specforge doctor`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: crate::project::ProjectArg,

    #[command(flatten)]
    output: OutputArg,
}

/// What `specforge doctor --format json` prints: the project's report and
/// the user's registry credentials, which only the CLI shows (ADR 0029 D3a).
#[derive(Serialize)]
struct Printed<'a> {
    #[serde(flatten)]
    report: DoctorDocument,
    credentials: &'a [CredentialLine],
    credentials_failures: usize,
}

impl Args {
    /// `specforge doctor`: the shared project health report (extensions and
    /// their enhancements, conflicts, shadowed keywords, installed
    /// binaries) plus registry credential health. Exit 1 on any
    /// error-level finding or credential failure.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let project = match self.project.compile() {
            Ok(project) => project,
            Err(error) => return Refusal::of(self.output.format).report(terminal, &error),
        };
        let report = diagnose(&ProjectView::of(&project), &Z3::default());
        let credentials = user_credential_health();

        match self.output.format {
            OutputFormat::Json => {
                terminal.json(&Printed {
                    report: report.document(),
                    credentials: &credentials.lines,
                    credentials_failures: credentials.failures,
                });
            }
            OutputFormat::Human => out!(terminal, "{}", render_human(&report, &credentials)),
        }

        // Doctor checks the project and the user's credentials: both must
        // pass.
        Exit::of_verdict(report.ok() && credentials.failures == 0)
    }
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
            finding_lines(&mut out, finding);
        }
        line!();
    }

    // The lock file, when it exists and cannot be read (E033).
    let lock: Vec<_> = report.about(Part::Lock).collect();
    if !lock.is_empty() {
        line!("Lock file:");
        for finding in lock {
            finding_lines(&mut out, finding);
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
        finding_lines(&mut out, conflict);
    }

    line!();
    line!("Shadowed constructs:");
    let mut shadowed = report.shadowed().peekable();
    if shadowed.peek().is_none() {
        line!("  none");
    }
    for (keyword, shadow) in shadowed {
        line!("  '{keyword}' [{}] {}", shadow.code, shadow.check);
    }

    line!();
    line!("Peer requirements:");
    let mut peers = report.about(Part::Peer).peekable();
    if peers.peek().is_none() {
        line!("  all satisfied");
    }
    for problem in peers {
        finding_lines(&mut out, problem);
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
    line!("Extension load:");
    // A missing or changed binary is listed above, once, with its remedy.
    let mut loads = report
        .about(Part::Load)
        .chain(report.about(Part::StructuralOnly))
        .peekable();
    if loads.peek().is_none() {
        line!("  none");
    }
    for finding in loads {
        finding_lines(&mut out, finding);
    }

    // What the registry build and the providers say about the setup.
    for (title, part) in [
        ("Providers", Part::Providers),
        ("Declarations", Part::Declaration),
        ("Registries", Part::Registry),
        ("Rules", Part::Rule),
        ("Surfaces", Part::Surface),
        ("Toolchain", Part::Toolchain),
    ] {
        let mut findings = report.about(part).peekable();
        if findings.peek().is_none() {
            continue;
        }
        line!();
        line!("{title}:");
        for finding in findings {
            finding_lines(&mut out, finding);
        }
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

/// A finding as two lines: `  [TAG] [CODE] check`, then its remedy.
fn finding_lines(out: &mut String, finding: &Finding) {
    let tag = match finding.status {
        FindingStatus::Error => "ERROR",
        FindingStatus::Warn => "WARN",
        FindingStatus::Info => "INFO",
    };
    out.push_str(&format!("  [{tag}] [{}] {}\n", finding.code, finding.check));
    out.push_str(&format!("    fix: {}\n", finding.remediation));
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    /// An extension of `name` declaring the kind `feature` and the edge type
    /// `uses`.
    fn declaring_feature_and_uses(name: &str) -> specforge_protocol_types::ExtensionDeclaration {
        specforge_protocol_types::ExtensionDeclaration {
            handshake: specforge_protocol_types::HandshakeResponse {
                name: name.into(),
                version: "1.0.0".into(),
                ..Default::default()
            },
            entities: vec![specforge_protocol_types::EntityKindDescriptor {
                name: "feature".into(),
                ..Default::default()
            }],
            edges: vec![specforge_protocol_types::EdgeTypeDescriptor {
                label: "uses".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    // No shipped builtin set produces an extension conflict (all nine
    // together compile clean), so the conflicts come in at the registry
    // build's seam: two declarations that claim one kind and one edge type.
    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor reports conflicts with resolution suggestions"
    )]
    fn doctor_reports_conflicts_with_a_resolution_suggestion() {
        let project = specforge_project::testing::Assembled::over(
            specforge_registry::build_registries(vec![
                declaring_feature_and_uses("acme"),
                declaring_feature_and_uses("@specforge/product"),
            ]),
        );
        let view = ProjectView::of(project.compiled());
        let report = specforge_ops::analysis::doctor::diagnose(
            &view,
            &specforge_ops::testing::ScriptedSolver::ready(),
        );

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
        assert!(
            conflicts[1]
                .remediation
                .starts_with("Two extensions register an edge type"),
            "{:?}",
            conflicts[1]
        );
        assert!(!report.ok(), "an error-level conflict fails doctor");

        let none = CredentialHealth {
            lines: Vec::new(),
            failures: 0,
            signing_key_present: false,
        };
        let human = render_human(&report, &none);
        let section = &human[human.find("Conflicts:").expect("conflicts section")..];
        assert!(
            section.contains("[ERROR] [E026] entity kind 'feature'"),
            "{human}"
        );
        assert!(
            section.contains("fix: Two extensions register the same entity kind keyword"),
            "{human}"
        );
        assert!(section.contains("[WARN] [W018]"), "{human}");
        assert!(
            section.contains("fix: Two extensions register an edge type"),
            "{human}"
        );
        assert!(human.contains("1 issue(s) found."), "{human}");
    }
}
