use serde::Serialize;
use specforge_common::{Diagnostic, SourceSpan, Sym, codes};

use crate::expr::SpannedExpr;
use crate::format_version::FormatVersion;

#[derive(Debug, Clone)]
pub struct SpecFile {
    pub path: Sym,
    pub imports: Vec<ImportDeclaration>,
    pub entities: Vec<Entity>,
    pub errors: Vec<ParseError>,
    /// The format version the file's `// specforge-format:` header declares;
    /// the current version when it has none.
    pub format_version: FormatVersion,
    /// What that header reports: I007 for an older version, E019 for a newer
    /// or unreadable one, spanned on the header line. The graph build
    /// reports them with the rest of the file's diagnostics.
    pub format_diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ImportKind {
    Full,
    Selective,
    Namespace,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportBinding {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportDeclaration {
    pub path: Sym,
    pub kind: ImportKind,
    pub bindings: Option<Vec<ImportBinding>>,
    pub namespace: Option<String>,
    pub is_pub: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub kind: EntityKind,
    pub id: EntityId,
    pub title: Option<String>,
    pub fields: FieldMap,
    pub span: SourceSpan,
    /// `method name(param: Type) -> Ret` members (ports define their
    /// interfaces this way). Empty for kinds that never declare methods.
    pub methods: Vec<MethodDecl>,
    /// `verify [kind] "text"` statements, in declaration order (ADR 0002,
    /// ADR 0094).
    pub obligations: Vec<VerifyStatement>,
}

/// One `method` member of an entity body.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MethodDecl {
    pub name: String,
    pub params: Vec<Parameter>,
    pub returns: Option<String>,
    /// Where `returns` is written; never serialized (a written form has no positions).
    #[serde(skip)]
    pub returns_span: Option<SourceSpan>,
    pub span: SourceSpan,
}

/// One parameter of a [`MethodDecl`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Parameter {
    pub name: String,
    pub ty: String,
    /// Where `ty` is written; never serialized.
    #[serde(skip)]
    pub ty_span: Option<SourceSpan>,
    /// Declared `name?: Type`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    /// `@name` annotations after the type (`id: EntityId @optional`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct EntityKind {
    pub raw: Sym,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct EntityId {
    pub raw: Sym,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldMap {
    entries: Vec<FieldEntry>,
}

impl Default for FieldMap {
    fn default() -> Self {
        Self::new()
    }
}

impl FieldMap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, key: Sym, value: FieldValue) {
        self.entries.push(FieldEntry {
            key,
            value,
            annotations: Vec::new(),
            value_span: None,
        });
    }

    /// Append an entry as is, keeping its value span.
    pub fn push_entry(&mut self, entry: FieldEntry) {
        self.entries.push(entry);
    }

    pub fn get(&self, key: &str) -> Option<&FieldValue> {
        // C3-10: intern the query once, then compare interned symbols
        // (u32 eq) per entry — previously every entry's key was resolved
        // to &str for a full string compare.
        let key_sym = Sym::new(key);
        self.entries
            .iter()
            .find(|e| e.key == key_sym)
            .map(|e| &e.value)
    }

    pub fn entries(&self) -> &[FieldEntry] {
        &self.entries
    }

    /// The entries, for passes that rewrite values in place (the semantic
    /// phase coerces values to their declared field types).
    pub fn entries_mut(&mut self) -> &mut [FieldEntry] {
        &mut self.entries
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Annotation {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldEntry {
    pub key: Sym,
    pub value: FieldValue,
    pub annotations: Vec<Annotation>,
    /// Where the value is written, so diagnostics about the value (E061)
    /// point at it rather than at the whole entity.
    pub value_span: Option<SourceSpan>,
}

/// One item of a list as written: its text, the exact source span of its
/// token (quotes included), and how it was written. What the item
/// references is its field's reading, never decided here
/// (`specforge_graph::Reading::references`).
#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    /// The item's text; a quoted item's is its unquoted content.
    pub text: String,
    pub span: SourceSpan,
    pub written: Written,
}

/// One member of a union as written: a union field's (`kind Open | "none"`) or a union body's
/// (`type Kind = A | B`). `text` is what the value held before members kept positions: a union
/// field's quoted member keeps its quotes, a union body's string variant does not; `written` says
/// which it was either way.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeMember {
    pub text: String,
    pub span: SourceSpan,
    pub written: Written,
}

impl PartialEq<&str> for TypeMember {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

impl PartialEq<String> for TypeMember {
    fn eq(&self, other: &String) -> bool {
        &self.text == other
    }
}

/// How a list item was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// A bare word: `alpha`.
    Bare,
    /// A scheme reference ID: `gh.issue:7`.
    SchemeRef,
    /// A quoted string: `"alpha"`.
    Quoted,
    /// An integer or `true` / `false`.
    Literal,
}

/// The field the parser gives a union body (`type X = a | b`) its variants
/// under. The name is the parser's own, not the user's: a field the user
/// writes as `values [a, b]` is a [`FieldValue::List`], under its own key, and
/// is not a union.
pub const UNION_VARIANTS_FIELD: &str = "variants";

/// The field of a scheme reference (`ref jira.issue:PROJ-1 "…"`, kind
/// [`specforge_common::structural::REF`]) holding its scheme (`jira`). The
/// name is the parser's own: no extension declares it.
pub const REF_SCHEME_FIELD: &str = "scheme";

#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    String(String),
    /// `[a, "b", 1, gh.issue:7]` as written: each item keeps its text, span
    /// and how it was written. What it references is its field's reading
    /// (`specforge_graph::Reading::references`), never decided here.
    List(Vec<ListItem>),
    /// A union body's variants (`type X = a | b`), under
    /// [`UNION_VARIANTS_FIELD`] only.
    VariantList(Vec<TypeMember>),
    Block(FieldMap),
    /// First-class formal expressions: `metric expr { a < 10ms, b > 5 }`.
    /// Spans are absolute file positions.
    Expression(Vec<SpannedExpr>),
    /// Union-typed field declaration value: `query_scope string | string[]`.
    /// Each element is the declared type's source text. (RES-20 direction.)
    TypeUnion(Vec<TypeMember>),
    Integer(i64),
    Boolean(bool),
    Date(String),
    Identifier(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifyStatement {
    pub kind: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: SourceSpan,
}

impl From<&ParseError> for specforge_common::Diagnostic {
    fn from(err: &ParseError) -> Self {
        specforge_common::Diagnostic::new(codes::E001, &err.message).with_span(err.span.clone())
    }
}
