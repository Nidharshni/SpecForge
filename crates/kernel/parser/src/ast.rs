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
}

/// One `method` member of an entity body.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MethodDecl {
    pub name: String,
    pub params: Vec<Parameter>,
    pub returns: Option<String>,
    pub span: SourceSpan,
}

/// One parameter of a [`MethodDecl`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Parameter {
    pub name: String,
    pub ty: String,
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

/// One item of a reference list: the target ID plus the exact source span
/// of its identifier token, so diagnostics (E003) can point at the token
/// instead of the whole entity block.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedRef {
    pub id: String,
    pub span: SourceSpan,
}

impl SpannedRef {
    pub fn as_str(&self) -> &str {
        &self.id
    }
}

/// The field the parser gives a union body (`type X = a | b`) its variants
/// under. The name is the parser's own, not the user's: a field the user
/// writes as `values [a, b]` is also a [`FieldValue::VariantList`], under
/// its own key, and is not a union.
pub const UNION_VARIANTS_FIELD: &str = "variants";

/// The field of a scheme reference (`ref jira.issue:PROJ-1 "…"`, kind
/// [`specforge_common::structural::REF`]) holding its scheme (`jira`). The
/// name is the parser's own: no extension declares it.
pub const REF_SCHEME_FIELD: &str = "scheme";

#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    String(String),
    ReferenceList(Vec<SpannedRef>),
    VariantList(Vec<String>),
    StringList(Vec<String>),
    /// A list containing items of mixed types (e.g., strings, integers, booleans).
    /// Preserves per-item type information instead of flattening to StringList.
    MixedList(Vec<FieldValue>),
    Block(FieldMap),
    VerifyList(Vec<VerifyStatement>),
    /// First-class formal expressions: `metric expr { a < 10ms, b > 5 }`.
    /// Spans are absolute file positions.
    Expression(Vec<SpannedExpr>),
    /// Union-typed field declaration value: `query_scope string | string[]`.
    /// Each element is the declared type's source text. (RES-20 direction.)
    TypeUnion(Vec<String>),
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

#[derive(Debug, Clone, Serialize)]
pub struct ParseError {
    pub message: String,
    pub span: SourceSpan,
    pub expected: Option<String>,
    pub found: Option<String>,
}

impl From<&ParseError> for specforge_common::Diagnostic {
    fn from(err: &ParseError) -> Self {
        specforge_common::Diagnostic::new(codes::E001, &err.message).with_span(err.span.clone())
    }
}
