//! The lexemes of a `.spec` text, read without parsing so a half-typed
//! document lexes too: identifiers, scheme ref IDs, numbers, strings,
//! comments and single punctuation characters, as tree-sitter's grammar
//! (`crates/tree-sitter-specforge/grammar.js`) tokenizes them; the test
//! `lexer_agrees_with_the_grammar` checks it over the repository's spec and
//! a fixture of the forms the spec does not write. A string is read the
//! grammar's way, across lines. One the grammar would not close is marked
//! (`Str { closed: false }`); a `"…"` one ends at its first line's end and a
//! `"""…"""` one before the next line that starts a top-level form, so an
//! unclosed quote never swallows the rest of a document being typed.
//! The parser's recovery from unclosed strings, the LSP's document, navigation
//! and the formatter read text through this module and scan none themselves
//! (ADR 0023, ADR 0038). Where a top-level form starts is this module's too
//! (`starts_top_level_form`): recovery and the LSP's document both ask it.
//!
//! This is the second reader of the language's text beside the grammar, and
//! `expr::tokenize` is a third that is not built on it. `tokenize` reads the
//! expression sub-language (`latency < 100ms`, an `expr { }` group, or a prose
//! line the prove pass reads), whose lexical rules are not the host's:
//! identifiers are `[a-z_][a-z0-9_]*` (here any ASCII letter and any non-ASCII
//! byte), a number's unit is alphabetic only and a trailing `.` belongs to the
//! number (`1.`, `1.ms`; here `.digits` only, then any word byte), `<=`, `>=`,
//! `==` and `!=` are one token (here two puncts), columns count characters
//! (here bytes), and any character that is no token is an error with a
//! position (here there are no errors). A tokenizer over these lexemes would
//! re-read every `Ident`, `Number`, `RefId`, string and comment lexeme
//! character by character to split and reject it, which is the old tokenizer
//! behind a layer, and would change the error position of an input such as
//! `Foo`, `abcÉ`, `10ms2` or `a.b:c`. So the two stay apart, and a test
//! (`expr::tests::the_tokenizer_agrees_with_the_lexer_and_the_grammar_on_the_corpus`)
//! pins what they share: on every expression of the repository's spec, the
//! tokens `tokenize` cuts are the lexemes `lex` cuts (the four two-character
//! operators joined), and what `parse_expression` reads from an `expr { }`
//! group's text is what the grammar read.

/// What a lexeme is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LexemeKind {
    /// Letters, digits and `_`, not starting with a digit; any non-ASCII
    /// byte continues it, so a word is never split.
    Ident,
    /// `scheme.kind:id` (`gh.issue:42`, the grammar's `scheme_ref_id`): a
    /// ref's ID, one lexeme.
    RefId,
    /// A word starting with a digit, with an optional `.digits` part:
    /// `42`, `10ms`, `1.5`. A leading `-` is its own `Punct`.
    Number,
    /// `"…"` (with `\` escapes) or `"""…"""`, quotes included, read as the
    /// grammar's `string` and `triple_quoted_string` tokens read it: across
    /// lines. `closed` is false for one the grammar would not close. Such a
    /// `"…"` (no closing quote; a `\` before a line break, which is no
    /// escape; or, spanning lines, a closing quote that runs straight into
    /// text, the pairing an unclosed quote shifts) ends at its first line's
    /// end. Such a `"""…"""` (no closing `"""`, or, spanning lines, closing
    /// quotes that run straight into text) ends at the end of the line
    /// before the next line that starts a top-level form, where the parser's
    /// recovery resumes, else at the text's end.
    Str { closed: bool },
    /// `//` to the end of its line, the newline excluded.
    Comment,
    /// One ASCII punctuation character.
    Punct(char),
}

impl LexemeKind {
    /// A string, closed or not.
    pub fn is_str(self) -> bool {
        matches!(self, LexemeKind::Str { .. })
    }
}

/// One lexeme, as a byte range of the text it was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lexeme {
    pub start: usize,
    pub end: usize,
    pub kind: LexemeKind,
}

impl Lexeme {
    /// An identifier or a scheme ref ID: what can name an entity.
    pub fn is_name(&self) -> bool {
        matches!(self.kind, LexemeKind::Ident | LexemeKind::RefId)
    }

    /// Its text in `text`, the text it was lexed from.
    pub fn text<'t>(&self, text: &'t str) -> &'t str {
        &text[self.start..self.end]
    }

    /// Whether it is the punctuation character `c`.
    pub fn is_punct(&self, c: char) -> bool {
        self.kind == LexemeKind::Punct(c)
    }
}

/// `"""`: what opens and closes a triple-quoted string.
pub(crate) const TRIPLE: &str = "\"\"\"";

/// The indentation of the line holding byte `at` of `text`, in bytes, when
/// nothing but whitespace precedes `at` on that line: what a top-level
/// form's line is measured by. `None` when a lexeme (or a string's tail)
/// precedes it on its line.
pub fn line_indent(text: &str, at: usize) -> Option<usize> {
    let line_start = text[..at].rfind('\n').map_or(0, |nl| nl + 1);
    text.as_bytes()[line_start..at]
        .iter()
        .all(u8::is_ascii_whitespace)
        .then_some(at - line_start)
}

/// Whether the first of `lexemes` (lexemes of `text`, in order; any after
/// its line are ignored) starts a line that opens a top-level form of the
/// grammar, a child of `source_file`: it is the first lexeme of its line,
/// that line is indented by at most `max_indent` bytes (`line_indent`), and
/// the line's lexemes read `use` then a path, `{` or `*`; `pub use` and the
/// same; `spec "Title" {`; `ref scheme.kind:id "Title"`; `kind name
/// ["Title"] {` (a define block too); or `kind name =` (a union). A title
/// is a `"…"` closed on that line.
///
/// The one rule of where a top-level form starts (ADR 0038 D6). The lexer
/// ends an unclosed `"""` before such a line, passing each line it scans
/// lexed alone; `form_starts` asks it over a whole document, for the parser's
/// recovery and the LSP's document. A test holds it to the grammar's
/// top-level nodes in the repository's spec, both ways.
pub fn starts_top_level_form(text: &str, lexemes: &[Lexeme], max_indent: usize) -> bool {
    let Some(first) = lexemes.first() else {
        return false;
    };
    if line_indent(text, first.start).is_none_or(|indent| indent > max_indent) {
        return false;
    }
    let line_end = text[first.start..]
        .find('\n')
        .map_or(text.len(), |nl| first.start + nl);
    let line = &lexemes[..lexemes.partition_point(|l| l.start < line_end)];
    let kind = |i: usize| line.get(i).map(|l| l.kind);
    let word = |i: usize| {
        line.get(i)
            .filter(|l| l.kind == LexemeKind::Ident)
            .map(|l| l.text(text))
    };
    // A header's title: a `"…"` closed on this line.
    let title = |i: usize| {
        line.get(i).is_some_and(|l| {
            l.kind == LexemeKind::Str { closed: true }
                && l.end <= line_end
                && !l.text(text).starts_with(TRIPLE)
        })
    };
    let punct = |i: usize, c: char| kind(i) == Some(LexemeKind::Punct(c));
    // An import's target: a path, bindings or `* as`.
    let import =
        |i: usize| kind(i).is_some_and(LexemeKind::is_str) || punct(i, '{') || punct(i, '*');
    match word(0) {
        Some("use") => import(1),
        Some("pub") => word(1) == Some("use") && import(2),
        Some("spec") => title(1) && punct(2, '{'),
        Some("ref") => kind(1) == Some(LexemeKind::RefId) && title(2),
        Some(_) if word(1).is_some() => punct(if title(2) { 3 } else { 2 }, '{') || punct(2, '='),
        _ => false,
    }
}

/// The lexemes of `text` (indices into `lexemes`, its `lex`, in order) that
/// start a top-level form as a document reads it: each first on its line,
/// whose line reads one of the grammar's top-level shapes
/// (`starts_top_level_form`) indented no deeper than the last line begun
/// with no bracket open (`{`, `[` or `(`). Such a line closes every bracket
/// open before it: a body, block, list or parameter list left unclosed ends
/// there. Comments neither start a form nor measure a line.
///
/// The one reading of where a document's forms start (ADR 0063 D3). The
/// parser's recovery cuts the source at each one the grammar's tree did not
/// start a form at (`cst`), and the LSP's document closes its frames at each.
pub fn form_starts(text: &str, lexemes: &[Lexeme]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut depth = 0usize;
    let mut top_indent = 0usize;
    let mut previous_end: Option<usize> = None;
    for (i, lexeme) in lexemes.iter().enumerate() {
        if lexeme.kind == LexemeKind::Comment {
            continue;
        }
        let first_on_line = previous_end.is_none_or(|end| text[end..lexeme.start].contains('\n'));
        previous_end = Some(lexeme.end);
        if first_on_line {
            let indent = line_indent(text, lexeme.start);
            if depth == 0
                && let Some(indent) = indent
            {
                top_indent = indent;
            }
            if starts_top_level_form(text, &lexemes[i..], top_indent) {
                starts.push(i);
                depth = 0;
                top_indent = indent.unwrap_or(top_indent);
            }
        }
        match lexeme.kind {
            LexemeKind::Punct('{' | '[' | '(') => depth += 1,
            LexemeKind::Punct('}' | ']' | ')') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    starts
}

/// The start of the first line after the one holding byte `from` of `text`
/// that starts a top-level form indented by no more than `max_indent` bytes
/// (`starts_top_level_form`, each line lexed alone: the text after a string
/// the grammar would not close cannot be lexed whole). Where such a string
/// ends: the lexer ends an unclosed `"""…"""` at the end of the line before
/// it (ADR 0038).
fn next_form_line(text: &str, from: usize, max_indent: usize) -> Option<usize> {
    let mut at = from;
    while let Some(nl) = text[at..].find('\n') {
        let line = at + nl + 1;
        if line >= text.len() {
            return None;
        }
        let line_end = text[line..].find('\n').map_or(text.len(), |nl| line + nl);
        let line_text = &text[line..line_end];
        if starts_top_level_form(line_text, &lex(line_text), max_indent) {
            return Some(line);
        }
        at = line;
    }
    None
}

/// A byte that continues an identifier: what the grammar's identifiers are
/// made of, and any non-ASCII byte, so a lexeme never splits a word.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

/// A byte that starts an identifier.
fn starts_ident(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

/// A byte of a scheme ref ID's last part: the grammar's
/// `/[^\s"{}()\[\],]+/`.
fn is_ref_id_byte(byte: u8) -> bool {
    !byte.is_ascii_whitespace()
        && !matches!(byte, b'"' | b'{' | b'}' | b'(' | b')' | b'[' | b']' | b',')
}

/// The end of the grammar's `identifier` (`[a-zA-Z_][a-zA-Z0-9_]*`)
/// starting at `at`, when one starts there.
fn ascii_ident_end(bytes: &[u8], at: usize) -> Option<usize> {
    let first = *bytes.get(at)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut end = at + 1;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    Some(end)
}

/// The end of a scheme ref ID starting at `at` (`scheme.kind:id`, as the
/// grammar's `scheme_ref_id` token reads it), when one starts there.
fn ref_id_end(bytes: &[u8], at: usize) -> Option<usize> {
    let scheme = ascii_ident_end(bytes, at)?;
    if bytes.get(scheme) != Some(&b'.') {
        return None;
    }
    let kind = ascii_ident_end(bytes, scheme + 1)?;
    if bytes.get(kind) != Some(&b':') {
        return None;
    }
    let mut end = kind + 1;
    while end < bytes.len() && is_ref_id_byte(bytes[end]) {
        end += 1;
    }
    (end > kind + 1).then_some(end)
}

/// Whether the byte at `at` of `text` continues a token: what the closing
/// quote of a string spanning lines is never followed by in the repository's
/// spec. Whitespace, a bracket, `,`, `|`, `/` (a comment) and the end of the
/// text do not continue one.
pub fn runs_into_text(text: &str, at: usize) -> bool {
    follows_text(text.as_bytes(), at)
}

/// `runs_into_text` over bytes.
fn follows_text(bytes: &[u8], at: usize) -> bool {
    bytes.get(at).is_some_and(|&c| {
        !(c.is_ascii_whitespace()
            || matches!(
                c,
                b'{' | b'}' | b'[' | b']' | b'(' | b')' | b',' | b'|' | b'/'
            ))
    })
}

/// Whether `text` is a scheme ref ID cut short: `scheme`, `scheme.`,
/// `scheme.kind`, `scheme.kind:` or a whole `scheme.kind:id` (the grammar's
/// `scheme_ref_id`, typed so far).
pub fn is_ref_id_prefix(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some(scheme) = ascii_ident_end(bytes, 0) else {
        return false;
    };
    match bytes.get(scheme) {
        None => return true,
        Some(b'.') => {}
        Some(_) => return false,
    }
    if scheme + 1 == bytes.len() {
        return true;
    }
    let Some(kind) = ascii_ident_end(bytes, scheme + 1) else {
        return false;
    };
    match bytes.get(kind) {
        None => true,
        Some(b':') => bytes[kind + 1..].iter().all(|&b| is_ref_id_byte(b)),
        Some(_) => false,
    }
}

/// The end of the `"…"` string opening at `open`, and whether the grammar
/// closes it: at its closing quote when the grammar's token ends there and,
/// if it spans lines, nothing runs straight into that quote; else at its
/// first line's end. `"`, `\` and `\n` are ASCII, so no byte of a wider
/// character is ever taken for one of them.
fn regular_string(bytes: &[u8], open: usize) -> (usize, bool) {
    let line_end = bytes[open..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(bytes.len(), |at| open + at);
    let mut i = open + 1;
    let close = loop {
        match bytes.get(i) {
            None => break None,
            Some(b'"') => break Some(i),
            // The grammar's escape is `\` and any character but a line break.
            Some(b'\\') => match bytes.get(i + 1) {
                None | Some(b'\n') => break None,
                Some(_) => i += 2,
            },
            Some(_) => i += 1,
        }
    };
    match close {
        Some(close) if close < line_end || !follows_text(bytes, close + 1) => (close + 1, true),
        _ => (line_end, false),
    }
}

/// The end of the `"""…"""` string opening at `open`, and whether the
/// grammar closes it: after the first `"""` that follows, unless the string
/// spans lines and that `"""` runs straight into text (the pairing an
/// unclosed `"""` shifts). One the grammar would not close ends at the end
/// of the line before the next line that starts a top-level form
/// (`next_form_line`, indented no deeper than `top_indent`, the top-level
/// line it sits under), else at the text's end.
fn triple_string(text: &str, open: usize, top_indent: usize) -> (usize, bool) {
    let body = open + TRIPLE.len();
    let close = text[body..].find(TRIPLE).map(|at| body + at + TRIPLE.len());
    match close {
        Some(end) if !text[open..end].contains('\n') || !follows_text(text.as_bytes(), end) => {
            (end, true)
        }
        _ => (
            next_form_line(text, body, top_indent).map_or(text.len(), |line| line - 1),
            false,
        ),
    }
}

/// The lexemes of `text`, in order, whitespace skipped. Text that starts
/// inside a string is read as code: callers lex from a statement's start.
/// A string the grammar would not close is marked `Str { closed: false }`:
/// a `"…"` ends at its first line's end, a `"""…"""` at the end of the line
/// before the next line that starts a top-level form, else at the text's end.
pub fn lex(text: &str) -> Vec<Lexeme> {
    let bytes = text.as_bytes();
    let find = |from: usize, needle: &[u8]| {
        bytes[from.min(bytes.len())..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|at| from + at)
    };
    let mut lexemes = Vec::new();
    // The brace depth, and the indentation of the last line begun at depth
    // 0: a top-level form after an unclosed `"""` is indented no deeper.
    let mut depth = 0usize;
    let mut top_indent = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        let start = i;
        if byte.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if depth == 0
            && !bytes[i..].starts_with(b"//")
            && let Some(indent) = line_indent(text, i)
        {
            top_indent = indent;
        }
        let kind = if bytes[i..].starts_with(b"\"\"\"") {
            let (end, closed) = triple_string(text, i, top_indent);
            i = end;
            LexemeKind::Str { closed }
        } else if byte == b'"' {
            let (end, closed) = regular_string(bytes, i);
            i = end;
            LexemeKind::Str { closed }
        } else if bytes[i..].starts_with(b"//") {
            i = find(i, b"\n").unwrap_or(bytes.len());
            LexemeKind::Comment
        } else if let Some(end) = ref_id_end(bytes, i) {
            i = end;
            LexemeKind::RefId
        } else if starts_ident(byte) {
            while i < bytes.len() && is_word(bytes[i]) {
                i += 1;
            }
            LexemeKind::Ident
        } else if byte.is_ascii_digit() {
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            while i < bytes.len() && is_word(bytes[i]) {
                i += 1;
            }
            LexemeKind::Number
        } else {
            // Every non-ASCII byte is a word byte: punctuation is ASCII.
            i += 1;
            match byte {
                b'{' => depth += 1,
                b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            LexemeKind::Punct(char::from(byte))
        };
        lexemes.push(Lexeme {
            start,
            end: i,
            kind,
        });
    }
    lexemes
}
