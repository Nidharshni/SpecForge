//! What an entity holds, as every output writes it (ADR 0222): its fields' values
//! ([`field_form::write`](crate::field_form::write)), the annotations written after them, and its
//! methods; its `verify` statements are [`Obligation`]s (ADR 0094). No position and no parse tree. The
//! graph, context and markdown exports, inspect (MCP, the context prompt, the hover), the outline and
//! the graph delta read it; nothing else writes a node's members.

use std::collections::BTreeMap;

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_parser::{Annotation, FieldMap, FieldValue, MethodDecl, Parameter, VerifyStatement};

use crate::Node;
use crate::field_form::{FieldJson, spell, write_fields};

/// One `verify` statement as every output writes it. An entity's statements are its own
/// member (`Node::obligations`, ADR 0094); `verify` is reserved syntax (ADR 0002) whose meaning
/// the extensions supply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[shape(define = "VerifyStatement")]
pub struct Obligation {
    pub kind: String,
    pub description: String,
}

impl Obligation {
    pub fn of(statement: &VerifyStatement) -> Self {
        Self {
            kind: statement.kind.clone(),
            description: statement.description.clone(),
        }
    }
}

/// One `@name ["value"]` annotation as written after a field's value or a parameter's type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[shape(define = "AnnotationJson")]
pub struct AnnotationJson {
    pub name: String,
    /// Absent when the annotation writes no value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl AnnotationJson {
    fn of(annotation: &Annotation) -> Self {
        Self {
            name: annotation.name.clone(),
            value: annotation.value.clone(),
        }
    }

    /// `@name`, or `@name "value"`: as `specforge format` writes it.
    pub fn spelled(&self) -> String {
        match &self.value {
            Some(value) => format!("@{} {}", self.name, spell::quoted(value)),
            None => format!("@{}", self.name),
        }
    }
}

/// One parameter of a method as written: `name[?]: type [@annotations]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[shape(define = "ParameterJson")]
pub struct ParameterJson {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// Present, true, when it is written `name?: Type`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<AnnotationJson>,
}

impl ParameterJson {
    fn of(parameter: &Parameter) -> Self {
        Self {
            name: parameter.name.clone(),
            ty: parameter.ty.clone(),
            optional: parameter.optional,
            annotations: parameter
                .annotations
                .iter()
                .map(AnnotationJson::of)
                .collect(),
        }
    }

    fn spelled(&self) -> String {
        let optional = if self.optional { "?" } else { "" };
        let mut text = format!("{}{optional}: {}", self.name, self.ty);
        for annotation in &self.annotations {
            text.push(' ');
            text.push_str(&annotation.spelled());
        }
        text
    }
}

/// One `method` member as every output writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[shape(define = "MethodJson")]
pub struct MethodJson {
    pub name: String,
    pub params: Vec<ParameterJson>,
    /// Absent when the method writes no return type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
}

impl MethodJson {
    /// The one conversion from the parse: the span and every position stay behind.
    pub fn of(method: &MethodDecl) -> Self {
        Self {
            name: method.name.clone(),
            params: method.params.iter().map(ParameterJson::of).collect(),
            returns: method.returns.clone(),
        }
    }

    /// `name(a: T, b?: U @ann) -> R`: the method line `specforge format` writes, without `method`.
    /// What the outline, the hover and the review document show.
    pub fn signature(&self) -> String {
        let params: Vec<String> = self.params.iter().map(ParameterJson::spelled).collect();
        let returns = self
            .returns
            .as_ref()
            .map(|r| format!(" -> {r}"))
            .unwrap_or_default();
        format!("{}({}){returns}", self.name, params.join(", "))
    }
}

/// What a node holds besides its id, kind, title and obligations, written.
#[derive(Debug, Clone, PartialEq)]
pub struct Members {
    /// Each field's value by [`write`], by name.
    pub fields: BTreeMap<String, FieldJson>,
    /// Each annotated field's annotations, in written order, by its path: its name, or `outer.key`
    /// for an entry of the block field `outer` (any depth). A field with none is absent.
    pub annotations: BTreeMap<String, Vec<AnnotationJson>>,
    /// Its methods, in written order.
    pub methods: Vec<MethodJson>,
}

impl Members {
    /// The members of a block written with `fields` and `methods`. A key written twice is its first
    /// entry, at any depth (ADR 0215 D5): on a node that is the only one.
    pub fn new(fields: &FieldMap, methods: &[MethodDecl]) -> Members {
        let mut annotations = BTreeMap::new();
        collect(fields, "", &mut annotations);
        Members {
            fields: write_fields(fields),
            annotations,
            methods: methods.iter().map(MethodJson::of).collect(),
        }
    }

    /// `Members::new(&node.fields, &node.methods)`.
    pub fn of(node: &Node) -> Members {
        Members::new(&node.fields, &node.methods)
    }

    /// The field (top-level name) an annotation path belongs to: `outer` for `outer.key`.
    pub fn field_of(path: &str) -> &str {
        path.split_once('.').map_or(path, |(outer, _)| outer)
    }
}

/// The annotations of each first entry of a key in `map` into `annotations` under `prefix`, a block's
/// entries below it.
fn collect(map: &FieldMap, prefix: &str, annotations: &mut BTreeMap<String, Vec<AnnotationJson>>) {
    let mut seen = Vec::new();
    for entry in map.entries() {
        if seen.contains(&entry.key) {
            continue;
        }
        seen.push(entry.key);
        let path = format!("{prefix}{}", entry.key);
        if !entry.annotations.is_empty() {
            annotations.insert(
                path.clone(),
                entry.annotations.iter().map(AnnotationJson::of).collect(),
            );
        }
        if let FieldValue::Block(inner) = &entry.value {
            collect(inner, &format!("{path}."), annotations);
        }
    }
}
