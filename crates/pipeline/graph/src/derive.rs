//! Derived reference fields (behavior `link_derived_references`).
//!
//! An extension field may declare `derived_from`: the host then gives the
//! field edges from the type names the entity writes in its field types or
//! method signatures, as if the entity had listed them in the field. The
//! core names no kind: which kinds derive which edges comes only from
//! [`DerivedReference`]s, built from the field registry.

use crate::{Edge, Graph};
use specforge_common::Sym;
use specforge_parser::FieldValue;
use std::collections::HashSet;

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
    /// The kind a name must resolve to for an edge.
    pub target_kind: String,
    pub from: DerivedFrom,
}

/// The names inside a type expression: `Result<A, B[]>` -> Result, A, B.
fn type_names(expr: &str) -> impl Iterator<Item = &str> {
    expr.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|name| !name.is_empty())
}

impl Graph {
    /// Add the edges of every derived reference field. Idempotent; run
    /// after references resolve, since resolution clears the edges.
    pub(crate) fn link_derived_references(
        &mut self,
        derived: &[DerivedReference],
        single_ref_fields: &HashSet<(String, String)>,
    ) {
        let mut edges: Vec<Edge> = Vec::new();
        for reference in derived {
            for node in self.nodes_by_kind(&reference.source_kind) {
                let kind = node.kind.raw.as_str();
                let mut expressions: Vec<&str> = Vec::new();
                match reference.from {
                    DerivedFrom::TypeExpressions => {
                        for entry in node.fields.entries() {
                            if single_ref_fields
                                .contains(&(kind.to_string(), entry.key.as_str().to_string()))
                            {
                                continue;
                            }
                            match &entry.value {
                                FieldValue::Identifier(expr) => expressions.push(expr),
                                FieldValue::TypeUnion(members)
                                | FieldValue::VariantList(members) => {
                                    // A quoted union member is a literal, not a type.
                                    expressions.extend(
                                        members
                                            .iter()
                                            .filter(|m| !m.starts_with('"'))
                                            .map(String::as_str),
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                    DerivedFrom::MethodSignatures => {
                        for method in &node.methods {
                            expressions.extend(method.params.iter().map(|p| p.ty.as_str()));
                            expressions.extend(method.returns.as_deref());
                        }
                    }
                }
                for name in expressions.into_iter().flat_map(type_names) {
                    if name == node.id.raw.as_str() {
                        continue;
                    }
                    let resolves = self
                        .node(name)
                        .is_some_and(|target| target.kind.raw.as_str() == reference.target_kind);
                    if resolves {
                        edges.push(Edge {
                            source: node.id.raw,
                            target: Sym::new(name),
                            label: Sym::new(&reference.field),
                        });
                    }
                }
            }
        }
        for edge in edges {
            self.add_edge(edge);
        }
    }
}
