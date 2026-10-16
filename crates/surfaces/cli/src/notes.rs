//! What a core command writes on stderr beside its document (ADR 0120).
//!
//! A command's stdout is its document (`--format json`), its human rendering, or a fixed format a reader parses
//! (export, schema, model, query, outline). Beside it, on stderr, a command may say two kinds of thing: a **notice**
//! (a coded diagnostic about the run: I016, W053, I020, W097, the registry configuration's W140/I003/W155/W156, a
//! failed registry's R005) and a **note** (prose for a person: `note: …`, `running …`). One rule, by format: a note is
//! written under human output only; a notice is written once, in the document when the document carries it, else
//! here, before the document. No other module decides it: a command hands each line to the verb that says which it is.

use crate::outcome::OutputFormat;
use crate::terminal::{Stream, Terminal};
use specforge_common::Diagnostic;
use std::fmt;

/// Where a command's notes and notices go: its terminal's stderr, by its output format.
#[derive(Clone, Copy)]
#[allow(
    dead_code,
    reason = "the verbs but `beside` are taken up by the next tickets of plan 09"
)]
pub(crate) struct Notes<'t> {
    terminal: &'t Terminal,
    format: OutputFormat,
}

#[allow(
    dead_code,
    reason = "the verbs but `beside` are taken up by the next tickets of plan 09"
)]
impl<'t> Notes<'t> {
    /// The notes of a command whose output is `format`. A command with a fixed format (no `--format human|json`:
    /// export, schema, model, query, outline) passes `OutputFormat::Human`, as it does to `Refusal::of`: nobody
    /// parses its stderr.
    pub(crate) fn new(terminal: &'t Terminal, format: OutputFormat) -> Self {
        Notes { terminal, format }
    }

    /// Notices no document carries (the schema notices of ADR 0111 D1, model's and query's I020, the registry
    /// configuration's, publish's declaration warnings): each `severity[CODE]: message` on stderr, under every
    /// format. Call it before the document (ADR 0111 D8).
    pub(crate) fn beside(&self, notices: &[Diagnostic]) {
        for notice in notices {
            self.line(format_args!("{}", specforge_common::render_plain(notice)));
        }
    }

    /// A warning no document carries and no code names (export's schema cache it could not write, collect's
    /// approval it could not remember): the line on stderr, under every format.
    pub(crate) fn warning(&self, line: fmt::Arguments<'_>) {
        self.line(line);
    }

    /// Notices the document carries (explore's `notices`, search's `diagnostics`, migrate's `structural_differences`):
    /// on stderr under human output; under JSON the document says them, and nothing is written.
    pub(crate) fn carried(&self, notices: &[Diagnostic]) {
        if self.format == OutputFormat::Human {
            self.beside(notices);
        }
    }

    /// A line for a person: a `note: …`, an announcement, or the human rendering of a fact the document carries
    /// (analyze's W097 lines and failed gate, migrate's hook failures and restore): on stderr under human output,
    /// nothing under JSON.
    pub(crate) fn person(&self, line: fmt::Arguments<'_>) {
        if self.format == OutputFormat::Human {
            self.line(line);
        }
    }

    fn line(&self, line: fmt::Arguments<'_>) {
        self.terminal.write_line(Stream::Err, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Facts;
    use specforge_common::codes;

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "under --format json a note is never written and a notice is written once: in the document when it carries it, else on stderr"
    )]
    fn each_verb_writes_by_its_format() {
        let notice = [Diagnostic::new(codes::W140, "careful")];
        type Verb = fn(&Notes<'_>, &[Diagnostic]);
        let verbs: [(&str, Verb, [bool; 2]); 4] = [
            ("beside", |n, d| n.beside(d), [true, true]),
            (
                "warning",
                |n, _| n.warning(format_args!("careful")),
                [true, true],
            ),
            ("carried", |n, d| n.carried(d), [true, false]),
            (
                "person",
                |n, _| n.person(format_args!("careful")),
                [true, false],
            ),
        ];
        for (name, verb, [human, json]) in verbs {
            for (format, written) in [(OutputFormat::Human, human), (OutputFormat::Json, json)] {
                let (terminal, capture) = Terminal::captured("", Facts::PIPED);
                verb(&Notes::new(&terminal, format), &notice);
                assert_eq!(
                    capture.stderr().contains("careful"),
                    written,
                    "{name} under {format:?}: {}",
                    capture.stderr()
                );
                assert_eq!(capture.stdout(), "", "{name}: nothing on stdout");
            }
        }
    }

    #[test]
    fn a_notice_is_rendered_plain() {
        let notice = Diagnostic::new(codes::W140, "careful");
        let (terminal, capture) = Terminal::captured("", Facts::PIPED);

        Notes::new(&terminal, OutputFormat::Json).beside(std::slice::from_ref(&notice));

        assert_eq!(
            capture.stderr(),
            format!("{}\n", specforge_common::render_plain(&notice))
        );
    }
}
