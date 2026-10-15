use crate::ast::*;
use crate::cst::{Cst, CstError, UnclosedString};
use crate::expr::{CmpOp, Expr, ExprSpan, SpannedExpr};
use crate::format_version::{CURRENT_FORMAT_VERSION, detect_format_version};
use crate::lex::MAX_NESTING;
use specforge_common::{SourceSpan, Sym, structural};
use tree_sitter::Node;
use tree_sitter_specforge::{field, kind};

mod body;
mod declarations;
mod errors;
mod expression;
mod reparse;

use errors::{position_span, unclosed_error};

/// Process escape sequences in a string literal (after quote stripping).
fn unescape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub fn parse(source: &str, file_path: &str) -> SpecFile {
    parse_incremental(source, file_path, None).0
}

/// What a file held before its replacement: its text and the tree it parsed to.
#[derive(Clone, Copy)]
pub struct Previous<'a> {
    pub text: &'a str,
    pub tree: &'a tree_sitter::Tree,
}

/// Parse `source`, reusing the subtrees of `previous` that the replacement left
/// unchanged (the longest common byte prefix and suffix of the two texts).
/// Returns the file and its tree, for the next reparse.
pub fn parse_incremental(
    source: &str,
    file_path: &str,
    previous: Option<Previous<'_>>,
) -> (SpecFile, Option<tree_sitter::Tree>) {
    let old_tree = previous.map(|previous| reparse::edited(previous, source));
    let (mut file, tree) = parse_syntax(source, file_path, old_tree.as_ref());
    // The header is read with the file, so every consumer of a parse (check,
    // watch, the LSP, MCP) has the version and what it reports.
    (file.format_version, file.format_diagnostics) = detect_format_version(source, file_path);
    (file, tree)
}

/// The file's entities, imports and syntax errors.
fn parse_syntax(
    source: &str,
    file_path: &str,
    old_tree: Option<&tree_sitter::Tree>,
) -> (SpecFile, Option<tree_sitter::Tree>) {
    let file_sym = Sym::new(file_path);
    let cst = match Cst::read(source, old_tree) {
        Ok(cst) => cst,
        Err(CstError::Language(e)) => {
            return (
                unparsed(
                    file_sym,
                    format!("failed to load specforge grammar: {e}"),
                    unparsed_span(file_sym),
                ),
                None,
            );
        }
        Err(CstError::NoTree) => {
            return (
                unparsed(
                    file_sym,
                    "tree-sitter parse failed".to_string(),
                    unparsed_span(file_sym),
                ),
                None,
            );
        }
        Err(CstError::TooDeep { at }) => {
            let message = format!(
                "brackets nested more than {MAX_NESTING} deep: a .spec file nests (), [] and {{}} at most {MAX_NESTING} levels"
            );
            let span = position_span(source, file_sym, at, at + 1);
            return (unparsed(file_sym, message, span), None);
        }
    };
    let mut file = SpecFile {
        path: file_sym,
        imports: Vec::new(),
        entities: Vec::new(),
        format_version: CURRENT_FORMAT_VERSION,
        format_diagnostics: Vec::new(),
        errors: Vec::new(),
    };
    let pieces = cst.pieces();
    let recovered = pieces.len() > 1 || pieces[0].unclosed().is_some();
    for (n, piece) in pieces.iter().enumerate() {
        let mut ctx = ParseContext::new(piece.text(), file_sym);
        ctx.row_offset = piece.row();
        ctx.limit = piece.unclosed().map(|_| piece.source_len());
        ctx.next_piece_row = pieces.get(n + 1).map(|next| next.row());
        ctx.walk(piece.root());
        // Errors past the piece come from the appended delimiter, and an
        // error around the opening quote is the grammar tripping over the
        // unclosed string: its own diagnostic replaces both.
        let limit = ctx.limit_point();
        ctx.errors
            .retain(|e| (e.span.start_line, e.span.start_col) < limit);
        if let Some(u) = piece.unclosed() {
            let quote = unclosed_error(source, file_sym, &u, piece.start() + piece.source_len());
            let open = (quote.span.start_line, quote.span.start_col);
            ctx.errors.retain(|e| {
                let start = (e.span.start_line, e.span.start_col);
                let end = (e.span.end_line, e.span.end_col);
                !(start <= open && open < end)
            });
            ctx.errors.push(quote);
        }
        file.imports.append(&mut ctx.imports);
        file.entities.append(&mut ctx.entities);
        file.errors.append(&mut ctx.errors);
    }
    if recovered {
        file.errors
            .sort_by_key(|e| (e.span.start_line, e.span.start_col));
    }
    (file, Some(cst.into_tree()))
}

/// A file nothing could be read from, with the one error saying why.
/// The span of an error that has no place in the text: the first character.
fn unparsed_span(file_sym: Sym) -> SourceSpan {
    SourceSpan {
        file: file_sym,
        start_line: 1,
        start_col: 1,
        end_line: 1,
        end_col: 1,
    }
}

/// A file read into no tree: its one error, `message` at `span`.
fn unparsed(file_sym: Sym, message: String, span: SourceSpan) -> SpecFile {
    SpecFile {
        path: file_sym,
        imports: Vec::new(),
        entities: Vec::new(),
        format_version: CURRENT_FORMAT_VERSION,
        format_diagnostics: Vec::new(),
        errors: vec![ParseError { message, span }],
    }
}

struct ParseContext<'a> {
    source: &'a str,
    file_sym: Sym,
    /// Lines before this piece of the file (recovery parses in pieces).
    row_offset: usize,
    /// Bytes of the piece that are real source; past this is the appended
    /// closing delimiter, and positions there clamp to its start.
    limit: Option<usize>,
    /// The source row (0-based) of the next piece, if one follows: where
    /// this piece's last body was ended.
    next_piece_row: Option<usize>,
    imports: Vec<ImportDeclaration>,
    entities: Vec<Entity>,
    errors: Vec<ParseError>,
}

impl<'a> ParseContext<'a> {
    fn new(source: &'a str, file_sym: Sym) -> Self {
        ParseContext {
            source,
            file_sym,
            row_offset: 0,
            limit: None,
            next_piece_row: None,
            imports: Vec::new(),
            entities: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn walk(&mut self, root: Node) {
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            match child.kind() {
                _ if child.is_error() => self.push_error_node(child),
                kind::ENTITY_BLOCK => self.parse_entity_block(child),
                kind::SPEC_BLOCK => self.parse_spec_block(child),
                kind::REF_BLOCK => self.parse_ref_block(child),
                kind::DEFINE_BLOCK => self.parse_define_block(child),
                kind::UNION_BLOCK => self.parse_union_block(child),
                kind::USE_IMPORT => self.parse_use_import(child, false),
                kind::PUB_USE_IMPORT => self.parse_use_import(child, true),
                kind::COMMENT => {}
                _ => {}
            }
        }
        // The walk above reports errors at block level and inside field
        // values. Tree-sitter recovers anywhere, so report the rest too
        // (method signatures, parameters, ...): no syntax error is silent.
        self.report_unreported_errors(root);
    }

    fn text(&self, node: Node) -> &'a str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    /// A node position as 1-based (line, column) in the whole file.
    fn position(&self, byte: usize, point: tree_sitter::Point) -> (usize, usize) {
        if let Some(limit) = self.limit
            && byte > limit
        {
            return self.limit_point();
        }
        (point.row + self.row_offset + 1, point.column + 1)
    }

    /// Where the real source of this piece ends, as 1-based (line, column).
    fn limit_point(&self) -> (usize, usize) {
        match self.limit {
            Some(limit) => {
                let before = &self.source[..limit];
                let row = before.matches('\n').count();
                let col = limit - before.rfind('\n').map_or(0, |nl| nl + 1);
                (row + self.row_offset + 1, col + 1)
            }
            None => (usize::MAX, usize::MAX),
        }
    }

    fn span(&self, node: Node) -> SourceSpan {
        let (start_line, start_col) = self.position(node.start_byte(), node.start_position());
        let (end_line, end_col) = self.position(node.end_byte(), node.end_position());
        SourceSpan {
            file: self.file_sym,
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    fn unquote(&self, node: Node) -> String {
        let text = self.text(node);
        if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
            let inner = &text[1..text.len() - 1];
            unescape(inner)
        } else {
            text.to_string()
        }
    }

    // --- Formal expressions (`metric expr { ... }`) ---------------------
}
