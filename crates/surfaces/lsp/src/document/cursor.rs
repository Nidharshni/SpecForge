//! What the LSP knows about one position of a document (ADR 0023).

use specforge_common::Sym;
use specforge_ops::read::navigate::{Navigator, Token};
use specforge_ops::view::ProjectView;
use specforge_parser::lex::LexemeKind;
use tower_lsp::lsp_types::{Position, Range};

use super::LineIndex;
use super::syntax::{Expect, FrameKind, Role, Syntax, line_end, statement_start};
use specforge_parser::lex::Lexeme;

/// What the LSP knows about one position of a document, read from its
/// syntax. The entity it names is read from navigation, which checks each
/// token against the same text (ADR 0016's `Precision::Token`), so a graph
/// that lags the buffer never names the wrong entity.
pub struct Cursor<'d> {
    pub(super) text: &'d str,
    pub(super) index: &'d LineIndex,
    pub(super) syntax: &'d Syntax,
    pub(super) offset: usize,
    pub(super) position: Position,
}

/// Where a cursor is lexically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Code,
    String,
    Comment,
}

/// The identifier or scheme ref ID under a cursor: the cursor inside it or
/// just past its end (as navigation's `occurrence_at` counts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word<'d> {
    pub text: &'d str,
    pub range: Range,
    /// Its text before the cursor: what completion matches.
    pub prefix: &'d str,
}

/// The entity block around a cursor, as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityAt<'d> {
    pub kind: &'d str,
    pub id: Option<&'d str>,
}

/// What completing at a cursor means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionSite<'d> {
    /// A statement start at the top level: `use` and the registered kinds.
    Keywords { prefix: &'d str },
    /// A statement start in an entity's own body: the kind's fields.
    Fields { kind: &'d str, prefix: &'d str },
    /// After `verify` in an entity's own body: the verify kinds its kind allows.
    VerifyKind { kind: &'d str, prefix: &'d str },
    /// An item of the list a field holds (entity IDs only in a reference list).
    ListItem {
        kind: &'d str,
        field: &'d str,
        prefix: &'d str,
    },
    /// A field's single value (completed from the field's declared type).
    Value {
        kind: &'d str,
        field: &'d str,
        prefix: &'d str,
    },
    /// A string, a comment, a nested block, a header, a define block.
    Nothing,
}

/// The ranges a completion item's edit covers: `insert` from the start of
/// the word under the cursor to the cursor, `replace` the whole word (both
/// empty at the cursor when there is no word).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordEdit {
    pub insert: Range,
    pub replace: Range,
}

/// What a cursor names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A `use` statement's path: the cursor anywhere on the statement but
    /// on a binding that names an entity.
    Import { path: String },
    /// An entity, and the range of the token that names it here.
    Entity { id: Sym, origin: Range },
    /// A field of the enclosing entity's kind: the cursor on its name, in
    /// the entity's own body.
    Field { kind: String, field: String },
}

impl<'d> Cursor<'d> {
    /// The cursor's position.
    pub fn position(&self) -> Position {
        self.position
    }

    /// The index of the last lexeme starting at or before the cursor.
    fn last_started(&self) -> Option<usize> {
        let lexemes = &self.syntax.lexemes;
        lexemes
            .partition_point(|l| l.start <= self.offset)
            .checked_sub(1)
    }

    /// The name lexeme under the cursor: the cursor inside it or just past
    /// its end.
    fn word_index(&self) -> Option<usize> {
        let lexemes = &self.syntax.lexemes;
        let last = self.last_started()?;
        let lexeme = lexemes[last];
        if lexeme.is_name() && self.offset <= lexeme.end {
            return Some(last);
        }
        let before = last.checked_sub(1)?;
        (lexeme.start == self.offset
            && lexemes[before].end == self.offset
            && lexemes[before].is_name())
        .then_some(before)
    }

    /// The bytes of the word under the cursor: its name lexeme, or a
    /// scheme ref ID being typed (`gh.is`, `gh.issue:`), which lexes as
    /// several lexemes until it is whole.
    fn word_bytes(&self) -> Option<(usize, usize)> {
        let lexemes = &self.syntax.lexemes;
        let whole = self
            .word_index()
            .map(|w| (lexemes[w].start, lexemes[w].end));
        let part = |l: &Lexeme| {
            matches!(
                l.kind,
                LexemeKind::Ident
                    | LexemeKind::RefId
                    | LexemeKind::Number
                    | LexemeKind::Punct('.' | ':' | '-' | '/')
            )
        };
        let touching = |l: &Lexeme| l.start <= self.offset && self.offset <= l.end;
        let mut at = self.last_started()?;
        if !(touching(&lexemes[at]) && part(&lexemes[at])) {
            match at.checked_sub(1) {
                Some(before) if lexemes[before].end == self.offset && part(&lexemes[before]) => {
                    at = before;
                }
                _ => return whole,
            }
        }
        let (mut first, mut last) = (at, at);
        while first > 0
            && lexemes[first - 1].end == lexemes[first].start
            && part(&lexemes[first - 1])
        {
            first -= 1;
        }
        while last + 1 < lexemes.len()
            && lexemes[last].end == lexemes[last + 1].start
            && part(&lexemes[last + 1])
        {
            last += 1;
        }
        let (start, end) = (lexemes[first].start, lexemes[last].end);
        if first < last && specforge_parser::lex::is_ref_id_prefix(&self.text[start..end]) {
            return Some((start, end));
        }
        whole
    }

    /// The identifier or scheme ref ID under the cursor (a scheme ref ID
    /// being typed whole).
    pub fn word(&self) -> Option<Word<'d>> {
        let (start, end) = self.word_bytes()?;
        Some(Word {
            text: &self.text[start..end],
            range: Range {
                start: self.index.position(start),
                end: self.index.position(end),
            },
            prefix: &self.text[start..self.offset],
        })
    }

    /// The ranges a completion item's edit covers here.
    pub fn word_edit(&self) -> WordEdit {
        match self.word() {
            Some(word) => WordEdit {
                insert: Range {
                    start: word.range.start,
                    end: self.position,
                },
                replace: word.range,
            },
            None => {
                let here = Range {
                    start: self.position,
                    end: self.position,
                };
                WordEdit {
                    insert: here,
                    replace: here,
                }
            }
        }
    }

    /// What completing here means: keywords at a top-level statement start,
    /// the kind's fields at a statement start in its own body, its verify
    /// kinds after `verify`, the value or list item of a field of its own
    /// body; nothing in a string, a comment, a nested block, a header or a
    /// define block.
    pub fn completion(&self) -> CompletionSite<'d> {
        if self.place() != Place::Code {
            return CompletionSite::Nothing;
        }
        let prefix = self.word().map_or("", |word| word.prefix);
        let syntax = self.syntax;
        let (state, top) = self.context();
        let kind_of = |frame: Option<u32>| {
            let frame = frame?;
            (syntax.frames[frame as usize].kind == FrameKind::Entity)
                .then(|| syntax.kind_of_frame(self.text, frame))
                .flatten()
        };
        // The field (its key) a value or list here belongs to, in the
        // entity's own body.
        let field = |key: Option<u32>| {
            let key = key?;
            let kind = syntax.kind_of_key(self.text, key)?;
            Some((kind, syntax.text(self.text, key)))
        };
        let site = match state {
            Expect::Top => Some(CompletionSite::Keywords { prefix }),
            Expect::Body | Expect::AfterValue(_) => {
                kind_of(top).map(|kind| CompletionSite::Fields { kind, prefix })
            }
            Expect::VerifyKind => {
                kind_of(top).map(|kind| CompletionSite::VerifyKind { kind, prefix })
            }
            Expect::Value(ctx) if kind_of(top).is_some() => {
                field(ctx.key).map(|(kind, field)| CompletionSite::Value {
                    kind,
                    field,
                    prefix,
                })
            }
            Expect::List => top.and_then(|list| {
                let list = syntax.frames[list as usize];
                kind_of(list.parent)?;
                let (kind, field) = field(list.key)?;
                Some(CompletionSite::ListItem {
                    kind,
                    field,
                    prefix,
                })
            }),
            _ => None,
        };
        site.unwrap_or(CompletionSite::Nothing)
    }

    /// Whether the cursor is in code, a string or a comment.
    pub fn place(&self) -> Place {
        let Some(last) = self.last_started() else {
            return Place::Code;
        };
        let lexeme = self.syntax.lexemes[last];
        if lexeme.start >= self.offset || self.offset > lexeme.end {
            return Place::Code;
        }
        match lexeme.kind {
            LexemeKind::Comment => Place::Comment,
            LexemeKind::Str { closed } if self.offset < lexeme.end || !closed => Place::String,
            _ => Place::Code,
        }
    }

    /// The walk's state at the cursor, and the frame then innermost.
    pub(super) fn context(&self) -> (Expect, Option<u32>) {
        if let Some(word) = self.word_index() {
            return self.syntax.before[word];
        }
        let lexemes = &self.syntax.lexemes;
        let before = lexemes.partition_point(|l| l.end <= self.offset);
        let previous = (0..before)
            .rev()
            .find(|&i| lexemes[i].kind != LexemeKind::Comment);
        let Some(previous) = previous else {
            return (Expect::Top, None);
        };
        let (state, top) = self.syntax.after[previous];
        let newline = self.text.as_bytes()[lexemes[previous].end..self.offset].contains(&b'\n');
        if newline {
            let start = statement_start(&self.syntax.frames, top);
            return (line_end(state, None, start), top);
        }
        (state, top)
    }

    /// The header the cursor is on or right after (an inline `ref`, a
    /// union, a header being typed).
    fn header(&self) -> Option<u32> {
        if let Some(word) = self.word_index()
            && let Some(header) = self.syntax.header_of[word]
        {
            return Some(header);
        }
        match self.context().0 {
            Expect::HeaderName(header)
            | Expect::HeaderTitle(header)
            | Expect::HeaderOpen(header) => Some(header),
            _ => None,
        }
    }

    /// The entity block around the cursor, as written: its body, or its
    /// header. `None` at the top level and in a define block.
    pub fn entity(&self) -> Option<EntityAt<'d>> {
        let (_, top) = self.context();
        let header = match self.syntax.entity_frame(top) {
            Some(frame) => self.syntax.frames[frame as usize].header?,
            None => self.header()?,
        };
        let header = self.syntax.headers[header as usize];
        Some(EntityAt {
            kind: self.syntax.text(self.text, header.kind),
            id: header.name.map(|name| self.syntax.text(self.text, name)),
        })
    }

    /// The key lexeme of the field, in the entity's own body, whose name,
    /// value or list the cursor is on.
    pub(super) fn field_key(&self) -> Option<u32> {
        let key = match self.word_index().and_then(|w| self.syntax.key_of[w]) {
            Some(key) => key,
            None => match self.context() {
                (
                    Expect::Value(ctx)
                    | Expect::AfterValue(ctx)
                    | Expect::Annotation(ctx)
                    | Expect::AnnotationArg(ctx)
                    | Expect::Suffix(ctx)
                    | Expect::ExprOpen(ctx),
                    _,
                ) => ctx.key?,
                (Expect::List | Expect::Expr | Expect::Params, Some(frame)) => {
                    self.syntax.frames[frame as usize].key?
                }
                _ => return None,
            },
        };
        self.syntax.in_own_body(key).then_some(key)
    }

    /// The field, in the entity's own body, whose name, value or list the
    /// cursor is on.
    pub fn field(&self) -> Option<&'d str> {
        Some(self.syntax.text(self.text, self.field_key()?))
    }

    /// The path of the `use` statement the cursor is on (anywhere from its
    /// first lexeme to its last), without its quotes.
    pub fn import(&self) -> Option<&'d str> {
        let lexemes = &self.syntax.lexemes;
        let import = self.syntax.imports.iter().find(|import| {
            lexemes[import.first as usize].start <= self.offset
                && self.offset <= lexemes[import.last as usize].end
        })?;
        let path = lexemes[import.path? as usize].text(self.text);
        Some(path.trim_matches('"'))
    }

    /// The declaration or reference token under the cursor, as navigation
    /// reads it: what prepareRename answers and rename may edit.
    pub fn occurrence(&self, nav: &Navigator<'_>, file: &str) -> Option<Token> {
        let (line, col) = self.index.source_position(self.position)?;
        nav.occurrence_at(file, line, col)
    }

    /// What the cursor names, asked in this order: a `use` binding's
    /// imported name that names an entity; the `use` statement it is on;
    /// the declaration or reference token under it (navigation's
    /// occurrence, token-precise; a quoted reference's ID inside its string
    /// included); an identifier or scheme ref ID at a
    /// reference position that names an entity of the graph; the field
    /// whose name it is on. `None` otherwise: a word in a string or a
    /// comment, a kind keyword, a keyword and a value of a field typed as
    /// no reference name nothing. `file` is the document's session key (as
    /// spans name it).
    pub fn target(&self, nav: &Navigator<'_>, file: &str) -> Option<Target> {
        self.resolve(nav.view(), || self.occurrence(nav, file))
    }

    /// What the cursor names without asking navigation about its position:
    /// the `use` binding, the `use` statement, an identifier or scheme ref
    /// ID at a reference position (a header's name included) that names an
    /// entity of `view`'s graph, the field whose name it is on. What a
    /// cursor in a buffer typed since the compile names (ADR 0023 D7): the
    /// compiled text's token at its position may be another one.
    pub fn named(&self, view: &ProjectView<'_>) -> Option<Target> {
        self.resolve(view, || None)
    }

    /// [`Cursor::target`]'s order of questions, with `occurrence` answering
    /// the one about the token under the cursor.
    fn resolve(
        &self,
        view: &ProjectView<'_>,
        occurrence: impl FnOnce() -> Option<Token>,
    ) -> Option<Target> {
        let word = self.word_index();
        let entity = |i: usize| {
            let lexeme = self.syntax.lexemes[i];
            let id = lexeme.text(self.text);
            view.graph().node(id)?;
            Some(Target::Entity {
                id: Sym::new(id),
                origin: Range {
                    start: self.index.position(lexeme.start),
                    end: self.index.position(lexeme.end),
                },
            })
        };
        if let Some(word) = word
            && self.syntax.binding[word]
            && let Some(target) = entity(word)
        {
            return Some(target);
        }
        if let Some(path) = self.import() {
            return Some(Target::Import {
                path: path.to_string(),
            });
        }
        // Navigation's token answers wherever it is written: a quoted reference's ID is one, inside a string
        // (ADR 0218). Any other word of a string or a comment names nothing.
        if let Some(occurrence) = occurrence() {
            return Some(Target::Entity {
                id: occurrence.target,
                origin: self.index.range(&occurrence.span),
            });
        }
        if self.place() != Place::Code {
            return None;
        }
        let word = word?;
        let forms = view.registries().field_forms();
        if self
            .syntax
            .reference_position(self.text, word as u32, forms)
            && let Some(target) = entity(word)
        {
            return Some(target);
        }
        if self.syntax.roles[word] == Role::Key
            && let Some(kind) = self.syntax.kind_of_key(self.text, word as u32)
        {
            return Some(Target::Field {
                kind: kind.to_string(),
                field: self.syntax.text(self.text, word as u32).to_string(),
            });
        }
        None
    }
}
