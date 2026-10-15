//! Semantic tokens: each lexeme's role, read as a token type (ADR 0023).

use specforge_ops::view::ProjectView;
use specforge_parser::lex::LexemeKind;
use specforge_registry::FieldType;

use super::LineIndex;
use super::syntax::{Role, Syntax};

/// Semantic token types used in the legend: every standard LSP semantic
/// token type. The legend is sent at `initialize`, before extensions load,
/// so it cannot grow with them; a kind's `semantic_token` is honored only
/// when it names one of these. Indices go over the wire, so consumers derive
/// them from this list rather than hardcoding them.
///
/// The ones the classification emits by itself: `keyword` (use, pub, from,
/// as, define, verify, method, expr, fn; a boolean value), `type` (entity
/// kind keywords), `function` (entity IDs when their kind declares no legend
/// token), `method` (method names), `variable` (references to no known
/// entity), `property` (field names), `string`, `comment`, `number`,
/// `enumMember` (verify kinds, enum values).
pub const TOKEN_TYPES: &[&str] = &[
    "namespace",
    "type",
    "class",
    "enum",
    "interface",
    "struct",
    "typeParameter",
    "parameter",
    "variable",
    "property",
    "enumMember",
    "event",
    "function",
    "method",
    "macro",
    "keyword",
    "modifier",
    "comment",
    "string",
    "number",
    "regexp",
    "operator",
    "decorator",
];

/// Semantic token modifiers. Bit positions.
pub const TOKEN_MODIFIERS: &[&str] = &[
    "declaration", // bit 0: entity declaration site
    "reference",   // bit 1: a reference to an entity
];

pub const MOD_DECLARATION: u32 = 1 << 0;
pub const MOD_REFERENCE: u32 = 1 << 1;

/// A classified token: its line, UTF-16 start and length, text, type (one
/// of [`TOKEN_TYPES`]) and modifier bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticToken {
    pub line: u32,
    pub col: u32,
    pub length: u32,
    pub text: String,
    pub token_type: &'static str,
    pub modifiers: u32,
}

/// The token type of an entity of `kind`: its declared `semantic_token`
/// when the legend carries it, else `function`.
fn kind_token(view: &ProjectView, kind: &str) -> &'static str {
    view.registries()
        .kinds()
        .get(kind)
        .and_then(|entry| entry.declared.semantic_token.as_deref())
        .and_then(|token| TOKEN_TYPES.iter().find(|t| **t == token).copied())
        .unwrap_or("function")
}

/// The type and modifiers of lexeme `i`, when it is classified.
fn classify(
    text: &str,
    syntax: &Syntax,
    i: usize,
    view: &ProjectView,
) -> Option<(&'static str, u32)> {
    let lexeme = syntax.lexemes[i];
    let role = syntax.roles[i];
    match lexeme.kind {
        LexemeKind::Comment => return Some(("comment", 0)),
        LexemeKind::Str { .. } => return Some(("string", 0)),
        LexemeKind::Number if matches!(role, Role::Value | Role::Item) => {
            return Some(("number", 0));
        }
        _ => {}
    }
    match role {
        Role::Keyword => Some(("keyword", 0)),
        Role::Kind => Some(("type", 0)),
        Role::Name => match syntax.header_of[i] {
            Some(header) => {
                let kind = syntax.text(text, syntax.headers[header as usize].kind);
                Some((kind_token(view, kind), MOD_DECLARATION))
            }
            None => Some(("method", MOD_DECLARATION)),
        },
        Role::Key => Some(("property", 0)),
        Role::VerifyKind => Some(("enumMember", 0)),
        Role::Value | Role::Item | Role::ImportName if lexeme.is_name() => {
            let fields = view.registries().fields();
            if syntax.reference_position(text, i as u32, view.registries().field_forms()) {
                let token = view
                    .graph()
                    .node(lexeme.text(text))
                    .map_or("variable", |node| kind_token(view, node.kind.raw.as_str()));
                return Some((token, MOD_REFERENCE));
            }
            if role != Role::Value {
                return None;
            }
            let key = syntax.key_of[i]?;
            let kind = syntax.kind_of_key(text, key)?;
            match fields.get(kind, syntax.text(text, key))?.field_type() {
                FieldType::Enum => Some(("enumMember", 0)),
                FieldType::Bool => Some(("keyword", 0)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The semantic tokens of a document, in order, one per line of a lexeme
/// (a multi-line string is cut at each line end).
pub(super) fn tokens(
    text: &str,
    index: &LineIndex,
    syntax: &Syntax,
    view: &ProjectView,
) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let lexemes = &syntax.lexemes;
    for i in 0..lexemes.len() {
        let Some((token_type, modifiers)) = classify(text, syntax, i, view) else {
            continue;
        };
        let mut start = lexemes[i].start;
        let end = lexemes[i].end;
        // A negative number's sign is part of it.
        if token_type == "number"
            && i > 0
            && lexemes[i - 1].is_punct('-')
            && lexemes[i - 1].end == start
        {
            start = lexemes[i - 1].start;
        }
        let first = index.position(start).line as usize;
        let last = index.position(end).line as usize;
        for line in first..=last {
            let from = start.max(index.line_start(line).unwrap_or(start));
            let to = end.min(index.line_end(line).unwrap_or(end));
            if from >= to {
                continue;
            }
            let at = index.position(from);
            tokens.push(SemanticToken {
                line: at.line,
                col: at.character,
                length: index.position(to).character - at.character,
                text: text[from..to].to_string(),
                token_type,
                modifiers,
            });
        }
    }
    tokens
}

/// `tokens`, delta-encoded against [`TOKEN_TYPES`].
pub(super) fn encode(tokens: &[SemanticToken]) -> Vec<tower_lsp::lsp_types::SemanticToken> {
    let mut data = Vec::with_capacity(tokens.len());
    let (mut prev_line, mut prev_col) = (0, 0);
    for token in tokens {
        let delta_line = token.line - prev_line;
        let delta_start = if delta_line == 0 {
            token.col - prev_col
        } else {
            token.col
        };
        data.push(tower_lsp::lsp_types::SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: TOKEN_TYPES
                .iter()
                .position(|t| *t == token.token_type)
                .unwrap_or(0) as u32,
            token_modifiers_bitset: token.modifiers,
        });
        prev_line = token.line;
        prev_col = token.col;
    }
    data
}
