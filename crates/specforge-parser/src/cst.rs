//! A `.spec` source read into concrete syntax, as every SpecForge reader of
//! the CST reads it: the parser's walk to the AST and the formatter.
//!
//! The grammar's tree of the whole text is taken as it is when it parsed, or
//! when it starts a form at every line the lexer reads as a form start
//! (`lex::form_starts`) and no string was left unclosed. Otherwise the text
//! is cut into pieces before each form start the tree missed and before the
//! first form start after each unclosed string, and each piece is parsed
//! alone. A piece holding a string left unclosed is parsed with the string's
//! closing delimiter appended, so its own form reads as far as the grammar
//! can, and the forms after it are untouched (ADR 0038 D2, ADR 0063 D1).
//! An editor's document reads the same form starts (`specforge_lsp`'s
//! `Syntax`), so the graph, the formatter and the editor see the same forms.
//!
//! The text is read through the language's one lexer (`crate::lex`), whose
//! strings are the grammar's: the culprit of an unclosed string is the first
//! string it reads as unclosed (no closing quote, a `\` before a line break,
//! or a multi-line pairing whose closing quotes run into text). The lexing
//! runs only on sources the grammar already rejected; a properly closed
//! multi-line string whose content looks like a block start is never split.

use crate::lex::{self, LexemeKind, TRIPLE, lex};
use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};
use tree_sitter::{LanguageError, Node, Parser, Tree};
use tree_sitter_specforge::kind;

/// A source's concrete syntax: one or more pieces, in source order, each a
/// run of whole top-level forms with its own tree.
pub struct Cst<'s> {
    whole: Tree,
    pieces: Vec<Piece<'s>>,
}

/// One piece of a source and the tree parsed from it.
pub struct Piece<'s> {
    start: usize,
    row: usize,
    source_len: usize,
    text: Cow<'s, str>,
    tree: Tree,
    unclosed: Option<UnclosedString>,
}

/// A string left unclosed, ended at the end of its piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnclosedString {
    /// Byte offset of the opening quote, in the source.
    pub open: usize,
    /// The delimiter: `"` or `"""`.
    pub delim: &'static str,
}

/// Why a source could not be read into a tree at all.
#[derive(Debug)]
pub enum CstError {
    /// The grammar did not load into the parser.
    Language(LanguageError),
    /// Tree-sitter returned no tree.
    NoTree,
}

impl<'s> Cst<'s> {
    /// Read `source`, reusing `old_tree` (already edited to `source`) for
    /// the whole text's parse.
    pub fn read(source: &'s str, old_tree: Option<&Tree>) -> Result<Cst<'s>, CstError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_specforge::LANGUAGE.into())
            .map_err(CstError::Language)?;
        let whole = parser.parse(source, old_tree).ok_or(CstError::NoTree)?;
        let plan = if whole.root_node().has_error() {
            cuts(source, &whole)
        } else {
            None
        };
        let Some(plan) = plan else {
            let piece = Piece {
                start: 0,
                row: 0,
                source_len: source.len(),
                text: Cow::Borrowed(source),
                tree: whole.clone(),
                unclosed: None,
            };
            return Ok(Cst {
                whole,
                pieces: vec![piece],
            });
        };
        let mut pieces = Vec::with_capacity(plan.len());
        for (start, end, unclosed) in plan {
            let text = match unclosed {
                Some(u) => Cow::Owned(format!("{}{}", &source[start..end], u.delim)),
                None => Cow::Borrowed(&source[start..end]),
            };
            let tree = parser.parse(text.as_ref(), None).ok_or(CstError::NoTree)?;
            pieces.push(Piece {
                start,
                row: source[..start].matches('\n').count(),
                source_len: end - start,
                text,
                tree,
                unclosed,
            });
        }
        Ok(Cst { whole, pieces })
    }

    /// The pieces, in source order: one when nothing was cut.
    pub fn pieces(&self) -> &[Piece<'s>] {
        &self.pieces
    }

    /// The whole text's tree, for the next incremental parse (the pieces'
    /// trees are not reusable: their texts are not the source).
    pub fn into_tree(self) -> Tree {
        self.whole
    }
}

impl<'s> Piece<'s> {
    /// Its first byte in the source.
    pub fn start(&self) -> usize {
        self.start
    }

    /// The source row (0-based) it starts on.
    pub fn row(&self) -> usize {
        self.row
    }

    /// The text its tree was parsed from: `source_len` bytes of the source
    /// from `start`, then the unclosed string's delimiter, if any.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// How many bytes of `text` are source.
    pub fn source_len(&self) -> usize {
        self.source_len
    }

    /// Its tree's root (a `source_file`).
    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    /// The string left unclosed that this piece ends, if any.
    pub fn unclosed(&self) -> Option<UnclosedString> {
        self.unclosed
    }

    /// Whether `node`, of this piece's tree, holds the unclosed string's
    /// opening quote: the form recovery broke, kept as written.
    pub fn holds_unclosed(&self, node: Node<'_>) -> bool {
        self.unclosed.is_some_and(|u| {
            let at = u.open - self.start;
            node.start_byte() <= at && at < node.end_byte()
        })
    }
}

/// Where recovery cuts `source`, as (start, end, the string the piece
/// ends): `None` when the tree starts a form at every form start and no
/// string was left unclosed. A cut is the start of the line of a form start
/// the tree did not start a top-level node at, or of the first form start
/// after an unclosed string.
fn cuts(source: &str, tree: &Tree) -> Option<Vec<(usize, usize, Option<UnclosedString>)>> {
    let lexemes = lex(source);
    let starts: Vec<usize> = lex::form_starts(source, &lexemes)
        .into_iter()
        .map(|i| lexemes[i].start)
        .collect();
    let root = tree.root_node();
    let mut cursor = root.walk();
    let started: HashSet<usize> = root
        .children(&mut cursor)
        .filter(|n| !n.is_error() && !n.is_missing() && n.kind() != kind::COMMENT)
        .map(|n| n.start_byte())
        .collect();
    let unclosed: Vec<UnclosedString> = lexemes
        .iter()
        .filter(|l| l.kind == LexemeKind::Str { closed: false })
        .map(|l| UnclosedString {
            open: l.start,
            delim: if l.text(source).starts_with(TRIPLE) {
                TRIPLE
            } else {
                "\""
            },
        })
        .collect();
    let mut at: BTreeSet<usize> = starts
        .iter()
        .filter(|s| !started.contains(s))
        .copied()
        .collect();
    for string in &unclosed {
        if let Some(&next) = starts.iter().find(|&&s| s > string.open) {
            at.insert(next);
        }
    }
    if at.is_empty() && unclosed.is_empty() {
        return None;
    }
    let line_start = |byte: usize| source[..byte].rfind('\n').map_or(0, |nl| nl + 1);
    let mut bounds: Vec<usize> = at.into_iter().map(line_start).filter(|&b| b > 0).collect();
    bounds.dedup();
    let mut plan = Vec::new();
    let mut start = 0;
    for end in bounds.into_iter().chain(std::iter::once(source.len())) {
        if end > start {
            // The first unclosed string in the piece is the one the grammar
            // runs to its end (ADR 0038 D2's culprit).
            let string = unclosed
                .iter()
                .find(|u| (start..end).contains(&u.open))
                .copied();
            plan.push((start, end, string));
            start = end;
        }
    }
    Some(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(src: &str) -> Vec<(usize, Option<UnclosedString>)> {
        Cst::read(src, None)
            .unwrap()
            .pieces()
            .iter()
            .map(|p| (p.start(), p.unclosed()))
            .collect()
    }

    fn unclosed(src: &str) -> Vec<UnclosedString> {
        pieces(src).into_iter().filter_map(|(_, u)| u).collect()
    }

    #[test]
    fn a_closed_file_needs_no_recovery() {
        let src = "behavior a \"A\" {\n  contract \"\"\"\nbehavior b \"B\" {\n  \"\"\"\n}\n";
        assert_eq!(pieces(src), vec![(0, None)]);
    }

    #[test]
    fn a_file_that_parses_is_one_borrowed_piece() {
        let src = "behavior a \"A\" {\n  status done\n}\n";
        let cst = Cst::read(src, None).unwrap();
        assert_eq!(cst.pieces().len(), 1);
        assert!(matches!(cst.pieces()[0].text, Cow::Borrowed(_)));
    }

    #[test]
    fn an_unclosed_triple_string_ends_before_the_next_block() {
        let src = "behavior a \"A\" {\n  contract \"\"\"\n  text\n}\n\nbehavior b \"B\" {\n}\n";
        let open = src.find("\"\"\"").unwrap();
        let next = src.find("behavior b").unwrap();
        assert_eq!(
            pieces(src),
            vec![
                (
                    0,
                    Some(UnclosedString {
                        open,
                        delim: "\"\"\""
                    })
                ),
                (next, None)
            ]
        );
    }

    #[test]
    fn a_shifted_title_pairing_is_blamed_on_the_first_string() {
        let src = "behavior a \"A {\n}\n\nbehavior b \"B\" {\n  status done\n}\n";
        let next = src.find("behavior b").unwrap();
        assert_eq!(
            pieces(src),
            vec![
                (
                    0,
                    Some(UnclosedString {
                        open: 11,
                        delim: "\""
                    })
                ),
                (next, None)
            ]
        );
    }

    #[test]
    fn a_later_string_does_not_take_the_blame() {
        let src = "behavior a \"A {\n  contract \"Oops.\"\n}\n\nbehavior b \"B\" {\n}\n";
        let next = src.find("behavior b").unwrap();
        assert_eq!(
            pieces(src),
            vec![
                (
                    0,
                    Some(UnclosedString {
                        open: 11,
                        delim: "\""
                    })
                ),
                (next, None)
            ]
        );
    }

    #[test]
    fn text_after_a_one_line_string_is_not_an_unclosed_string() {
        let src = "behavior a \"A\" {\n  status \"done\"x\n}\n\nbehavior b \"B\" {\n}\n";
        assert!(unclosed(src).is_empty());
    }

    #[test]
    fn each_unclosed_string_is_ended() {
        let src = "a x \"X {\n}\nb y \"Y\" {\n  c \"\"\"\n  text\n}\nc z \"Z\" {\n}\n";
        let found = pieces(src);
        let starts: Vec<usize> = found.iter().map(|(start, _)| *start).collect();
        assert_eq!(
            starts,
            vec![0, src.find("b y").unwrap(), src.find("c z").unwrap()]
        );
        assert_eq!(found[1].1.map(|u| u.delim), Some("\"\"\""));
    }

    /// Every corpus file lexes the way the grammar does: all strings closed,
    /// none running into text. Recovery never fires on a valid file anyway
    /// (it waits for a grammar error), but its blame rule relies on this.
    #[test]
    fn no_string_of_the_corpora_runs_into_text() {
        let mut files = Vec::new();
        for corpus in CORPORA {
            collect_specs(&repo_root().join(corpus), &mut files);
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
    use specforge_repo_root::repo_root;

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
            pieces(src),
            vec![(
                0,
                Some(UnclosedString {
                    open: 28,
                    delim: "\""
                })
            )]
        );
        let cst = Cst::read(src, None).unwrap();
        assert_eq!(cst.pieces()[0].source_len(), src.len());
        assert_eq!(cst.pieces()[0].text(), format!("{src}\""));
    }

    #[test]
    fn slashes_in_a_scheme_ref_are_not_a_comment() {
        let src = "ref web.page:https://x.io \"T\"\nbehavior a \"A\" {\n}\n";
        assert!(unclosed(src).is_empty());
    }

    #[test]
    fn a_body_cut_where_the_tree_missed_a_form() {
        let src = "port pay \"Pay\" {\n  direction inbound\n  method charge\n\nbehavior settle \"Settle\" {\n  contract \"Settles.\"\n}\n";
        let cst = Cst::read(src, None).unwrap();
        let found: Vec<(usize, usize, bool)> = cst
            .pieces()
            .iter()
            .map(|p| (p.start(), p.row(), p.unclosed().is_some()))
            .collect();
        assert_eq!(
            found,
            vec![
                (0, 0, false),
                (src.find("behavior settle").unwrap(), 4, false)
            ]
        );
    }
}
