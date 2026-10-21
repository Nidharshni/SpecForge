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
use specforge_common::{Code, ProjectPath};
use specforge_ops::{OpError, OpErrorKind, Writes, Wrote};
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

/// The `--format human|json` flag every core command with a JSON form takes
/// (`#[command(flatten)]`; `trace` declares its own, json by default).
#[derive(clap::Args, Clone, Copy, Debug)]
pub(crate) struct OutputArg {
    /// Output format: human or json
    #[arg(long, default_value = "human")]
    pub(crate) format: OutputFormat,
}

impl OutputArg {
    /// The option's id and long name.
    pub(crate) const NAME: &'static str = "format";

    /// The option as clap declares it: the one declaration, the derive's, for a command line built at run time
    /// (`new`'s), as [`crate::project::ProjectArg::arg`] is `--path`.
    pub(crate) fn arg() -> clap::Arg {
        use clap::Args as _;
        Self::augment_args(clap::Command::new("output"))
            .get_arguments()
            .find(|argument| argument.get_id() == Self::NAME)
            .cloned()
            .expect("OutputArg declares --format")
    }
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

    /// A fixed-format command's refusal (ADR 0120 D1, ADR 0029 D5c): export, schema, model, query, outline,
    /// completions and `format --stdin` print a document a reader parses, or nothing. Their refusal is
    /// `error[CODE]` on stderr whatever format the document is in, and stdout stays empty. [`Exit::Failed`].
    pub(crate) fn fixed() -> Self {
        Self::of(OutputFormat::Human)
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
    pub(crate) fn report(self, terminal: &Terminal, error: &impl AsRef<OpError>) -> Exit {
        let error = error.as_ref();
        match self.format {
            OutputFormat::Json => terminal.json(&error_document(error, self.root)),
            OutputFormat::Human => err!(terminal, "{}", error_lines(error, self.root)),
        }
        self.exit
    }

    /// Report a failure under the catalogued error `code`, as
    /// [`Self::report`].
    pub(crate) fn coded(
        self,
        terminal: &Terminal,
        kind: OpErrorKind,
        code: Code,
        message: impl Into<String>,
    ) -> Exit {
        self.report(terminal, &OpError::coded(kind, code, message))
    }
}

/// A document and what the command wrote (ADR 0022 D6, ADR 0162): no `files_written` for a preview, every file a
/// run wrote otherwise (`[]` when none), as MCP's reply.
#[derive(Debug, serde::Serialize)]
pub(crate) struct Written<D> {
    #[serde(flatten)]
    document: D,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_written: Option<Vec<ProjectPath>>,
}

impl<D> Written<D> {
    /// `document` with what a mutation that can preview wrote, named from `root`.
    pub(crate) fn of(document: D, wrote: &Wrote, root: &Path) -> Self {
        Self {
            document,
            files_written: wrote.named(Some(root)),
        }
    }

    /// `document` with what a mutation that cannot preview wrote (init, a rollback).
    pub(crate) fn ran(document: D, writes: &Writes, root: &Path) -> Self {
        Self {
            document,
            files_written: Some(writes.named(Some(root))),
        }
    }
}

/// `{error, code}`, `suggestion` when the refusal has one, `files_written` when it left files
/// written ([`OpError::writes`], named from `root` when given): a refusal printed under
/// `--format json`.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ErrorDocument<'a> {
    error: &'a str,
    code: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_written: Option<Vec<ProjectPath>>,
}

/// The document a refusal prints under `--format json`.
pub(crate) fn error_document<'a>(error: &'a OpError, root: Option<&Path>) -> ErrorDocument<'a> {
    ErrorDocument {
        error: &error.message,
        code: &error.code,
        suggestion: error.suggestion.as_deref(),
        files_written: (!error.writes.is_empty()).then(|| error.writes.named(root)),
    }
}

/// `error[CODE]: message`, `  hint: suggestion` when there is one,
/// `  wrote: file` per file left written; each line newline-terminated.
pub(crate) fn error_lines(error: &OpError, root: Option<&Path>) -> String {
    let mut lines = format!("error[{}]: {}\n", error.code, error.message);
    if let Some(suggestion) = &error.suggestion {
        lines.push_str(&format!("  hint: {suggestion}\n"));
    }
    for file in error.writes.named(root) {
        lines.push_str(&format!("  wrote: {file}\n"));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Facts;
    use specforge_common::codes;
    use specforge_ops::{RunVerdict, Writes};

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "under --format json a refusal is the error document on stdout and nothing on stderr"
    )]
    fn a_refusal_writes_to_the_stream_its_format_names() {
        let error = OpError::coded(
            specforge_ops::OpErrorKind::EntityNotFound,
            codes::E003,
            "gone",
        );

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

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "a fixed-format command (export, schema, model, query, outline) refuses on stderr whatever its --format names, with nothing on stdout"
    )]
    fn a_fixed_refusal_is_on_stderr() {
        let error = OpError::coded(
            specforge_ops::OpErrorKind::EntityNotFound,
            codes::E003,
            "gone",
        );
        let (terminal, capture) = Terminal::captured("", Facts::PIPED);
        assert_eq!(Refusal::fixed().report(&terminal, &error), Exit::Failed);
        assert_eq!(capture.stdout(), "");
        assert_eq!(capture.stderr(), "error[E003]: gone\n");
    }

    fn failed_after_writing() -> OpError {
        OpError::coded(
            specforge_ops::OpErrorKind::Internal,
            codes::E032,
            "failed to write specforge.lock",
        )
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
            error_lines(
                &OpError::coded(
                    specforge_ops::OpErrorKind::EntityNotFound,
                    codes::E003,
                    "gone"
                ),
                None
            ),
            "error[E003]: gone\n"
        );
    }

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "under --format json a refusal is the error document on stdout and nothing on stderr"
    )]
    fn error_document_carries_files_written_only_when_written() {
        let json = |document: ErrorDocument| serde_json::to_value(document).unwrap();
        let plain = OpError::coded(
            specforge_ops::OpErrorKind::EntityNotFound,
            codes::E003,
            "gone",
        );
        assert_eq!(
            json(error_document(&plain, None)),
            serde_json::json!({"error": "gone", "code": "E003"})
        );
        let hinted = OpError::coded(
            specforge_ops::OpErrorKind::EntityNotFound,
            codes::E003,
            "gone",
        )
        .with_suggestion("look again");
        assert_eq!(
            json(error_document(&hinted, None)),
            serde_json::json!({"error": "gone", "code": "E003", "suggestion": "look again"})
        );

        let failed = failed_after_writing();
        let written = json(error_document(&failed, Some(Path::new("/p"))));
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
        let error = OpError::coded(
            specforge_ops::OpErrorKind::EntityNotFound,
            codes::E003,
            "gone",
        );
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
