//! Derived reference fields (behavior `link_derived_references`).
//!
//! An extension field may declare `derived_from`: the host then gives the
//! field edges from the type names the entity writes in its field types or
//! method signatures, as if the entity had listed them in the field. The
//! core names no kind: which kinds derive which edges comes only from
//! [`DerivedReference`]s, built from the field registry. Each name is read
//! with the position it is written at, so the graph's finder reports it
//! (`written_tokens`, ADR 0176).

use crate::{Edge, Graph, Reading};
use specforge_common::{SourceSpan, Sym};
use specforge_parser::lex::{LexemeKind, lex};
use specforge_parser::{FieldEntry, FieldValue, MethodDecl, Written};

/// Where a derived reference field takes its targets from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivedFrom {
    /// The type names in the entity's field values written as type syntax:
    /// an identifier, `T[]` or `A | B`, with generics inside them.
    TypeExpressions,
    /// The type names in the entity's method parameter and return types.
    MethodSignatures,
}

impl DerivedFrom {
    /// The `derived_from` value a field descriptor declares.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "type_expressions" => Some(Self::TypeExpressions),
            "method_signatures" => Some(Self::MethodSignatures),
            _ => None,
        }
    }
}

/// One registered field whose edges the host derives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedReference {
    /// The kind that declares the field.
    pub source_kind: String,
    /// The field's name, which labels its edges.
    pub field: String,
    pub from: DerivedFrom,
}

/// One type expression an entity writes, and where it starts: what the derived walk reads names from.
#[derive(Clone, Copy)]
pub(crate) struct TypeExpression<'v> {
    pub text: &'v str,
    /// Where `text` starts; `None` for a value built without a position.
    pub at: Option<&'v SourceSpan>,
}

/// The type expressions a field's entry writes, with positions: an identifier's text at the entry's value span, a
/// union's members that were not written quoted at their spans, and a bare item of a list under a member of an
/// open-fields kind at its span. A single reference names an entity, not a type.
pub(crate) fn expressions<'v>(reading: &Reading, entry: &'v FieldEntry) -> Vec<TypeExpression<'v>> {
    if reading.is_single_reference() {
        return Vec::new();
    }
    match &entry.value {
        FieldValue::Identifier(expr) => vec![TypeExpression {
            text: expr,
            at: entry.value_span.as_ref(),
        }],
        FieldValue::TypeUnion(members) | FieldValue::VariantList(members) => members
            .iter()
            .filter(|m| m.written != Written::Quoted)
            .map(|m| TypeExpression {
                text: &m.text,
                at: Some(&m.span),
            })
            .collect(),
        FieldValue::List(items) if *reading == Reading::Member => items
            .iter()
            .filter(|item| item.written == Written::Bare)
            .map(|item| TypeExpression {
                text: &item.text,
                at: Some(&item.span),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The type expressions `reference` reads from a block of `kind`: its entries written as types
/// ([`DerivedFrom::TypeExpressions`]), or its methods' parameter and return types
/// ([`DerivedFrom::MethodSignatures`]). The derived walk links the names in them; the finder reports them.
fn type_expressions<'v>(
    reference: &DerivedReference,
    forms: &crate::FieldForms,
    kind: &str,
    entries: &'v [FieldEntry],
    methods: &'v [MethodDecl],
) -> Vec<TypeExpression<'v>> {
    let mut written = Vec::new();
    match reference.from {
        DerivedFrom::TypeExpressions => {
            for entry in entries {
                let reading = forms.reading(kind, entry.key.as_str());
                written.extend(expressions(&reading, entry));
            }
        }
        DerivedFrom::MethodSignatures => {
            for method in methods {
                written.extend(method.params.iter().map(|p| TypeExpression {
                    text: &p.ty,
                    at: p.ty_span.as_ref(),
                }));
                written.extend(method.returns.as_deref().map(|text| TypeExpression {
                    text,
                    at: method.returns_span.as_ref(),
                }));
            }
        }
    }
    written
}

/// The names inside a type expression, each with where it is written: the identifiers of the language's lexer
/// (`Result<A, B[]>` -> Result, A, B), at the expression's position plus the identifier's offset.
fn names(expression: TypeExpression<'_>) -> impl Iterator<Item = (&str, Option<SourceSpan>)> {
    lex(expression.text)
        .into_iter()
        .filter(|lexeme| lexeme.kind == LexemeKind::Ident)
        .map(move |lexeme| {
            (
                &expression.text[lexeme.start..lexeme.end],
                expression
                    .at
                    .map(|at| within(at, expression.text, lexeme.start, lexeme.end)),
            )
        })
}

/// The names `reference` links from one block of its kind, in written order, each with where it is written (`None`: a
/// value built without a position): the type names of the block's type expressions (`entries` as the graph build
/// stores them, and `methods`) that are not `own` and name an entity (`kind_of`) the field's declared target admits.
/// None when the field declares no target. The linker makes each an edge, the finder a token (ADR 0163).
pub(crate) fn derived_names(
    reference: &DerivedReference,
    forms: &crate::FieldForms,
    entries: &[FieldEntry],
    methods: &[MethodDecl],
    own: Option<Sym>,
    kind_of: &dyn Fn(&str) -> Option<Sym>,
) -> Vec<(Sym, Option<SourceSpan>)> {
    let kind = reference.source_kind.as_str();
    let Some(target) = forms.target(kind, &reference.field) else {
        return Vec::new();
    };
    type_expressions(reference, forms, kind, entries, methods)
        .into_iter()
        .flat_map(names)
        .map(|(name, at)| (Sym::new(name), at))
        .filter(|(name, _)| {
            Some(*name) != own && kind_of(name.as_str()).is_some_and(|k| target.admits(k.as_str()))
        })
        .collect()
}

/// The span of bytes `start..end` of `text`, which is written from `at`'s start (lines counted from the newlines
/// before each offset; columns are 1-based bytes).
fn within(at: &SourceSpan, text: &str, start: usize, end: usize) -> SourceSpan {
    let position = |offset: usize| {
        let before = &text.as_bytes()[..offset];
        match before.iter().rposition(|&b| b == b'\n') {
            None => (at.start_line, at.start_col + offset),
            Some(newline) => (
                at.start_line + before.iter().filter(|&&b| b == b'\n').count(),
                offset - newline,
            ),
        }
    };
    let (start_line, start_col) = position(start);
    let (end_line, end_col) = position(end);
    SourceSpan {
        file: at.file,
        start_line,
        start_col,
        end_line,
        end_col,
    }
}

impl Graph {
    /// Add the edges of every derived reference field. Idempotent; run
    /// after references resolve, since resolution clears the edges.
    pub(crate) fn link_derived_references(
        &mut self,
        derived: &[DerivedReference],
        forms: &crate::FieldForms,
    ) {
        let mut edges: Vec<Edge> = Vec::new();
        for reference in derived {
            for node in self.nodes_by_kind(&reference.source_kind) {
                let kind_of = |name: &str| self.node(name).map(|named| named.kind.raw);
                let names = derived_names(
                    reference,
                    forms,
                    node.fields.entries(),
                    &node.methods,
                    Some(node.id.raw),
                    &kind_of,
                );
                edges.extend(names.into_iter().map(|(name, _)| Edge {
                    source: node.id.raw,
                    target: name,
                    label: Sym::new(&reference.field),
                }));
            }
        }
        for edge in edges {
            self.add_edge(edge);
        }
    }
}
