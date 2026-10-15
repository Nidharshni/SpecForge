//! A document's lexemes (`specforge_parser::lex`), each with its role, and
//! the entity bodies, nested blocks, expression groups, lists and parameter
//! lists they open: the structure the cursor and the semantic tokens read.
//! A line that starts a top-level form (`lex::form_starts`, where the
//! parser's recovery cuts too) closes every frame left open before it.
//! Nothing here names a kind or a field: kinds and fields are read as
//! structure; the registries are consulted by the readers.

use specforge_graph::FieldForms;
use specforge_parser::Written;
use specforge_parser::lex::{Lexeme, LexemeKind};

mod walk;

/// What a lexeme is in its statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// `use pub from as define verify method expr fn`.
    Keyword,
    /// An entity header's first word (structure, not registry).
    Kind,
    /// An entity header's ID (Ident or RefId); a method's name.
    Name,
    /// An entity header's string.
    Title,
    /// A field's name, in a body or a nested block.
    Key,
    /// `verify <kind> "…"`.
    VerifyKind,
    /// A field's value lexemes (Str, Number, Ident, RefId).
    Value,
    /// A list's items.
    Item,
    /// A use statement's path.
    ImportPath,
    /// A use statement's binding or alias names.
    ImportName,
    Comment,
    /// Punctuation, expression atoms, an unreadable lexeme.
    Other,
}

/// What a frame is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameKind {
    /// An entity's body.
    Entity,
    /// A `define` block's body (W143: it declares nothing).
    Define,
    /// A nested block (`requires { … }`).
    Block,
    /// An `expr { … }` group.
    Expr,
    /// A list (`[ … ]`).
    List,
    /// A parameter list or a parenthesized type (`( … )`).
    Params,
}

/// A region a lexeme opens: an entity body, a nested block, a list.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) kind: FrameKind,
    pub(crate) parent: Option<u32>,
    /// The lexeme that closed it, when written.
    pub(crate) close: Option<u32>,
    /// The field (its key lexeme) whose value it is.
    pub(crate) key: Option<u32>,
    /// The header it is the body of (an entity).
    pub(crate) header: Option<u32>,
    /// What the walk expects after it closes.
    resume: Expect,
}

/// An entity header: `kind name "title"`, opening a body or not (an
/// inline `ref`, a union).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Header {
    pub(crate) kind: u32,
    pub(crate) name: Option<u32>,
}

/// A `use` statement.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Import {
    /// Its first and last lexemes.
    pub(crate) first: u32,
    pub(crate) last: u32,
    /// Its path (a `Str`), when written.
    pub(crate) path: Option<u32>,
}

/// The context a value is read in: the field it belongs to (`None` for a
/// union's variants and a method's return type) and the role its atoms take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ctx {
    pub(crate) key: Option<u32>,
    atoms: Role,
}

/// Where a `use` statement's walk is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UseStep {
    /// After `pub`: `use`.
    Pub,
    /// After `use`.
    Start,
    /// Inside `{ … }`, expecting a binding.
    Binding,
    /// After a binding's name.
    AfterBinding,
    /// After `as`, expecting an alias.
    Alias,
    /// After `*`.
    Star,
    /// After `* as`.
    StarAlias,
    /// Expecting `from`.
    From,
    /// After `from`, expecting the path.
    Path,
}

/// What the walk expects next: the state a position is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expect {
    /// A statement start at the top level.
    Top,
    /// A statement start in an entity body, a define block or a nested
    /// block.
    Body,
    /// After an entity header's kind: its name.
    HeaderName(u32),
    /// After its name: a title, `{` or `=`.
    HeaderTitle(u32),
    /// After its title: `{`.
    HeaderOpen(u32),
    DefineName,
    DefineOpen,
    Use(UseStep, u32),
    /// A value's next atom.
    Value(Ctx),
    /// After a value's atom: an operator continues it.
    AfterValue(Ctx),
    /// After `@`: the annotation's name.
    Annotation(Ctx),
    /// After an annotation's name: its optional string.
    AnnotationArg(Ctx),
    /// After `verify`: a kind or the description.
    VerifyKind,
    /// After the verify kind: the description.
    VerifyDesc,
    /// After `method`: its name.
    MethodName,
    /// After the method's name: `(`.
    MethodParams,
    /// After the parameters: `->`.
    MethodAfter,
    /// After `-` of `->`.
    MethodArrow,
    /// Inside a list.
    List,
    /// Inside an `expr` group.
    Expr,
    /// Inside parentheses.
    Params,
    /// After `expr`: its `{`.
    ExprOpen(Ctx),
    /// After a type's `[`: the `]` of its array suffix.
    Suffix(Ctx),
}

/// A document's lexemes, each with its role, and its frames: the entity
/// bodies, nested blocks, expression groups, lists and parameter lists the
/// lexemes open, each with its parent and its closing lexeme when written.
pub(crate) struct Syntax {
    pub(crate) lexemes: Vec<Lexeme>,
    pub(crate) roles: Vec<Role>,
    pub(crate) frames: Vec<Frame>,
    pub(crate) headers: Vec<Header>,
    pub(crate) imports: Vec<Import>,
    /// For each lexeme, the innermost frame holding it (an opener and its
    /// closer belong to the frame around them).
    pub(crate) frame_of: Vec<Option<u32>>,
    /// For each lexeme, the field (key lexeme) it belongs to: a key, its
    /// value's lexemes, its list's items.
    pub(crate) key_of: Vec<Option<u32>>,
    /// For each lexeme, the header it is part of.
    pub(crate) header_of: Vec<Option<u32>>,
    /// For each lexeme, the `use` statement it is part of.
    pub(crate) import_of: Vec<Option<u32>>,
    /// Whether a lexeme is a `use` binding's imported name (not an alias).
    pub(crate) binding: Vec<bool>,
    /// The state each lexeme was read in, and the frame then innermost.
    pub(crate) before: Vec<(Expect, Option<u32>)>,
    /// The state after each lexeme, and the frame then innermost.
    pub(crate) after: Vec<(Expect, Option<u32>)>,
}

impl Syntax {
    /// The text of lexeme `i`.
    pub(crate) fn text<'t>(&self, text: &'t str, i: u32) -> &'t str {
        self.lexemes[i as usize].text(text)
    }

    /// The entity frame `frame` is in (itself included); `None` inside a
    /// define block or outside any entity.
    pub(crate) fn entity_frame(&self, mut frame: Option<u32>) -> Option<u32> {
        while let Some(f) = frame {
            match self.frames[f as usize].kind {
                FrameKind::Entity => return Some(f),
                FrameKind::Define => return None,
                _ => frame = self.frames[f as usize].parent,
            }
        }
        None
    }

    /// Whether lexeme `i`'s field belongs to its entity's own body (not to
    /// a nested block or a define block): the frame of its key is an
    /// entity's.
    pub(crate) fn in_own_body(&self, key: u32) -> bool {
        self.frame_of[key as usize]
            .is_some_and(|f| self.frames[f as usize].kind == FrameKind::Entity)
    }

    /// The entity kind (its keyword's text) a frame's entity has.
    pub(crate) fn kind_of_frame<'t>(&self, text: &'t str, frame: u32) -> Option<&'t str> {
        let header = self.frames[frame as usize].header?;
        Some(self.text(text, self.headers[header as usize].kind))
    }

    /// The kind a field (its key lexeme) belongs to, when the field is in
    /// an entity's own body.
    pub(crate) fn kind_of_key<'t>(&self, text: &'t str, key: u32) -> Option<&'t str> {
        if !self.in_own_body(key) {
            return None;
        }
        self.kind_of_frame(text, self.frame_of[key as usize]?)
    }

    /// Whether lexeme `i` is at a reference position: an entity header's
    /// name; a value or list item, in the entity's own body, of a field
    /// whose reading may name an entity (`Reading::may_name`, ADR 0093); a
    /// `use` binding's imported name. The one rule the cursor (what a word
    /// names) and semantic tokens (what is a reference) share.
    pub(crate) fn reference_position(&self, text: &str, i: u32, forms: &FieldForms) -> bool {
        let lexeme = &self.lexemes[i as usize];
        if !lexeme.is_name() {
            return false;
        }
        match self.roles[i as usize] {
            Role::Name => self.header_of[i as usize].is_some(),
            Role::ImportName => self.binding[i as usize],
            Role::Value | Role::Item => {
                let Some(key) = self.key_of[i as usize] else {
                    return false;
                };
                // A value written in a nested block is the block's.
                let own = match self.roles[i as usize] {
                    Role::Item => self.frame_of[i as usize].is_some_and(|list| {
                        self.frames[list as usize].parent == self.frame_of[key as usize]
                    }),
                    _ => self.frame_of[i as usize] == self.frame_of[key as usize],
                };
                let written = match lexeme.kind {
                    LexemeKind::RefId => Written::SchemeRef,
                    _ => Written::Bare,
                };
                own && self
                    .kind_of_key(text, key)
                    .is_some_and(|kind| forms.reading(kind, self.text(text, key)).may_name(written))
            }
            _ => false,
        }
    }

    /// Roles inside a define block: it declares nothing (W143), so its
    /// keys, values and names are not classified.
    fn flatten_defines(&mut self) {
        for i in 0..self.lexemes.len() {
            let inside = {
                let mut frame = self.frame_of[i];
                let mut define = false;
                while let Some(f) = frame {
                    if self.frames[f as usize].kind == FrameKind::Define {
                        define = true;
                        break;
                    }
                    frame = self.frames[f as usize].parent;
                }
                define
            };
            if inside
                && matches!(
                    self.roles[i],
                    Role::Key | Role::Value | Role::Item | Role::VerifyKind | Role::Name
                )
            {
                self.roles[i] = Role::Other;
            }
        }
    }
}

/// The statement start inside frame `top` (the top level when `None`).
pub(crate) fn statement_start(frames: &[Frame], top: Option<u32>) -> Expect {
    match top.map(|f| frames[f as usize].kind) {
        None => Expect::Top,
        Some(FrameKind::Entity | FrameKind::Define | FrameKind::Block) => Expect::Body,
        Some(FrameKind::List) => Expect::List,
        Some(FrameKind::Expr) => Expect::Expr,
        Some(FrameKind::Params) => Expect::Params,
    }
}

/// The state after a line's end: a statement whose value is not written on
/// its line ends there (`start`, the statement start). `next` is the lexeme
/// on a following line, when one is known: `|` continues a value, `{` a
/// header.
pub(crate) fn line_end(state: Expect, next: Option<&Lexeme>, start: Expect) -> Expect {
    let next_is = |c: char| next.is_some_and(|l| l.is_punct(c));
    match state {
        Expect::AfterValue(_) if next_is('|') => state,
        Expect::HeaderTitle(_) | Expect::HeaderOpen(_) | Expect::DefineOpen if next_is('{') => {
            state
        }
        Expect::Value(_)
        | Expect::AfterValue(_)
        | Expect::Annotation(_)
        | Expect::AnnotationArg(_)
        | Expect::Suffix(_)
        | Expect::ExprOpen(_)
        | Expect::VerifyKind
        | Expect::VerifyDesc
        | Expect::MethodName
        | Expect::MethodParams
        | Expect::MethodAfter
        | Expect::MethodArrow
        | Expect::HeaderName(_)
        | Expect::HeaderTitle(_)
        | Expect::HeaderOpen(_)
        | Expect::DefineName
        | Expect::DefineOpen => start,
        Expect::Top
        | Expect::Body
        | Expect::Use(..)
        | Expect::List
        | Expect::Expr
        | Expect::Params => state,
    }
}
