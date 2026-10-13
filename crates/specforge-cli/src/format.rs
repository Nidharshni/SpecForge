use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal, Written};
use crate::terminal::{Terminal, errln, out, outln};
use specforge_common::Diagnostic;
use specforge_formatter::unified_diff;
use specforge_ops::format::{self, Mode, Request};
use specforge_ops::{OpError, OpErrorKind};
use std::path::{Path, PathBuf};

// `specforge format`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Explicit file or directory paths to format
    #[arg()]
    paths: Vec<PathBuf>,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    /// Check formatting without modifying files (exit 1 if unformatted)
    #[arg(long)]
    check: bool,

    /// Show unified diff of formatting changes
    #[arg(long)]
    diff: bool,

    /// Read from stdin, write to stdout
    #[arg(long)]
    stdin: bool,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    /// Run the `specforge format` command.
    ///
    /// Returns the exit, one rule in every mode (write, `--check`,
    /// `--diff`, `--stdin`) and output format:
    /// - 1 when a file could not be read or written, or has a region left
    ///   unformatted (W142); the other files, and the formatted parts, are
    ///   still written or printed;
    /// - 1 under `--check` when a file would change;
    /// - 0 otherwise.
    ///
    /// Under `--format json` it prints the format document `specforge.format`
    /// answers (ADR 0052) with the files it wrote, and nothing on stderr;
    /// `--stdin`, whose output is the formatted text, refuses it.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let project_root = match self.project.root() {
            Ok(root) => root,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        if self.stdin {
            if format == OutputFormat::Json {
                return Refusal::of(format).report(
                    terminal,
                    &OpError::new(
                        OpErrorKind::InvalidInput,
                        OpErrorKind::InvalidInput.as_str(),
                        "--stdin prints the formatted text; it has no JSON form",
                    )
                    .with_suggestion("drop --format json, or name the files to format"),
                );
            }
            return run_stdin(project_root.dir(), self.project.path(), terminal);
        }
        let mode = Mode::of_flags(self.check, self.diff, None);
        let outcome = match format::run(&Request {
            root: &project_root,
            paths: &self.paths,
            mode,
        }) {
            Ok(outcome) => outcome,
            // A config that does not say which files are the sources:
            // nothing was read or written.
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        if format == OutputFormat::Json {
            // The files written, named from the project root as MCP names
            // them; a run that only reports lists none.
            let files_written = mode
                .writes()
                .then(|| outcome.writes().names_under(project_root.dir()));
            terminal.json(&Written {
                document: outcome.document(self.diff),
                files_written,
            });
            return Exit::of_verdict(outcome.ok());
        }
        if outcome.found_nothing() {
            errln!(terminal, "No .spec files found");
            return Exit::Passed;
        }
        for d in &outcome.diagnostics {
            print_diagnostic(d, terminal);
        }
        // A failed file is reported as every operation's failure is
        // (`error[CODE]: …`), its code and kind the OS-given ones.
        // The run's exit is its verdict (`outcome.ok()`), not this report's.
        for failure in &outcome.failures {
            let _ = Refusal::of(OutputFormat::Human).report(terminal, &failure.to_op_error());
        }

        for change in &outcome.changes {
            let shown = change.path.display().to_string();
            if self.diff {
                out!(
                    terminal,
                    "{}",
                    unified_diff(&shown, &change.before, &change.after).diff_text
                );
            } else if change.written || !mode.writes() {
                outln!(terminal, "{shown}");
            }
        }

        if mode == Mode::Write {
            errln!(
                terminal,
                "Formatted {} file(s), {} changed",
                outcome.checked,
                outcome.changes.len()
            );
        }

        Exit::of_verdict(outcome.ok())
    }
}

/// A diagnostic on stderr: `<file>: <message>` when it names a file, the
/// bare message when it is spanned in unnamed text (stdin), else
/// `<severity>[<code>]: <message>` (a configuration file's W141 or E069
/// names its file itself).
fn print_diagnostic(d: &Diagnostic, terminal: &Terminal) {
    match &d.span {
        Some(span) if !span.file.as_str().is_empty() => {
            errln!(terminal, "{}: {}", span.file.as_str(), d.message);
        }
        Some(_) => errln!(terminal, "{}", d.message),
        None => errln!(terminal, "{}[{}]: {}", d.severity, d.code, d.message),
    }
}

/// Format stdin, text of the project at `root` in `dir`, and write it to
/// stdout: it gets the configuration a file in `dir` would. The formatted
/// text is printed even when a region was left unformatted; the exit code
/// is then 1.
fn run_stdin(root: &Path, dir: &Path, terminal: &Terminal) -> Exit {
    let input = match terminal.read_to_string() {
        Ok(input) => input,
        Err(e) => {
            return Refusal::of(OutputFormat::Human).report(
                terminal,
                &OpError::new(
                    OpErrorKind::of_io(&e),
                    "file_unreadable",
                    format!("failed to read stdin: {e}"),
                ),
            );
        }
    };

    let place = format::Place::InProject { root, dir };
    let result = format::document(place, &input, None, None);

    for d in &result.diagnostics {
        print_diagnostic(d, terminal);
    }

    out!(terminal, "{}", result.formatted);
    terminal.flush();

    Exit::of_verdict(result.complete())
}
