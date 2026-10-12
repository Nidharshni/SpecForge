//! `specforge analyze` — static analysis passes over the compiled graph.
//!
//! The analysis itself is [`specforge_ops::analyze`], shared with the MCP
//! tool. This module is the CLI half: arguments in, rendering and exit
//! codes out.

use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, err, errln, outln};
use specforge_common::{codes, diagnostic_summary, render_diagnostics, truncate_diagnostics};
use specforge_ops::OpError;
use specforge_ops::analyze::{
    AnalyzeOptions, AnalyzeOutcome, ProveOptions, ReportSource, StrayRecord, analyze,
};
use specforge_ops::view::ProjectView;
use specforge_project::CompiledProject;
use std::path::PathBuf;

// `specforge analyze`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Analysis pass to run: all (default), coverage, contracts, or a pass
    /// an extension declares (`<extension>:<pass>`)
    pass: Option<String>,

    /// Project directory
    #[arg(long, default_value = ".")]
    path: PathBuf,

    #[command(flatten)]
    output: OutputArg,

    /// Fail (exit 1) on warnings as well as errors
    #[arg(long, default_value_t = false)]
    strict: bool,

    /// Test-results report for proof-level verdicts (default: the project's
    /// specforge-report.json, written by `specforge collect`)
    #[arg(long)]
    test_results: Option<PathBuf>,

    /// Verify declared bounds and claims with an SMT solver (z3)
    #[arg(long, default_value_t = false)]
    prove: bool,

    /// Fail with a non-zero exit when proof coverage falls below this
    /// percentage (needs test results)
    #[arg(long)]
    min: Option<f64>,
}

impl Args {
    /// The analysis the flags ask for: any pass name (the passes are the
    /// project's, so the operation refuses one it does not run, exit 2 as
    /// clap refuses a flag's value), the report named or the one
    /// `specforge collect` last recorded.
    fn options(&self) -> AnalyzeOptions {
        AnalyzeOptions {
            pass: self
                .pass
                .clone()
                .unwrap_or_else(|| specforge_ops::analyze::EVERY_PASS.to_string()),
            strict: self.strict,
            report: match &self.test_results {
                Some(path) => ReportSource::File(path.clone()),
                None => ReportSource::Recorded,
            },
            min: self.min,
            prove: self.prove.then(ProveOptions::default),
        }
    }

    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let project = match crate::project::compile(&self.path) {
            Ok(project) => project,
            Err(error) => return Refusal::measuring(self.output.format).report(terminal, &error),
        };
        // A refusal is the error document under `--format json`, the lines
        // on stderr otherwise; analyze measures, so it cannot judge the
        // project either way.
        let format = self.output.format;
        let outcome = match analyze(&ProjectView::of(&project), &self.options()) {
            Ok(outcome) => outcome,
            Err(error) => {
                return Refusal::measuring(format).report(terminal, &OpError::from(error));
            }
        };
        // D3: stray test records, on stderr before the reports. Exact
        // matching is preserved; the warning only surfaces what was
        // silently dropped before.
        for stray in &outcome.stray_records {
            errln!(terminal, "{}", stray_line(stray));
        }
        match format {
            OutputFormat::Json => terminal.json(&outcome.document()),
            OutputFormat::Human => human(&outcome, &project, terminal),
        }

        // The gate comes after the reports print, so the operator still
        // sees the full analysis. Under `--format json` the document carries where
        // it landed and nothing goes to stderr.
        if let (OutputFormat::Human, Some(failure)) = (format, outcome.gate_failure()) {
            err!(terminal, "{}", crate::outcome::error_lines(&failure, None));
        }
        Exit::of(outcome.verdict())
    }
}

/// The warning a test record that names no entity gets.
fn stray_line(stray: &StrayRecord) -> String {
    match &stray.near {
        Some(near) => format!(
            "{}: test record references unknown entity '{}' (did you mean '{near}'?)",
            codes::W097,
            stray.entity_id
        ),
        None => format!(
            "{}: test record references unknown entity '{}'",
            codes::W097,
            stray.entity_id
        ),
    }
}

/// The reports, one per pass, then the diagnostic summary.
fn human(outcome: &AnalyzeOutcome, project: &CompiledProject, terminal: &Terminal) {
    let reports = &outcome.passes;
    // Human output is capped at the codebase-wide diagnostic limit so a
    // noisy first run stays readable; JSON output is never truncated.
    let color = terminal.facts().color_out;
    // The text each file was compiled from, to quote in snippets.
    let sources = project.source_texts();
    for report in reports {
        outln!(terminal, "analyze/{} — {}", report.name, report.description);
        if report.findings.is_empty() {
            outln!(terminal, "  no findings");
        } else {
            let mut rendered_findings = report.findings.clone();
            truncate_diagnostics(&mut rendered_findings);
            let rendered = render_diagnostics(&rendered_findings, &sources, color);
            if !rendered.is_empty() {
                outln!(terminal, "{rendered}");
            } else {
                for d in &rendered_findings {
                    outln!(terminal, "  [{d}]");
                }
            }
        }
        let mut lines = Vec::new();
        summary_lines(&report.summary, "", &mut lines);
        if !lines.is_empty() {
            outln!(terminal, "  summary:");
            for line in lines {
                outln!(terminal, "    {line}");
            }
        }
        outln!(terminal);
    }
    outln!(
        terminal,
        "{}",
        diagnostic_summary(
            &reports
                .iter()
                .flat_map(|r| r.findings.iter().cloned())
                .collect::<Vec<_>>(),
            color,
        )
    );
}

/// A pass summary as `key: value` lines, nested keys dotted. Its shape is
/// the extension's, so nothing here knows what the keys mean; nulls (a
/// section with nothing to report) are left out.
fn summary_lines(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Null => {}
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                summary_lines(value, &key, out);
            }
        }
        serde_json::Value::String(s) => out.push(format!("{prefix}: {s}")),
        other => out.push(format!("{prefix}: {other}")),
    }
}

#[cfg(test)]
mod summary_tests {
    use super::{Args, ReportSource, summary_lines};
    use clap::Parser;

    #[derive(Parser)]
    struct Line {
        #[command(flatten)]
        args: Args,
    }

    fn options(flags: &[&str]) -> specforge_ops::analyze::AnalyzeOptions {
        Line::try_parse_from(std::iter::once("analyze").chain(flags.iter().copied()))
            .expect("the flags parse")
            .args
            .options()
    }

    #[test]
    fn the_flags_build_the_analysis_options() {
        let plain = options(&[]);
        assert_eq!(plain.pass, specforge_ops::analyze::EVERY_PASS);
        assert!(matches!(plain.report, ReportSource::Recorded));
        assert!(plain.prove.is_none() && plain.min.is_none() && !plain.strict);

        let asked = options(&[
            "coverage",
            "--test-results",
            "r.json",
            "--prove",
            "--min",
            "80",
            "--strict",
        ]);
        assert_eq!(asked.pass, "coverage");
        assert!(
            matches!(&asked.report, ReportSource::File(p) if p == std::path::Path::new("r.json"))
        );
        assert!(asked.prove.is_some() && asked.strict);
        assert_eq!(asked.min, Some(80.0));
    }

    #[test]
    fn nested_keys_are_dotted_and_nulls_left_out() {
        let mut lines = Vec::new();
        let summary = serde_json::json!({
            "pass": "coverage",
            "funnel": {"proven": 2, "failures": 0},
            "results": null,
            "kinds": ["a", "b"]
        });
        summary_lines(&summary, "", &mut lines);
        assert_eq!(
            lines,
            vec![
                "funnel.failures: 0",
                "funnel.proven: 2",
                "kinds: [\"a\",\"b\"]",
                "pass: coverage"
            ]
        );
    }
}
