//! Derived reference fields (behavior `link_derived_references`).
//!
//! An extension field may declare `derived_from`: the host then gives the
//! field edges from the type names the entity writes in its field types or
//! method signatures, as if the entity had listed them in the field. The
//! core names no kind: which kinds derive which edges comes only from
//! [`DerivedReference`]s, built from the field registry. Each name is read
//! with the position it is written at, so the edge records where (`Graph::written`,
//! ADR 0127).

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
pub(crate) fn type_expressions<'v>(
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
pub(crate) fn names(
    expression: TypeExpression<'_>,
) -> impl Iterator<Item = (&str, Option<SourceSpan>)> {
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
        let mut edges: Vec<(Edge, Option<SourceSpan>)> = Vec::new();
        for reference in derived {
            // The kind a name must resolve to for an edge: the field's declared target.
            let Some(target) = forms.target(&reference.source_kind, &reference.field) else {
                continue;
            };
            for node in self.nodes_by_kind(&reference.source_kind) {
                let kind = node.kind.raw.as_str();
                let written =
                    type_expressions(reference, forms, kind, node.fields.entries(), &node.methods);
                for (name, at) in written.into_iter().flat_map(names) {
                    if name == node.id.raw.as_str() {
                        continue;
                    }
                    let resolves = self
                        .node(name)
                        .is_some_and(|named| target.admits(named.kind.raw.as_str()));
                    if resolves {
                        let edge = Edge {
                            source: node.id.raw,
                            target: Sym::new(name),
                            label: Sym::new(&reference.field),
                        };
                        edges.push((edge, at));
                    }
                }
            }
        }
        for (edge, at) in edges {
            self.add_written(edge, at.as_ref());
        }
    }
}
