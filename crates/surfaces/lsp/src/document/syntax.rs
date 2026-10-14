//! A document's lexemes (`specforge_parser::lex`), each with its role, and
//! the entity bodies, nested blocks, expression groups, lists and parameter
//! lists they open: the structure the cursor and the semantic tokens read.
//! A line that starts a top-level form (`lex::form_starts`, where the
//! parser's recovery cuts too) closes every frame left open before it.
//! Nothing here names a kind or a field: kinds and fields are read as
//! structure; the registries are consulted by the readers.

use specforge_parser::lex::{self, Lexeme, LexemeKind, lex};
use specforge_registry::{FieldRegistry, FieldType};

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

/// Punctuation that continues a value after an atom.
fn continues_value(c: char) -> bool {
    matches!(c, '|' | '<' | ',' | '?' | ':' | '-' | '(')
}

impl Syntax {
    /// The syntax of `text`.
    pub(crate) fn read(text: &str) -> Syntax {
        let lexemes = lex(text);
        let form_starts = lex::form_starts(text, &lexemes);
        let n = lexemes.len();
        let mut walk = Walk {
            text,
            syntax: Syntax {
                roles: vec![Role::Other; n],
                frames: Vec::new(),
                headers: Vec::new(),
                imports: Vec::new(),
                frame_of: vec![None; n],
                key_of: vec![None; n],
                header_of: vec![None; n],
                import_of: vec![None; n],
                binding: vec![false; n],
                before: vec![(Expect::Top, None); n],
                after: vec![(Expect::Top, None); n],
                lexemes,
            },
            state: Expect::Top,
            top: None,
            prev: None,
            form_starts,
            next_form: 0,
        };
        walk.run();
        let mut syntax = walk.syntax;
        syntax.flatten_defines();
        syntax
    }

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
    /// the registry does not type as `Enum`, `Bool`, `Integer`, `String`,
    /// `StringList` (but for an unquoted item, which the graph links) or
    /// `Block`; a `use` binding's imported name. The one rule the cursor
    /// (what a word names), completion (where entity IDs complete; a string
    /// list's declared strings complete only refs) and semantic tokens
    /// (what is a reference) share.
    pub(crate) fn reference_position(&self, text: &str, i: u32, fields: &FieldRegistry) -> bool {
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
                let item = self.roles[i as usize] == Role::Item;
                own && self
                    .kind_of_key(text, key)
                    .is_some_and(|kind| may_reference(fields, kind, self.text(text, key), item))
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

/// Whether a name written as a value of a field, or as an item of its
/// list when `item`, may name an entity: the registry does not type the
/// field as a value that names none. A name written unquoted in any list
/// is linked to the entity it names (the graph reads every such list as
/// references), so a string list's unquoted items are references too; its
/// strings are strings.
pub(crate) fn may_reference(fields: &FieldRegistry, kind: &str, field: &str, item: bool) -> bool {
    match fields.get(kind, field).map(|entry| entry.field_type()) {
        None | Some(FieldType::Reference | FieldType::ReferenceList) => true,
        Some(FieldType::StringList) => item,
        Some(_) => false,
    }
}

/// The walk over a document's lexemes that assigns roles and frames.
struct Walk<'t> {
    text: &'t str,
    syntax: Syntax,
    state: Expect,
    /// The innermost open frame.
    top: Option<u32>,
    /// The last lexeme read that is not a comment.
    prev: Option<usize>,
    /// The lexemes that start a top-level form (`lex::form_starts`), in
    /// order, and how many of them the walk has passed: a frame open at one
    /// is closed there.
    form_starts: Vec<usize>,
    next_form: usize,
}

impl Walk<'_> {
    fn run(&mut self) {
        for i in 0..self.syntax.lexemes.len() {
            let lexeme = self.syntax.lexemes[i];
            if lexeme.kind == LexemeKind::Comment {
                self.syntax.roles[i] = Role::Comment;
                self.syntax.frame_of[i] = self.top;
                self.syntax.before[i] = (self.state, self.top);
                self.syntax.after[i] = (self.state, self.top);
                continue;
            }
            let newline = self
                .prev
                .is_some_and(|p| self.newline_between(self.syntax.lexemes[p].end, lexeme.start));
            let starts_form = self.form_starts.get(self.next_form) == Some(&i);
            if starts_form {
                self.next_form += 1;
            }
            if starts_form && self.top.is_some() {
                // A line that starts a top-level form closes whatever is
                // open (`lex::form_starts`, where the parser's recovery cuts
                // too: a block being typed never takes the forms after it).
                self.top = None;
                self.state = Expect::Top;
            } else if newline {
                self.state = self.at_line_end(self.state, Some(&lexeme));
            }
            self.syntax.frame_of[i] = self.top;
            // A lexeme a state does not take ends the statement; it is
            // read again as the next statement's start.
            for _ in 0..4 {
                self.syntax.before[i] = (self.state, self.top);
                if self.step(i) {
                    break;
                }
                self.state = self.statement_start();
                self.syntax.frame_of[i] = self.top;
            }
            self.syntax.after[i] = (self.state, self.top);
            self.prev = Some(i);
        }
    }

    fn newline_between(&self, from: usize, to: usize) -> bool {
        self.text.as_bytes()[from..to].contains(&b'\n')
    }

    /// The state at a line's end: a statement whose value is not written
    /// on its line ends there. `next` is the lexeme that follows, when one
    /// does: an operator that continues a value keeps it open.
    fn at_line_end(&self, state: Expect, next: Option<&Lexeme>) -> Expect {
        line_end(state, next, self.statement_start_of(self.top))
    }

    fn statement_start(&self) -> Expect {
        self.statement_start_of(self.top)
    }

    fn statement_start_of(&self, top: Option<u32>) -> Expect {
        statement_start(&self.syntax.frames, top)
    }

    /// The lexeme after `i` that is not a comment, when it is on the same
    /// line.
    fn next_on_line(&self, i: usize) -> Option<Lexeme> {
        let lexemes = &self.syntax.lexemes;
        let next = lexemes[i + 1..]
            .iter()
            .find(|l| l.kind != LexemeKind::Comment)?;
        (!self.newline_between(lexemes[i].end, next.start)).then_some(*next)
    }

    fn set(&mut self, i: usize, role: Role) {
        self.syntax.roles[i] = role;
    }

    /// Open a frame at the lexeme being read; the walk then expects
    /// `inside`, and `resume` after it closes.
    fn open(
        &mut self,
        kind: FrameKind,
        key: Option<u32>,
        header: Option<u32>,
        resume: Expect,
        inside: Expect,
    ) {
        self.syntax.frames.push(Frame {
            kind,
            parent: self.top,
            close: None,
            key,
            header,
            resume,
        });
        self.top = Some(self.syntax.frames.len() as u32 - 1);
        self.state = inside;
    }

    /// Close the innermost frame `closer` closes, and every frame above it.
    /// `false` when no open frame matches.
    fn close(&mut self, i: usize, closer: char) -> bool {
        let matches = |kind: FrameKind| match closer {
            '}' => matches!(
                kind,
                FrameKind::Entity | FrameKind::Define | FrameKind::Block | FrameKind::Expr
            ),
            ']' => kind == FrameKind::List,
            _ => kind == FrameKind::Params,
        };
        let mut frame = self.top;
        while let Some(f) = frame {
            let current = self.syntax.frames[f as usize];
            if matches(current.kind) {
                self.syntax.frames[f as usize].close = Some(i as u32);
                self.top = current.parent;
                self.state = current.resume;
                self.syntax.frame_of[i] = self.top;
                return true;
            }
            // A brace never closes past an entity: `]` and `)` stop at one.
            if closer != '}'
                && matches!(
                    current.kind,
                    FrameKind::Entity | FrameKind::Define | FrameKind::Block | FrameKind::Expr
                )
            {
                return false;
            }
            frame = current.parent;
        }
        false
    }

    /// Start a `use` statement at lexeme `i` (`use`, or `pub` before it).
    fn start_import(&mut self, i: usize, step: UseStep) {
        self.set(i, Role::Keyword);
        self.syntax.imports.push(Import {
            first: i as u32,
            last: i as u32,
            path: None,
        });
        let import = self.syntax.imports.len() as u32 - 1;
        self.syntax.import_of[i] = Some(import);
        self.state = Expect::Use(step, import);
    }

    /// Read lexeme `i` as part of `use` statement `import`, then expect
    /// `next` (the statement's end when `None`).
    fn import_part(&mut self, i: usize, import: u32, role: Role, next: Option<UseStep>) {
        self.set(i, role);
        self.syntax.import_of[i] = Some(import);
        self.syntax.imports[import as usize].last = i as u32;
        self.state = match next {
            Some(step) => Expect::Use(step, import),
            None => self.statement_start(),
        };
    }

    /// Read lexeme `i` as an atom of a value read in `ctx`.
    fn value_part(&mut self, i: usize, ctx: Ctx, role: Role, next: Expect) {
        self.set(i, role);
        self.syntax.key_of[i] = ctx.key;
        self.state = next;
    }

    /// Open an entity's body at `{` (lexeme `i`) after header `header`.
    fn open_entity(&mut self, i: usize, header: u32) {
        self.set(i, Role::Other);
        self.open(
            FrameKind::Entity,
            None,
            Some(header),
            Expect::Top,
            Expect::Body,
        );
    }

    /// Read lexeme `i` in the current state; `false` when the state does
    /// not take it (the statement ended before it).
    fn step(&mut self, i: usize) -> bool {
        let lexeme = self.syntax.lexemes[i];
        let kind = lexeme.kind;
        let text = self.text;
        let word = (kind == LexemeKind::Ident).then(|| lexeme.text(text));
        let punct = match kind {
            LexemeKind::Punct(c) => Some(c),
            _ => None,
        };
        match self.state {
            Expect::Top => {
                self.top_statement(i, kind, word);
                true
            }
            Expect::HeaderName(header) => match kind {
                LexemeKind::Ident | LexemeKind::RefId => {
                    self.set(i, Role::Name);
                    self.syntax.header_of[i] = Some(header);
                    self.syntax.headers[header as usize].name = Some(i as u32);
                    self.state = Expect::HeaderTitle(header);
                    true
                }
                LexemeKind::Str { .. } => {
                    // `spec "Name" {`: a title and no name.
                    self.set(i, Role::Title);
                    self.syntax.header_of[i] = Some(header);
                    self.state = Expect::HeaderOpen(header);
                    true
                }
                LexemeKind::Punct('{') => {
                    self.open_entity(i, header);
                    true
                }
                _ => false,
            },
            Expect::HeaderTitle(header) => match kind {
                LexemeKind::Str { .. } => {
                    self.set(i, Role::Title);
                    self.syntax.header_of[i] = Some(header);
                    self.state = Expect::HeaderOpen(header);
                    true
                }
                LexemeKind::Punct('{') => {
                    self.open_entity(i, header);
                    true
                }
                LexemeKind::Punct('=') => {
                    // A union: its variants are values of no field.
                    self.set(i, Role::Other);
                    self.state = Expect::Value(Ctx {
                        key: None,
                        atoms: Role::Value,
                    });
                    true
                }
                _ => false,
            },
            Expect::HeaderOpen(header) => {
                if punct == Some('{') {
                    self.open_entity(i, header);
                    return true;
                }
                false
            }
            Expect::DefineName | Expect::DefineOpen => match kind {
                LexemeKind::Ident if self.state == Expect::DefineName => {
                    self.set(i, Role::Other);
                    self.state = Expect::DefineOpen;
                    true
                }
                LexemeKind::Punct('{') => {
                    self.set(i, Role::Other);
                    self.open(FrameKind::Define, None, None, Expect::Top, Expect::Body);
                    true
                }
                _ => false,
            },
            Expect::Use(step, import) => self.import_step(i, step, import, kind, word, punct),
            Expect::Body => {
                self.body_statement(i, kind, word);
                true
            }
            Expect::VerifyKind | Expect::VerifyDesc => match kind {
                LexemeKind::Ident if self.state == Expect::VerifyKind => {
                    self.set(i, Role::VerifyKind);
                    self.state = Expect::VerifyDesc;
                    true
                }
                LexemeKind::Str { .. } => {
                    self.set(i, Role::Value);
                    self.state = Expect::AfterValue(Ctx {
                        key: None,
                        atoms: Role::Other,
                    });
                    true
                }
                _ => false,
            },
            Expect::MethodName => {
                if kind == LexemeKind::Ident {
                    self.set(i, Role::Name);
                    self.state = Expect::MethodParams;
                    return true;
                }
                false
            }
            Expect::MethodParams => {
                if punct == Some('(') {
                    self.set(i, Role::Other);
                    self.open(
                        FrameKind::Params,
                        None,
                        None,
                        Expect::MethodAfter,
                        Expect::Params,
                    );
                    return true;
                }
                false
            }
            Expect::MethodAfter => {
                if punct == Some('-') {
                    self.set(i, Role::Other);
                    self.state = Expect::MethodArrow;
                    return true;
                }
                false
            }
            Expect::MethodArrow => {
                if punct == Some('>') {
                    self.set(i, Role::Other);
                    self.state = Expect::Value(Ctx {
                        key: None,
                        atoms: Role::Other,
                    });
                    return true;
                }
                false
            }
            Expect::Value(ctx) => self.value_atom(i, ctx, kind, word, punct),
            Expect::AfterValue(ctx) => self.after_value(i, ctx, punct),
            Expect::Suffix(ctx) => {
                if punct == Some(']') {
                    self.value_part(i, ctx, Role::Other, Expect::AfterValue(ctx));
                    return true;
                }
                false
            }
            Expect::Annotation(ctx) => {
                if kind == LexemeKind::Ident {
                    self.value_part(i, ctx, Role::Other, Expect::AnnotationArg(ctx));
                    return true;
                }
                false
            }
            Expect::AnnotationArg(ctx) => {
                if kind.is_str() {
                    self.value_part(i, ctx, Role::Other, Expect::AfterValue(ctx));
                    return true;
                }
                self.state = Expect::AfterValue(ctx);
                self.step(i)
            }
            Expect::ExprOpen(ctx) => {
                if punct == Some('{') {
                    self.value_part(i, ctx, Role::Other, Expect::Expr);
                    self.open(
                        FrameKind::Expr,
                        ctx.key,
                        None,
                        Expect::AfterValue(ctx),
                        Expect::Expr,
                    );
                    return true;
                }
                self.state = Expect::Value(ctx);
                self.step(i)
            }
            Expect::List | Expect::Expr | Expect::Params => {
                self.inside_group(i, kind, punct);
                true
            }
        }
    }

    /// A top-level statement's first lexeme.
    fn top_statement(&mut self, i: usize, kind: LexemeKind, word: Option<&str>) {
        match (kind, word) {
            (_, Some("use")) => self.start_import(i, UseStep::Start),
            (_, Some("pub"))
                if self
                    .next_on_line(i)
                    .is_some_and(|next| next.text(self.text) == "use") =>
            {
                self.start_import(i, UseStep::Pub)
            }
            (_, Some("define")) => {
                self.set(i, Role::Keyword);
                self.state = Expect::DefineName;
            }
            (LexemeKind::Ident, _) => {
                self.set(i, Role::Kind);
                self.syntax.headers.push(Header {
                    kind: i as u32,
                    name: None,
                });
                let header = self.syntax.headers.len() as u32 - 1;
                self.syntax.header_of[i] = Some(header);
                self.state = Expect::HeaderName(header);
            }
            (LexemeKind::Punct('}'), _) => {
                self.close(i, '}');
            }
            _ => self.set(i, Role::Other),
        }
    }

    /// A lexeme of a `use` statement; `false` when it ends the statement.
    fn import_step(
        &mut self,
        i: usize,
        step: UseStep,
        import: u32,
        kind: LexemeKind,
        word: Option<&str>,
        punct: Option<char>,
    ) -> bool {
        let next = match (step, kind, word, punct) {
            (UseStep::Pub, _, Some("use"), _) => Some((Role::Keyword, Some(UseStep::Start))),
            (UseStep::Start | UseStep::Path, LexemeKind::Str { .. }, _, _) => {
                self.syntax.imports[import as usize].path = Some(i as u32);
                Some((Role::ImportPath, None))
            }
            (UseStep::Start, _, _, Some('{')) => Some((Role::Other, Some(UseStep::Binding))),
            (UseStep::Start, _, _, Some('*')) => Some((Role::Other, Some(UseStep::Star))),
            (UseStep::Binding, LexemeKind::Ident, _, _) => {
                self.syntax.binding[i] = true;
                Some((Role::ImportName, Some(UseStep::AfterBinding)))
            }
            (UseStep::Binding | UseStep::AfterBinding, _, _, Some(',')) => {
                Some((Role::Other, Some(UseStep::Binding)))
            }
            (UseStep::Binding | UseStep::AfterBinding, _, _, Some('}')) => {
                Some((Role::Other, Some(UseStep::From)))
            }
            (UseStep::AfterBinding, _, Some("as"), _) => {
                Some((Role::Keyword, Some(UseStep::Alias)))
            }
            (UseStep::Alias, LexemeKind::Ident, _, _) => {
                Some((Role::ImportName, Some(UseStep::AfterBinding)))
            }
            (UseStep::Star, _, Some("as"), _) => Some((Role::Keyword, Some(UseStep::StarAlias))),
            (UseStep::StarAlias, LexemeKind::Ident, _, _) => {
                Some((Role::ImportName, Some(UseStep::From)))
            }
            (UseStep::From, _, Some("from"), _) => Some((Role::Keyword, Some(UseStep::Path))),
            _ => None,
        };
        match next {
            Some((role, next)) => {
                self.import_part(i, import, role, next);
                true
            }
            None => false,
        }
    }

    /// A statement's first lexeme in a body: a closer, `verify`, `method`
    /// or a field's key.
    fn body_statement(&mut self, i: usize, kind: LexemeKind, word: Option<&str>) {
        // A nested block holds fields only: `verify` and `method` are keys
        // there (the grammar's `nested_block`).
        let members = self
            .top
            .is_some_and(|f| self.syntax.frames[f as usize].kind != FrameKind::Block);
        match (kind, word) {
            (LexemeKind::Punct('}'), _) => {
                if !self.close(i, '}') {
                    self.set(i, Role::Other);
                }
            }
            (_, Some("verify")) if members && self.is_verify_statement(i) => {
                self.set(i, Role::Keyword);
                self.state = Expect::VerifyKind;
            }
            (_, Some("method")) if members && self.is_method_statement(i) => {
                self.set(i, Role::Keyword);
                self.state = Expect::MethodName;
            }
            (LexemeKind::Ident, _) => {
                self.set(i, Role::Key);
                self.syntax.key_of[i] = Some(i as u32);
                self.state = Expect::Value(Ctx {
                    key: Some(i as u32),
                    atoms: Role::Value,
                });
            }
            _ => self.set(i, Role::Other),
        }
    }

    /// A value's atom; `false` when a closer ends the statement.
    fn value_atom(
        &mut self,
        i: usize,
        ctx: Ctx,
        kind: LexemeKind,
        word: Option<&str>,
        punct: Option<char>,
    ) -> bool {
        match (kind, word, punct) {
            (LexemeKind::Str { .. }, _, _) => {
                self.value_part(i, ctx, ctx.atoms, Expect::AfterValue(ctx));
            }
            (_, Some("expr"), _) if self.next_on_line(i).is_some_and(|next| next.is_punct('{')) => {
                self.value_part(i, ctx, Role::Keyword, Expect::ExprOpen(ctx));
            }
            (_, Some("fn"), _) if self.next_on_line(i).is_some_and(|next| next.is_punct('(')) => {
                self.value_part(i, ctx, Role::Keyword, Expect::Value(ctx));
            }
            (LexemeKind::Ident | LexemeKind::RefId | LexemeKind::Number, _, _) => {
                self.value_part(i, ctx, ctx.atoms, Expect::AfterValue(ctx));
            }
            (_, _, Some('[')) => {
                self.value_part(i, ctx, Role::Other, Expect::List);
                self.open(
                    FrameKind::List,
                    ctx.key,
                    None,
                    Expect::AfterValue(ctx),
                    Expect::List,
                );
            }
            (_, _, Some('{')) => {
                self.value_part(i, ctx, Role::Other, Expect::Body);
                self.open(
                    FrameKind::Block,
                    ctx.key,
                    None,
                    Expect::AfterValue(ctx),
                    Expect::Body,
                );
            }
            (_, _, Some('(')) => self.open_parens(i, ctx),
            (_, _, Some('}' | ']' | ')')) => return false,
            _ => self.value_part(i, ctx, Role::Other, Expect::Value(ctx)),
        }
        true
    }

    /// `(` in a value: a parenthesized type (`fn(…)`, `()`).
    fn open_parens(&mut self, i: usize, ctx: Ctx) {
        self.value_part(i, ctx, Role::Other, Expect::Params);
        self.open(
            FrameKind::Params,
            ctx.key,
            None,
            Expect::AfterValue(ctx),
            Expect::Params,
        );
    }

    /// The lexeme after a value's atom; `false` when it starts the next
    /// statement (an atom with no operator before it, or a closer).
    fn after_value(&mut self, i: usize, ctx: Ctx, punct: Option<char>) -> bool {
        match punct {
            Some('[') if self.array_suffix(i) => {
                self.value_part(i, ctx, Role::Other, Expect::Suffix(ctx));
            }
            Some('(') => self.open_parens(i, ctx),
            Some(c) if continues_value(c) => {
                self.value_part(i, ctx, Role::Other, Expect::Value(ctx));
            }
            Some('@') => self.value_part(i, ctx, Role::Other, Expect::Annotation(ctx)),
            Some('}' | ']' | ')') => return false,
            Some(_) => self.value_part(i, ctx, Role::Other, Expect::AfterValue(ctx)),
            None => return false,
        }
        true
    }

    /// A lexeme inside a list, an expression group or parentheses.
    fn inside_group(&mut self, i: usize, kind: LexemeKind, punct: Option<char>) {
        let state = self.state;
        let key = self.top.and_then(|f| self.syntax.frames[f as usize].key);
        self.syntax.key_of[i] = key;
        match punct {
            Some('}') => {
                if !self.close(i, '}') {
                    self.set(i, Role::Other);
                }
            }
            Some(']') if state == Expect::List => {
                self.close(i, ']');
            }
            Some(')') if state == Expect::Params => {
                self.close(i, ')');
            }
            Some('(') if state == Expect::Params => {
                self.open(FrameKind::Params, key, None, Expect::Params, Expect::Params);
            }
            _ => {
                let role = match (state, kind) {
                    (
                        Expect::List,
                        LexemeKind::Ident
                        | LexemeKind::RefId
                        | LexemeKind::Str { .. }
                        | LexemeKind::Number,
                    ) => Role::Item,
                    (Expect::Expr, LexemeKind::Number) => Role::Value,
                    _ => Role::Other,
                };
                self.set(i, role);
            }
        }
    }

    /// Whether `verify` (lexeme `i`) starts a verify statement, not a field
    /// named verify: a string follows, or a kind then a string, or nothing
    /// yet on its line (being typed).
    fn is_verify_statement(&self, i: usize) -> bool {
        let Some(next) = self.next_on_line(i) else {
            return true;
        };
        match next.kind {
            LexemeKind::Str { .. } => true,
            LexemeKind::Ident => {
                let at = self.syntax.lexemes[i + 1..]
                    .iter()
                    .position(|l| l.start == next.start)
                    .map_or(i + 1, |p| i + 1 + p);
                self.next_on_line(at)
                    .is_none_or(|after| after.kind.is_str())
            }
            _ => false,
        }
    }

    /// Whether `method` (lexeme `i`) starts a method statement, not a field
    /// named method: a name follows, then `(` (or nothing yet: typed).
    fn is_method_statement(&self, i: usize) -> bool {
        let Some(name) = self.next_on_line(i) else {
            return false;
        };
        if name.kind != LexemeKind::Ident {
            return false;
        }
        let at = self.syntax.lexemes[i + 1..]
            .iter()
            .position(|l| l.start == name.start)
            .map_or(i + 1, |p| i + 1 + p);
        self.next_on_line(at)
            .is_none_or(|after| after.is_punct('('))
    }

    /// Whether `[` (lexeme `i`) is an array suffix: written right after
    /// the type and closed right away (`Type[]`).
    fn array_suffix(&self, i: usize) -> bool {
        let lexemes = &self.syntax.lexemes;
        let open = lexemes[i];
        i > 0
            && lexemes[i - 1].end == open.start
            && lexemes
                .get(i + 1)
                .is_some_and(|close| close.is_punct(']') && close.start == open.end)
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
