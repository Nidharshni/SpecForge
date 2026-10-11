//! How a core command ends (ADR 0029): its exit code, and what it prints
//! when its operation refuses. Extension commands keep their own contract
//! (ADR 0011).
//!
//! A command ends in one of three ways. Its run passed ([`Exit::Passed`]).
//! Its run's verdict failed, or its operation refused ([`Exit::Failed`]).
//! It could not judge the project ([`Exit::Unjudged`]). A refusal is
//! printed by [`Refusal::report`] and nowhere else: `error[CODE]: message`
//! on stderr, or under JSON output the error document on stdout.

use crate::OutputFormat;
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

    /// Report `error`: under JSON output the error document on stdout and
    /// nothing on stderr (ADR 0011), under human output [`error_lines`] on
    /// stderr. Returns the exit.
    pub(crate) fn report(self, error: &OpError) -> Exit {
        match self.format {
            OutputFormat::Json => {
                let document = error_document(error, self.root);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&document).expect("serialize JSON output")
                );
            }
            OutputFormat::Human => eprint!("{}", error_lines(error, self.root)),
        }
        self.exit
    }

    /// Report a failure under the catalogued error `code`, as
    /// [`Self::report`].
    pub(crate) fn coded(self, code: Code, message: impl Into<String>) -> Exit {
        self.report(&OpError::diagnostic(code, message))
    }
}

/// `value` as pretty JSON on stdout: every `--format json` document.
pub(crate) fn print_json(value: &impl serde::Serialize) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("serialize JSON output")
    );
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
    use specforge_common::codes;
    use specforge_ops::RunVerdict;

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
        // Both formats end the same way; only the stream differs.
        for format in [OutputFormat::Human, OutputFormat::Json] {
            assert_eq!(Refusal::of(format).report(&error).code(), 1);
            assert_eq!(Refusal::measuring(format).report(&error).code(), 2);
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
