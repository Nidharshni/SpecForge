//! How a core command ends (ADR 0029): its exit code, and what it prints
//! when its operation refuses. Extension commands keep their own contract
//! (ADR 0011).
//!
//! A command ends in one of three ways. Its run passed ([`Exit::Passed`]).
//! Its run's verdict failed, or its operation refused ([`Exit::Failed`]).
//! It could not judge the project ([`Exit::Unjudged`]). A refusal is
//! printed by [`Refusal::report`] and nowhere else: `error[CODE]: message`
//! on stderr, or under JSON output the error document on stdout.

use crate::terminal::{Terminal, err};
use specforge_common::Code;
use specforge_ops::{OpError, Writes};
use std::path::Path;

/// The process exit code: one table for every core command. A core
/// command's `run` returns it; `main` alone turns it into a code.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Exit {
    /// 0: the command did what it was asked, and its run's verdict, if it
    /// has one, passed.
    Passed,
    /// 1: the run's verdict failed (check found an error, format --check a
    /// change, migrate rolled back, analyze an error finding or E048), or
    /// the operation refused.
    Failed,
    /// 2: the command could not judge the project: the command line was
    /// refused (clap, an extension command's arg rule), or a measuring
    /// command (`stats`, `analyze`) refused, since it cannot read what it
    /// measures against (its report, its passes, its gate's inputs).
    Unjudged,
}

impl Exit {
    /// [`Exit::Passed`] when `ok`, else [`Exit::Failed`].
    pub(crate) const fn of_verdict(ok: bool) -> Self {
        Self::of(specforge_ops::RunVerdict::of(ok))
    }

    /// The exit of an operation's run verdict.
    pub(crate) const fn of(verdict: specforge_ops::RunVerdict) -> Self {
        match verdict {
            specforge_ops::RunVerdict::Passed => Self::Passed,
            specforge_ops::RunVerdict::Failed => Self::Failed,
            specforge_ops::RunVerdict::Unjudged => Self::Unjudged,
        }
    }

    pub(crate) const fn code(self) -> i32 {
        match self {
            Self::Passed => 0,
            Self::Failed => 1,
            Self::Unjudged => 2,
        }
    }
}

/// Human-readable vs machine-readable output: what `--format human|json`
/// asks for.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Human,
    Json,
}

impl OutputFormat {
    /// How a publisher key change is decided for this output: `--yes`
    /// accepts it; JSON output can't ask anyone, so it refuses; a terminal
    /// is asked.
    pub(crate) fn trust(self, assume_yes: bool) -> specforge_ops::extension::Trust {
        use specforge_ops::extension::Trust;
        match (assume_yes, self) {
            (true, _) => Trust::AssumeYes,
            (false, OutputFormat::Json) => Trust::Refuse,
            (false, OutputFormat::Human) => Trust::Prompt,
        }
    }
}

/// The `--format human|json` flag every core command with a JSON form takes
/// (`#[command(flatten)]`; `trace` declares its own, json by default).
#[derive(clap::Args, Clone, Copy, Debug)]
pub(crate) struct OutputArg {
    /// Output format: human or json
    #[arg(long, default_value = "human")]
    pub(crate) format: OutputFormat,
}

/// How one command reports its operation's refusal: the output format, the
/// root the files a failed operation left written are named from, and the
/// exit the refusal costs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Refusal<'a> {
    format: OutputFormat,
    root: Option<&'a Path>,
    exit: Exit,
}

impl<'a> Refusal<'a> {
    /// A command's refusal: [`Exit::Failed`].
    pub(crate) fn of(format: OutputFormat) -> Self {
        Self {
            format,
            root: None,
            exit: Exit::Failed,
        }
    }

    /// A measuring command's refusal (`stats`, `analyze`):
    /// [`Exit::Unjudged`].
    pub(crate) fn measuring(format: OutputFormat) -> Self {
        Self {
            exit: Exit::Unjudged,
            ..Self::of(format)
        }
    }

    /// The files the failed operation left written are named from `root`.
    pub(crate) fn at(self, root: &'a Path) -> Self {
        Self {
            root: Some(root),
            ..self
        }
    }

    /// Report `error` at `terminal`: under JSON output the error document
    /// on stdout and nothing on stderr (ADR 0011), under human output
    /// [`error_lines`] on stderr. Returns the exit.
    pub(crate) fn report(self, terminal: &Terminal, error: &OpError) -> Exit {
        match self.format {
            OutputFormat::Json => terminal.json(&error_document(error, self.root)),
            OutputFormat::Human => err!(terminal, "{}", error_lines(error, self.root)),
        }
        self.exit
    }

    /// Report a failure under the catalogued error `code`, as
    /// [`Self::report`].
    pub(crate) fn coded(self, terminal: &Terminal, code: Code, message: impl Into<String>) -> Exit {
        self.report(terminal, &OpError::diagnostic(code, message))
    }
}

/// A document and the files the command wrote (ADR 0022 D6): what `add`,
/// `remove`, `init` and `migrate` print under `--format json`. A dry run
/// lists none (`files_written` absent), as MCP's preview.
#[derive(Debug, serde::Serialize)]
pub(crate) struct Written<D> {
    #[serde(flatten)]
    pub(crate) document: D,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) files_written: Option<Vec<String>>,
}

impl<D> Written<D> {
    /// `document` with the files a command wrote.
    pub(crate) fn of(document: D, files_written: Vec<String>) -> Self {
        Self {
            document,
            files_written: Some(files_written),
        }
    }

    /// `document` of a preview: no `files_written`.
    #[allow(dead_code)]
    pub(crate) fn preview(document: D) -> Self {
        Self {
            document,
            files_written: None,
        }
    }
}

/// `{"error", "code", "suggestion"}`, and `files_written` when the
/// operation left files written ([`OpError::writes`], named from `root`
/// when given).
pub(crate) fn error_document(error: &OpError, root: Option<&Path>) -> serde_json::Value {
    let mut document = serde_json::json!({
        "error": error.message,
        "code": error.code,
        "suggestion": error.suggestion,
    });
    if !error.writes.is_empty() {
        document["files_written"] = serde_json::json!(files_written(&error.writes, root));
    }
    document
}

/// `error[CODE]: message`, `  hint: suggestion` when there is one,
/// `  wrote: file` per file left written; each line newline-terminated.
pub(crate) fn error_lines(error: &OpError, root: Option<&Path>) -> String {
    let mut lines = format!("error[{}]: {}\n", error.code, error.message);
    if let Some(suggestion) = &error.suggestion {
        lines.push_str(&format!("  hint: {suggestion}\n"));
    }
    for file in files_written(&error.writes, root) {
        lines.push_str(&format!("  wrote: {file}\n"));
    }
    lines
}

/// The files `writes` names, as a command lists them: relative to `root`
/// (absolute outside it), sorted.
pub(crate) fn files_written(writes: &Writes, root: Option<&Path>) -> Vec<String> {
    match root {
        Some(root) => writes.names_under(root),
        None => writes
            .paths()
            .map(|path| path.display().to_string())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Facts;
    use specforge_common::codes;
    use specforge_ops::RunVerdict;

    #[test]
    fn trust_is_yes_then_what_the_output_can_ask() {
        use specforge_ops::extension::Trust;
        assert_eq!(OutputFormat::Human.trust(true), Trust::AssumeYes);
        assert_eq!(OutputFormat::Json.trust(true), Trust::AssumeYes);
        assert_eq!(OutputFormat::Json.trust(false), Trust::Refuse);
        assert_eq!(OutputFormat::Human.trust(false), Trust::Prompt);
    }

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "under --format json a refusal is the error document on stdout and nothing on stderr"
    )]
    fn a_refusal_writes_to_the_stream_its_format_names() {
        let error = OpError::diagnostic(codes::E003, "gone");

        let (terminal, capture) = Terminal::captured("", Facts::PIPED);
        assert_eq!(
            Refusal::of(OutputFormat::Json).report(&terminal, &error),
            Exit::Failed
        );
        assert!(capture.stdout().contains("\"code\": \"E003\""));
        assert_eq!(capture.stderr(), "");

        let (terminal, capture) = Terminal::captured("", Facts::PIPED);
        assert_eq!(
            Refusal::of(OutputFormat::Human).report(&terminal, &error),
            Exit::Failed
        );
        assert_eq!(capture.stdout(), "");
        assert_eq!(capture.stderr(), "error[E003]: gone\n");
    }

    fn failed_after_writing() -> OpError {
        OpError::diagnostic(codes::E032, "failed to write specforge.lock")
            .with_suggestion("check the permissions")
            .with_writes(Writes::from_iter(["/p/specforge.json", "/p/spec/a.spec"]))
    }

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "an operation's refusal is error[CODE]: message, its hint and the files it left written, on stderr"
    )]
    fn error_lines_name_code_hint_and_writes() {
        let error = failed_after_writing();

        assert_eq!(
            error_lines(&error, Some(Path::new("/p"))),
            "error[E032]: failed to write specforge.lock\n  hint: check the permissions\n  wrote: spec/a.spec\n  wrote: specforge.json\n"
        );
        // Nothing to add: the code and the message alone.
        assert_eq!(
            error_lines(&OpError::diagnostic(codes::E003, "gone"), None),
            "error[E003]: gone\n"
        );
    }

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "under --format json a refusal is the error document on stdout and nothing on stderr"
    )]
    fn error_document_carries_files_written_only_when_written() {
        let plain = error_document(&OpError::diagnostic(codes::E003, "gone"), None);
        assert_eq!(
            plain,
            serde_json::json!({"error": "gone", "code": "E003", "suggestion": null})
        );

        let written = error_document(&failed_after_writing(), Some(Path::new("/p")));
        assert_eq!(written["code"], "E032");
        assert_eq!(written["suggestion"], "check the permissions");
        assert_eq!(
            written["files_written"],
            serde_json::json!(["spec/a.spec", "specforge.json"])
        );
    }

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "a passed run exits 0, a failed verdict or a refusal 1, a refusal of a measuring command 2"
    )]
    fn the_exit_table() {
        assert_eq!(Exit::of_verdict(true).code(), 0);
        assert_eq!(Exit::of_verdict(false).code(), 1);
        assert_eq!(Exit::Unjudged.code(), 2);
        let error = OpError::diagnostic(codes::E003, "gone");
        let (terminal, _) = Terminal::captured("", Facts::PIPED);
        // Both formats end the same way; only the stream differs.
        for format in [OutputFormat::Human, OutputFormat::Json] {
            assert_eq!(Refusal::of(format).report(&terminal, &error).code(), 1);
            assert_eq!(
                Refusal::measuring(format).report(&terminal, &error).code(),
                2
            );
        }
        for (verdict, code) in [
            (RunVerdict::Passed, 0),
            (RunVerdict::Failed, 1),
            (RunVerdict::Unjudged, 2),
        ] {
            assert_eq!(Exit::of(verdict).code(), code);
        }
    }
}
