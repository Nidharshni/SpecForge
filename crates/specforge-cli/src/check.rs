use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, err, errln};
use clap::builder::{PossibleValuesParser, TypedValueParser};
use specforge_common::{Diagnostic, Severity, diagnostic_summary, render_diagnostics};
use specforge_ops::check::{CacheRecord, CheckOptions, check};
use specforge_ops::view::ProjectView;
use specforge_ops::{OpError, OpErrorKind};
use specforge_project::{LintProfile, SharedRuntime};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// `specforge check`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(default_value = ".", help = crate::project::PATH_HELP)]
    path: PathBuf,

    /// Promote warnings to errors
    #[arg(long)]
    strict: bool,

    #[command(flatten)]
    output: OutputArg,

    /// Extra lint profiles: inferred (I200/I202 from specforge-infer.json);
    /// pedantic is the default and adds nothing
    #[arg(
        long,
        value_delimiter = ',',
        value_parser = PossibleValuesParser::new(specforge_project::LINT_PROFILE_NAMES)
            .try_map(|name| name.parse::<specforge_project::LintProfile>())
    )]
    lint: Vec<LintProfile>,

    /// Show only diagnostics of this severity (after --strict); the exit
    /// code and --cache still judge everything reported
    #[arg(
        long,
        ignore_case = true,
        value_parser = PossibleValuesParser::new(specforge_ops::check::SEVERITY_NAMES)
            .try_map(|name| specforge_ops::check::parse_severity(&name))
    )]
    severity: Option<Severity>,

    /// Record each entity's lifecycle state (e.g. a feature's status) in
    /// specforge-cache.json when the check passes (the build cache history
    /// rules compare against)
    #[arg(long)]
    cache: bool,
}

impl Args {
    /// The check the flags ask for.
    fn options(&self) -> CheckOptions {
        CheckOptions {
            strict: self.strict,
            lint_profiles: self.lint.clone(),
            severity: self.severity,
            record_cache: self.cache,
        }
    }

    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let runtime: SharedRuntime =
            Arc::new(specforge_component::ComponentRuntime::with_user_cache());
        run_in(
            &self.path,
            runtime,
            self.output.format,
            &self.options(),
            terminal,
        )
    }
}

/// `specforge check` with the project's extensions running in `runtime`:
/// the check operation over what the compile reported, the diagnostics the
/// severity filter shows rendered, and the verdict over everything as the
/// exit code. With `record_cache`, a check that passes records the build's
/// lifecycle states (the fields kinds declare as `lifecycle_field`) in
/// `specforge-cache.json`.
fn run_in(
    path: &Path,
    runtime: SharedRuntime,
    format: OutputFormat,
    options: &CheckOptions,
    terminal: &Terminal,
) -> Exit {
    if options.lint_profiles.contains(&LintProfile::Pedantic) {
        errln!(
            terminal,
            "note: --lint pedantic is the default: info diagnostics are always reported"
        );
    }
    let compiled = match crate::project::compile_in(path, runtime) {
        Ok(compiled) => compiled,
        Err(error) => return Refusal::of(format).report(terminal, &error),
    };
    let outcome = match check(&ProjectView::of(&compiled), compiled.diagnostics(), options) {
        Ok(outcome) => outcome,
        Err(error) => return Refusal::of(format).report(terminal, &error.into()),
    };
    let shown: Vec<Diagnostic> = outcome.shown().into_iter().cloned().collect();

    match format {
        OutputFormat::Json => {
            let entries = specforge_common::diagnostics_json(&shown);
            terminal.json(&entries);
        }
        OutputFormat::Human => {
            let color = terminal.facts().color_err;
            if !shown.is_empty() {
                let sources = compiled.source_texts();
                let rendered = render_diagnostics(&shown, &sources, color);
                err!(terminal, "{}", rendered);
            }
            // The summary counts everything reported, as the verdict does.
            let summary = diagnostic_summary(&outcome.reported, color);
            errln!(terminal, "{}", with_filter_note(&summary, outcome.severity));
        }
    }

    match &outcome.cache {
        CacheRecord::NotRequested | CacheRecord::Written => {}
        CacheRecord::NotWritten => errln!(
            terminal,
            "note: {} not written: the check failed",
            specforge_project::BUILD_CACHE_FILE
        ),
        CacheRecord::WriteFailed(e) => {
            return Refusal::of(format).report(
                terminal,
                &OpError::new(
                    OpErrorKind::Internal,
                    "cache_write_failed",
                    format!("cannot write {}: {e}", specforge_project::BUILD_CACHE_FILE),
                ),
            );
        }
    }

    // Strict already promoted warnings: errors alone decide.
    Exit::of_verdict(outcome.ok())
}

/// `summary` with `(showing <severity> only)` after its first line when a
/// severity filter hides the rest.
fn with_filter_note(summary: &str, severity: Option<Severity>) -> String {
    let Some(severity) = severity else {
        return summary.to_string();
    };
    let (first, rest) = match summary.split_once('\n') {
        Some((first, rest)) => (first, Some(rest)),
        None => (summary, None),
    };
    let mut out = format!("{first} (showing {severity} only)");
    if let Some(rest) = rest {
        out.push('\n');
        out.push_str(rest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Facts;
    use serde_json::json;
    use specforge_extension_sdk::prelude::*;
    use specforge_test_macros::test as specforge_test;
    use specforge_wasm::testing::InProcessRuntime;

    const EXT: &str = "@test/audit";

    /// An extension, in process, that declares the `gadget` kind and one
    /// check-phase pass, `audit`, which fails (E951) every gadget whose id
    /// starts with `bad`.
    fn audit_extension() -> InProcessRuntime {
        InProcessRuntime::new().with(|| {
            let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "1.0.0"));
            c.kind("gadget", |k| {
                k.keyword("gadget");
            });
            c.pass("audit", |p| {
                p.phase("check").run(|input: &PassInput| {
                    input
                        .entities
                        .iter()
                        .filter(|e| e.id.starts_with("bad"))
                        .map(|e| {
                            PassDiagnostic::new(
                                "E951",
                                PassSeverity::Error,
                                format!("gadget '{}' fails the audit", e.id),
                            )
                            .with_entity(&e.id)
                        })
                        .collect::<Vec<_>>()
                });
            });
            c
        })
    }

    fn project(spec: &str) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        let config = json!({ "name": "p", "version": "0.1.0", "extensions": [EXT] });
        std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
        specforge_installed::testing::install(dir.path(), &[EXT]);
        std::fs::write(dir.path().join("a.spec"), spec).unwrap();
        dir
    }

    #[specforge_test(
        behavior = "run_check_phase_passes",
        verify = "a check pass's diagnostics are reported by specforge check"
    )]
    fn check_reports_a_check_pass_and_exits_on_its_errors() {
        let clean = project("gadget good \"Good\" {\n}\n");
        let failing = project("gadget good \"Good\" {\n}\n\ngadget bad_one \"Bad\" {\n}\n");

        let human = |dir: &tempfile::TempDir| {
            let (terminal, capture) = Terminal::captured("", Facts::PIPED);
            let exit = run_in(
                dir.path(),
                std::sync::Arc::new(audit_extension()),
                OutputFormat::Human,
                &CheckOptions::default(),
                &terminal,
            );
            (exit, capture)
        };
        let (exit, capture) = human(&clean);
        assert_eq!(exit, Exit::Passed);
        assert_eq!(capture.stdout(), "");
        let (exit, capture) = human(&failing);
        assert_eq!(exit, Exit::Failed, "the pass's error fails the check");
        // The pass's diagnostic and the summary go to stderr, nothing to stdout.
        assert!(capture.stderr().contains("E951"), "{}", capture.stderr());
        assert!(
            capture.stderr().trim_end().ends_with("1 error") || capture.stderr().contains("error"),
            "{}",
            capture.stderr()
        );
        assert_eq!(capture.stdout(), "");

        // What check reports is the compile's diagnostics, the pass's
        // among them with its code and severity.
        let reported = specforge_project::CompiledProject::compile(
            &specforge_common::ProjectRoot::of(failing.path()).unwrap(),
            Some(Arc::new(audit_extension())),
        )
        .diagnostics();
        let audit: Vec<_> = reported.iter().filter(|d| d.code == "E951").collect();
        assert_eq!(audit.len(), 1, "{reported:?}");
        assert_eq!(audit[0].severity, specforge_common::Severity::Error);
    }
}
