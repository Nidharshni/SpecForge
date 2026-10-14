//! What property tests of the parser, the formatter and the project session
//! draw from (feature `testing`; ADR 0090): the repository's tracked `.spec`
//! corpus, and edits of it a user could type mid-edit.

use crate::lex::{LexemeKind, lex};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The directories under the repository root whose `.spec` files are the
/// corpus: every tracked one, and nothing a developer keeps untracked beside
/// them.
pub const CORPUS_ROOTS: &[&str] = &[
    "spec",
    "examples",
    "integrations",
    "extensions",
    "crates",
    "xtask",
];

/// Every `.spec` file under [`CORPUS_ROOTS`], read once, sorted by path, with
/// `target/` and hidden directories skipped.
///
/// # Panics
///
/// With the count, when fewer than 200 files are found: a corpus that small
/// means the root moved.
pub fn corpus() -> &'static [(PathBuf, String)] {
    static CORPUS: OnceLock<Vec<(PathBuf, String)>> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let root = specforge_repo_root::repo_root();
        let mut paths = Vec::new();
        for dir in CORPUS_ROOTS {
            walk(&root.join(dir), &mut paths);
        }
        paths.sort();
        let files: Vec<(PathBuf, String)> = paths
            .into_iter()
            .filter_map(|path| std::fs::read_to_string(&path).ok().map(|text| (path, text)))
            .collect();
        assert!(
            files.len() >= 200,
            "the corpus holds {} .spec files under {}, fewer than 200: the root moved",
            files.len(),
            root.display()
        );
        files
    })
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "spec") {
            out.push(path);
        }
    }
}

/// One edit a user could make: each lands on a character boundary of the text
/// it edits.
#[derive(Debug, Clone)]
pub enum Edit {
    /// Delete up to 40 bytes from a position.
    Delete { at: usize, len: usize },
    /// Insert one of [`TOKENS`] at a position.
    Insert { at: usize, token: &'static str },
    /// Write a line twice.
    DuplicateLine { line: usize },
    /// Drop everything from a position.
    Truncate { at: usize },
}

/// What an insertion types: the grammar's delimiters, a comment, a header of
/// each top-level form, and characters a recovery has to skip.
pub const TOKENS: &[&str] = &[
    "{",
    "}",
    "[",
    "]",
    "\"",
    "\"\"\"",
    "//",
    "\n",
    " ",
    ",",
    "@optional",
    "verify unit \"x\"",
    "behavior b \"B\" {",
    "use \"x\"",
    "->",
    "expr {",
    "(",
    ")",
    "\t",
    "\\",
    "type t \"T\" = a | b",
    "ref rust.fn:x",
    "#",
    "─",
];

/// `at` taken modulo `text`'s length, rounded down to a character boundary.
fn boundary(text: &str, at: usize) -> usize {
    let at = if text.is_empty() {
        0
    } else {
        at % (text.len() + 1)
    };
    text.floor_char_boundary(at)
}

impl Edit {
    /// `text` with this edit applied; positions are taken modulo the text's
    /// length and rounded down to a character boundary.
    pub fn apply(&self, text: &str) -> String {
        match *self {
            Edit::Delete { at, len } => {
                let start = boundary(text, at);
                let end = text.floor_char_boundary((start + len).min(text.len()));
                format!("{}{}", &text[..start], &text[end..])
            }
            Edit::Insert { at, token } => {
                let at = boundary(text, at);
                format!("{}{token}{}", &text[..at], &text[at..])
            }
            Edit::DuplicateLine { line } => {
                let lines: Vec<&str> = text.split_inclusive('\n').collect();
                if lines.is_empty() {
                    return String::new();
                }
                let at = line % lines.len();
                let mut out = String::with_capacity(text.len() + lines[at].len() + 1);
                for (i, l) in lines.iter().enumerate() {
                    out.push_str(l);
                    if i == at {
                        if !l.ends_with('\n') {
                            out.push('\n');
                        }
                        out.push_str(l);
                    }
                }
                out
            }
            Edit::Truncate { at } => text[..boundary(text, at)].to_string(),
        }
    }
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (any::<usize>(), 1usize..=40).prop_map(|(at, len)| Edit::Delete { at, len }),
        (any::<usize>(), 0..TOKENS.len()).prop_map(|(at, t)| Edit::Insert {
            at,
            token: TOKENS[t]
        }),
        any::<usize>().prop_map(|line| Edit::DuplicateLine { line }),
        any::<usize>().prop_map(|at| Edit::Truncate { at }),
    ]
}

/// A corpus file and the same file after one to three edits. Its `Debug` text
/// names the file and the edits, not the (large) texts: to reproduce a
/// failure, apply [`Edit::apply`] to the file in order.
#[derive(Clone)]
pub struct EditedFile {
    /// The corpus file, relative to the repository root.
    pub path: String,
    /// The edits, in order.
    pub edits: Vec<Edit>,
    /// The file as tracked.
    pub original: String,
    /// The file after the edits.
    pub edited: String,
}

impl std::fmt::Debug for EditedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditedFile")
            .field("path", &self.path)
            .field("edits", &self.edits)
            .finish()
    }
}

/// A corpus file edited one to three times.
pub fn edited_corpus() -> impl Strategy<Value = EditedFile> {
    (0..corpus().len(), vec(edit(), 1..=3)).prop_map(|(file, edits)| {
        let (path, original) = &corpus()[file];
        let edited = edits
            .iter()
            .fold(original.clone(), |text, e| e.apply(&text));
        EditedFile {
            path: relative(path),
            edits,
            original: original.clone(),
            edited,
        }
    })
}

fn relative(path: &Path) -> String {
    let root = specforge_repo_root::repo_root();
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Which lines of `text` may take a layout change: not a line that starts or
/// ends inside a string lexeme, and not a line inside a `[ ... ]` that holds a
/// comment lexeme (`formatting_consistency` exempts those).
fn touchable_lines(text: &str) -> Vec<bool> {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let line_of = |at: usize| starts.partition_point(|&s| s <= at) - 1;
    let mut touchable = vec![true; starts.len()];
    let mut spoil = |from: usize, to: usize| {
        for flag in &mut touchable[line_of(from)..=line_of(to)] {
            *flag = false;
        }
    };
    let lexemes = lex(text);
    for l in &lexemes {
        if l.kind.is_str() && text[l.start..l.end].contains('\n') {
            spoil(l.start, l.end);
        }
    }
    let mut open: Vec<usize> = Vec::new();
    for (i, l) in lexemes.iter().enumerate() {
        if l.is_punct('[') {
            open.push(i);
        } else if l.is_punct(']')
            && let Some(first) = open.pop()
            && lexemes[first..i]
                .iter()
                .any(|m| m.kind == LexemeKind::Comment)
        {
            spoil(lexemes[first].start, l.start);
        }
    }
    touchable
}

/// `text` with its layout changed and nothing else: touchable lines
/// re-indented (`indent` is prepended) and given trailing spaces.
fn relaid(text: &str, indent: &str) -> String {
    let touchable = touchable_lines(text);
    let mut out = String::with_capacity(text.len() * 2);
    for (line, ok) in text.split_inclusive('\n').zip(touchable) {
        let (body, newline) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        if !ok || body.trim().is_empty() {
            out.push_str(line);
            continue;
        }
        out.push_str(indent);
        out.push_str(body);
        out.push_str("  ");
        out.push_str(newline);
    }
    out
}

/// A corpus file and the same file with its layout changed and nothing else:
/// lines re-indented (by three spaces or a tab) and trailing spaces added, on
/// lines that start and end outside a string lexeme ([`crate::lex::lex`]) and
/// lie in no `[ ... ]` that holds a comment lexeme. `edits` is empty and
/// `edited` is the relaid text.
pub fn relaid_corpus() -> impl Strategy<Value = EditedFile> {
    (0..corpus().len(), any::<bool>()).prop_map(|(file, tab)| {
        let (path, original) = &corpus()[file];
        EditedFile {
            path: relative(path),
            edits: Vec::new(),
            original: original.clone(),
            edited: relaid(original, if tab { "\t" } else { "   " }),
        }
    })
}

/// The configuration every property test runs under: 256 cases
/// (`PROPTEST_CASES` overrides it for a deep run), a fixed seed so a gate run
/// is reproducible, no regression files written into the source tree, and
/// shrinking bounded at 2,000 iterations.
pub fn property_config() -> Config {
    Config {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(256),
        rng_seed: RngSeed::Fixed(0x5EED_F0E9),
        failure_persistence: None,
        max_shrink_iters: 2_000,
        ..Config::default()
    }
}
