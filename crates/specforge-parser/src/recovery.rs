//! Recovery from unclosed strings.
//!
//! Both string kinds may span lines, so an unclosed `"` or `"""` would run
//! to the end of the file, or pair with the next string's opening quote and
//! shift every later pairing, swallowing the blocks after it. The string
//! that was never closed is ended just before the next line that starts a
//! top-level form of the grammar, and parsing resumes there.
//!
//! The text is read through the language's one lexer (`crate::lex`), whose
//! strings are the grammar's: the culprit is the first string it reads as
//! unclosed (no closing quote, a `\` before a line break, or a multi-line
//! pairing whose closing quotes run into text). The scan runs only on files
//! the grammar already rejected; a properly closed multi-line string whose
//! content looks like a block start is never split. The resume line is
//! `lex::next_form_line`'s, where the lexer itself ends an unclosed
//! `"""…"""`, so the LSP's document resumes where the parser does (ADR 0038
//! D1, D6).

use crate::lex::{self, LexemeKind, TRIPLE, lex};

/// A string the recovery ends early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnclosedString {
    /// Byte offset of the opening quote.
    pub open: usize,
    /// The delimiter: `"` or `"""`.
    pub delim: &'static str,
    /// Byte offset where the string is ended: the start of the next
    /// line that starts a top-level form, or the end of the source when none
    /// follows.
    pub end: usize,
}

/// Find the strings to end early, in source order. Empty when no string
/// was left unclosed.
pub(crate) fn unclosed_strings(source: &str) -> Vec<UnclosedString> {
    let mut found = Vec::new();
    let mut pos = 0;
    while let Some((open, delim, top_indent)) = culprit(source, pos) {
        let end =
            lex::next_form_line(source, open + delim.len(), top_indent).unwrap_or(source.len());
        found.push(UnclosedString { open, delim, end });
        if end == source.len() {
            break;
        }
        pos = end;
    }
    found
}

/// The first string of `source[from..]` the grammar does not close, as (the
/// opening quote's offset in `source`, its delimiter, the indentation of the
/// top-level line it sits under). `from` is a line start at brace depth 0.
fn culprit(source: &str, from: usize) -> Option<(usize, &'static str, usize)> {
    let text = &source[from..];
    let mut depth = 0usize;
    let mut top_indent = 0usize;
    let mut previous_end: Option<usize> = None;
    for lexeme in lex(text) {
        let first_on_line = previous_end.is_none_or(|end| text[end..lexeme.start].contains('\n'));
        if first_on_line && depth == 0 && lexeme.kind != LexemeKind::Comment {
            // First on its line: only whitespace precedes it there.
            if let Some(indent) = lex::line_indent(text, lexeme.start) {
                top_indent = indent;
            }
        }
        match lexeme.kind {
            LexemeKind::Punct('{') => depth += 1,
            LexemeKind::Punct('}') => depth = depth.saturating_sub(1),
            LexemeKind::Str { closed: false } => {
                let delim = if lexeme.text(text).starts_with(TRIPLE) {
                    TRIPLE
                } else {
                    "\""
                };
                return Some((from + lexeme.start, delim, top_indent));
            }
            _ => {}
        }
        previous_end = Some(lexeme.end);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_file_needs_no_recovery() {
        let src = "behavior a \"A\" {\n  contract \"\"\"\nbehavior b \"B\" {\n  \"\"\"\n}\n";
        assert!(unclosed_strings(src).is_empty());
    }

    #[test]
    fn an_unclosed_triple_string_ends_before_the_next_block() {
        let src = "behavior a \"A\" {\n  contract \"\"\"\n  text\n}\n\nbehavior b \"B\" {\n}\n";
        let open = src.find("\"\"\"").unwrap();
        let end = src.find("behavior b").unwrap();
        assert_eq!(
            unclosed_strings(src),
            vec![UnclosedString {
                open,
                delim: "\"\"\"",
                end
            }]
        );
    }

    #[test]
    fn a_shifted_title_pairing_is_blamed_on_the_first_string() {
        let src = "behavior a \"A {\n}\n\nbehavior b \"B\" {\n  status done\n}\n";
        let end = src.find("behavior b").unwrap();
        assert_eq!(
            unclosed_strings(src),
            vec![UnclosedString {
                open: 11,
                delim: "\"",
                end
            }]
        );
    }

    #[test]
    fn a_later_string_does_not_take_the_blame() {
        let src = "behavior a \"A {\n  contract \"Oops.\"\n}\n\nbehavior b \"B\" {\n}\n";
        let end = src.find("behavior b").unwrap();
        assert_eq!(
            unclosed_strings(src),
            vec![UnclosedString {
                open: 11,
                delim: "\"",
                end
            }]
        );
    }

    #[test]
    fn text_after_a_one_line_string_is_not_an_unclosed_string() {
        let src = "behavior a \"A\" {\n  status \"done\"x\n}\n\nbehavior b \"B\" {\n}\n";
        assert!(unclosed_strings(src).is_empty());
    }

    #[test]
    fn each_unclosed_string_is_ended() {
        let src = "a x \"X {\n}\nb y \"Y\" {\n  c \"\"\"\n  text\n}\nc z \"Z\" {\n}\n";
        let found = unclosed_strings(src);
        let ends: Vec<usize> = found.iter().map(|u| u.end).collect();
        assert_eq!(
            ends,
            vec![src.find("b y").unwrap(), src.find("c z").unwrap()]
        );
        assert_eq!(found[1].delim, "\"\"\"");
    }

    /// Every corpus file lexes the way the grammar does: all strings closed,
    /// none running into text. Recovery never fires on a valid file anyway
    /// (it waits for a grammar error), but its blame rule relies on this.
    #[test]
    fn no_string_of_the_corpora_runs_into_text() {
        let mut files = Vec::new();
        for corpus in CORPORA {
            collect_specs(&root().join(corpus), &mut files);
        }
        assert!(files.len() > 20, "found only {} corpus files", files.len());
        for file in files {
            let src = std::fs::read_to_string(&file).unwrap();
            for lexeme in lex(&src) {
                let LexemeKind::Str { closed } = lexeme.kind else {
                    continue;
                };
                assert!(
                    closed,
                    "{}: string at byte {} unclosed",
                    file.display(),
                    lexeme.start
                );
                assert!(
                    !lex::runs_into_text(&src, lexeme.end),
                    "{}: string at byte {} runs into text",
                    file.display(),
                    lexeme.start
                );
            }
        }
    }

    const CORPORA: [&str; 3] = ["spec", "integrations/rust/spec", "examples"];

    fn root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn collect_specs(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_specs(&path, out);
            } else if path.extension().is_some_and(|e| e == "spec") {
                out.push(path);
            }
        }
    }

    #[test]
    fn with_no_block_after_it_the_string_runs_to_the_end() {
        let src = "behavior a \"A\" {\n  contract \"oops\n}\n";
        assert_eq!(
            unclosed_strings(src),
            vec![UnclosedString {
                open: 28,
                delim: "\"",
                end: src.len()
            }]
        );
    }

    #[test]
    fn slashes_in_a_scheme_ref_are_not_a_comment() {
        let src = "ref web.page:https://x.io \"T\"\nbehavior a \"A\" {\n}\n";
        assert!(unclosed_strings(src).is_empty());
    }
}
