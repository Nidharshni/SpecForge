//! `specforge collect` — run the project's test runners and record which
//! entities their tests prove in `specforge-report.json` (ADR 0002).
//!
//! Runners are extensions (`@specforge/cargo-test`, …). Each declares a
//! collector: the files that select it, the command that runs it, where
//! the report lands, and a pure export that maps the report to entities.
//! This command runs the declared command after the user approves it for
//! the project, then hands the report to the extension. `--no-run` and
//! `--report` skip running and parse an existing report instead. The flow
//! itself lives in [`specforge_ops::collect`], shared with MCP; this
//! adapter only chooses the consent (a terminal prompt, `--yes`, or what
//! was approved before) and presents the outcome.

use crate::terminal::{Terminal, errln, outln};

use crate::outcome::{Exit, OutputFormat, Refusal};
use specforge_ops::collect::{self, Collector, Consent, Mode, Request, RunnerOutput};
use specforge_ops::view::ProjectView;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

pub struct Options<'a> {
    pub runner: Option<&'a str>,
    pub no_run: bool,
    pub reports: &'a [PathBuf],
    pub yes: bool,
}

pub fn run(path: &Path, options: &Options, format: OutputFormat, terminal: &Terminal) -> Exit {
    // The project the path is in (ADR 0029 D6); collect refuses one that is none.
    let root = specforge_common::project_root_of(path);

    let project = crate::pipeline::compile_project(&root);

    let mode = if !options.reports.is_empty() {
        Mode::Reports(options.reports)
    } else if options.no_run {
        Mode::NoRun
    } else if format == OutputFormat::Json {
        // Keep stdout for the JSON result.
        Mode::Run(RunnerOutput::Stderr)
    } else {
        Mode::Run(RunnerOutput::Inherit)
    };
    let mut prompt = |collector: &Collector, argv: &[String]| ask(collector, argv, &root);
    let consent = if options.yes {
        Consent::Yes
    } else if format == OutputFormat::Json || !std::io::stdin().is_terminal() {
        // Nobody to ask: only what was approved before runs.
        Consent::Approved
    } else {
        Consent::Prompt(&mut prompt)
    };
    let mut announce = |collector: &Collector, argv: &[String]| {
        if format != OutputFormat::Json {
            errln!(
                terminal,
                "running {} ({}): {}",
                collector.name,
                collector.extension,
                argv.join(" ")
            );
        }
    };
    let request = Request {
        runner: options.runner,
        mode,
        consent,
        announce: &mut announce,
    };
    // The view is rooted where the project compiled: the report lands where
    // every view over the project reads it.
    let outcome = match collect::collect(&ProjectView::of(&project), request) {
        Ok(outcome) => outcome,
        Err(e) => {
            return Refusal::of(format).report(terminal, &e);
        }
    };
    for why in &outcome.unsaved_approvals {
        errln!(terminal, "warning: could not remember the approval: {why}");
    }

    if format == OutputFormat::Json {
        terminal.json(&outcome.document());
    } else {
        for r in &outcome.runners {
            let s = &r.stats;
            let by_convention = match r.by_convention {
                0 => String::new(),
                n => format!(", {n} linked by naming convention"),
            };
            outln!(
                terminal,
                "{}: {} entities, {} passed, {} failed, {} skipped ({} report file(s){by_convention})",
                r.name,
                s.entities,
                s.passed,
                s.failed,
                s.skipped,
                r.files
            );
            if let Some(code) = r.exit_code.filter(|c| *c != 0) {
                outln!(
                    terminal,
                    "  the runner exited with status {code}; results of the tests that ran are recorded"
                );
            }
        }
        for d in &outcome.diagnostics {
            outln!(terminal, "{}: {}", d.code, d.message);
            if let Some(hint) = &d.suggestion {
                outln!(terminal, "  hint: {hint}");
            }
        }
        outln!(terminal, "report written: {}", outcome.report.display());
    }
    Exit::Passed
}

/// Ask on the terminal whether the collector's command may run here.
fn ask(collector: &Collector, argv: &[String], root: &Path) -> bool {
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "{} wants to run this command in {}:\n  {}",
        collector.extension,
        root.display(),
        argv.join(" ")
    );
    let _ = write!(err, "Allow it for this project? [y/N] ");
    let _ = err.flush();
    let mut answer = String::new();
    if std::io::stdin().lock().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim(), "y" | "Y" | "yes")
}
