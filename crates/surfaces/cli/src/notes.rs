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
use specforge_ops::Noticed;
use std::fmt;

/// Where a command's notes and notices go: its terminal's stderr, by its output format.
#[derive(Clone, Copy)]
pub(crate) struct Notes<'t> {
    terminal: &'t Terminal,
    format: OutputFormat,
}

impl<'t> Notes<'t> {
    /// The notes of a command whose output is `format`.
    pub(crate) fn new(terminal: &'t Terminal, format: OutputFormat) -> Self {
        Notes { terminal, format }
    }

    /// The notes of a fixed-format command (see [`crate::outcome::Refusal::fixed`]): its notices go on stderr.
    pub(crate) fn fixed(terminal: &'t Terminal) -> Self {
        Self::new(terminal, OutputFormat::Human)
    }

    /// Notices no document carries (the schema notices of ADR 0111 D1, model's and query's I020, the registry
    /// configuration's, publish's declaration warnings): each `severity[CODE]: message` on stderr, under every
    /// format. Call it before the document (ADR 0111 D8).
    pub(crate) fn beside(&self, notices: &[Diagnostic]) {
        for notice in notices {
            self.line(format_args!("{}", specforge_common::render_plain(notice)));
        }
    }

    /// A read's notices, which its document does not carry (ADR 0111 D1, ADR 0120 D1): each on stderr under every
    /// format, before the document. The read's value, to print (ADR 0130).
    pub(crate) fn noticed<T>(&self, read: Noticed<T>) -> T {
        let (value, notices) = read.into_value_and_notices();
        self.beside(&notices);
        value
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
    /// (analyze's W097 lines and failed gate, migrate's failure): on stderr under human output,
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

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "under --format json a note is never written and a notice is written once: in the document when it carries it, else on stderr"
    )]
    fn a_read_is_written_beside_and_handed_back() {
        let notice = Diagnostic::new(codes::W053, "breaking");
        for format in [OutputFormat::Human, OutputFormat::Json] {
            let (terminal, capture) = Terminal::captured("", Facts::PIPED);
            let value =
                Notes::new(&terminal, format).noticed(Noticed::new(7, vec![notice.clone()]));
            assert_eq!(value, 7);
            assert_eq!(
                capture.stderr(),
                format!("{}\n", specforge_common::render_plain(&notice)),
                "{format:?}"
            );
            assert_eq!(capture.stdout(), "");
        }
    }

    /// ADR 0130 D2: only the notes open a noticed read, so a command cannot print a read's
    /// document without its notices.
    #[test]
    fn only_the_notes_open_a_noticed_read() {
        let elsewhere: Vec<String> = sources()
            .into_iter()
            .filter(|(name, text)| name != "notes.rs" && text.contains("into_value_and_notices("))
            .map(|(name, _)| name)
            .collect();
        assert!(
            elsewhere.is_empty(),
            "into_value_and_notices outside notes: {elsewhere:?}"
        );
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

    /// Every `.rs` under `src`, with its path relative to `src`.
    fn sources() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    found.push(path);
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut paths = Vec::new();
        walk(&src, &mut paths);
        paths
            .into_iter()
            .map(|p| {
                let name = p.strip_prefix(&src).unwrap().display().to_string();
                (name, std::fs::read_to_string(&p).unwrap())
            })
            .collect()
    }

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "under --format json a note is never written and a notice is written once: in the document when it carries it, else on stderr"
    )]
    fn only_the_notes_render_a_notice() {
        let mut elsewhere = Vec::new();
        let mut hand_gated = Vec::new();
        for (name, text) in sources() {
            if !matches!(
                name.as_str(),
                "notes.rs" | "extension_command.rs" | "extension_command/tests.rs"
            ) && text.contains("render_plain(")
            {
                elsewhere.push(name.clone());
            }
            let lines: Vec<&str> = text.lines().collect();
            for (at, line) in lines.iter().enumerate() {
                let gate = line.trim_start().starts_with("if ")
                    && (line.contains("!= OutputFormat::Json")
                        || line.contains("== OutputFormat::Human"));
                let writes = lines[at + 1..lines.len().min(at + 3)]
                    .iter()
                    .any(|l| l.contains("errln!(") || l.contains("err!("));
                if gate && writes {
                    hand_gated.push(format!("{name}:{}", at + 1));
                }
            }
        }
        assert!(
            elsewhere.is_empty(),
            "render_plain outside notes: {elsewhere:?}"
        );
        assert!(
            hand_gated.is_empty(),
            "a hand gate on stderr: {hand_gated:?}"
        );
    }
}
