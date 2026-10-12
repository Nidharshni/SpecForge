//! The LSP's one reader of a `.spec` buffer (ADR 0023): a [`Document`]
//! owns an open buffer, its version, its [`LineIndex`] (the only
//! conversion between byte offsets and UTF-16 positions) and its syntax,
//! which answers what is at a position ([`Cursor`]).

mod cursor;
mod line_index;
mod syntax;
mod tokens;

pub use cursor::{CompletionSite, Cursor, EntityAt, Place, Target, Word, WordEdit};
pub use line_index::LineIndex;
pub use tokens::{MOD_DECLARATION, MOD_REFERENCE, SemanticToken, TOKEN_MODIFIERS, TOKEN_TYPES};

use specforge_ops::view::ProjectView;
use std::sync::{Arc, OnceLock};
use syntax::Syntax;
use tower_lsp::lsp_types::{Position, Range};

/// An open document: the editor's text of one `.spec` file, its version,
/// its line index, and (read on first use, once per edit) its syntax. Every
/// question the LSP asks about a buffer's text is asked here: where a
/// position is (`index`), what is at it (`at`), how it is highlighted
/// (`tokens`). The graph is never
/// consulted for structure: it lags the buffer while the user types
/// (ADR 0023).
pub struct Document {
    text: String,
    /// The editor's version from didOpen/didChange, stamped onto published
    /// diagnostics so clients drop stale deliveries.
    version: Option<i32>,
    index: Arc<LineIndex>,
    syntax: OnceLock<Syntax>,
}

impl Document {
    pub fn new(text: String) -> Document {
        let index = Arc::new(LineIndex::new(&text));
        Document {
            text,
            version: None,
            index,
            syntax: OnceLock::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn version(&self) -> Option<i32> {
        self.version
    }

    pub fn set_version(&mut self, version: i32) {
        self.version = Some(version);
    }

    /// Where each line starts: positions in this document, both ways.
    pub fn index(&self) -> &Arc<LineIndex> {
        &self.index
    }

    /// Apply one content change: `range` replaced by `text`, its ends
    /// clamped to the document; the whole text when `range` is `None`.
    pub fn apply_change(&mut self, range: Option<Range>, text: &str) {
        match range {
            Some(range) => {
                let start = self.index.offset_clamped(range.start);
                let end = self.index.offset_clamped(range.end);
                let (start, end) = (start.min(end), start.max(end));
                self.text.replace_range(start..end, text);
            }
            None => self.text = text.to_string(),
        }
        self.index = Arc::new(LineIndex::new(&self.text));
        self.syntax = OnceLock::new();
    }

    /// The document's syntax, read on first use after an edit.
    fn syntax(&self) -> &Syntax {
        self.syntax.get_or_init(|| Syntax::read(&self.text))
    }

    /// What is at `position`; `None` past the end of its line or of the
    /// document.
    pub fn at(&self, position: Position) -> Option<Cursor<'_>> {
        let offset = self.index.offset(position)?;
        Some(Cursor {
            text: &self.text,
            index: &self.index,
            syntax: self.syntax(),
            offset,
            position,
        })
    }

    /// The semantic tokens, in document order, one per line of a lexeme
    /// (a multi-line string or comment is cut at each line end). `view`
    /// gives each kind's declared token type, each field's declared type
    /// and the entities a reference may name.
    pub fn tokens(&self, view: &ProjectView<'_>) -> Vec<SemanticToken> {
        tokens::tokens(&self.text, &self.index, self.syntax(), view)
    }

    /// [`Self::tokens`], delta-encoded against [`TOKEN_TYPES`] for
    /// `textDocument/semanticTokens/full`.
    pub fn semantic_tokens(
        &self,
        view: &ProjectView<'_>,
    ) -> Vec<tower_lsp::lsp_types::SemanticToken> {
        tokens::encode(&self.tokens(view))
    }
}
