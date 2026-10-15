//! Where entity IDs are written: an entity's declaration name, and each
//! reference (one edge of the graph) as the identifier token its field
//! holds.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{SourceSpan, Sym};
use specforge_graph::{DerivedFrom, Edge, FieldForms, Node};
use specforge_parser::FieldValue;

use super::references::reference_edges;
use super::text::SourceText;
use super::{Navigator, contains};
use crate::Refusal;
use crate::options::{Choice, OptionTable};
use crate::view::EntityField;
use specforge_parser::lex::Lexeme;

/// Where an entity is declared: its whole block, and its name token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub id: Sym,
    pub kind: Sym,
    pub block: SourceSpan,
    /// The ID as written in the declaration; the block when it could not
    /// be found there ([`Precision::Entity`]).
    pub name: SourceSpan,
    pub precision: Precision,
}

/// Which references of an entity a question asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Direction {
    /// Other entities' references to it.
    #[default]
    Incoming,
    /// Its own references to other entities.
    Outgoing,
    Both,
}

/// `specforge.find_references`' `direction` (ADR 0027).
pub const DIRECTION: OptionTable<Direction> = OptionTable {
    argument: "direction",
    choices: &[
        Choice {
            name: "incoming",
            aliases: &[],
            help: "other entities' references to it",
            value: Direction::Incoming,
        },
        Choice {
            name: "outgoing",
            aliases: &[],
            help: "its own references to other entities",
            value: Direction::Outgoing,
        },
        Choice {
            name: "both",
            aliases: &[],
            help: "",
            value: Direction::Both,
        },
    ],
    default: Some(Direction::Incoming),
};

/// A references question: which direction, and whether the entity's own
/// declaration is among the answers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReferenceQuery {
    pub direction: Direction,
    pub include_declaration: bool,
}

/// What an occurrence of an ID is: the declaration, or a reference.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Serialize, Shape)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Declaration,
    Reference,
}

/// `Token` when the span is the identifier as written; `Entity` when the
/// text was unreadable or did not spell the ID there (a stale graph), and
/// the span falls back to the holder's block.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Serialize, Shape)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    Token,
    Entity,
}

/// One place an entity's ID is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    /// The entity whose text holds the occurrence.
    pub holder: Sym,
    /// The entity it names.
    pub target: Sym,
    /// The field (the edge's label); `None` for the declaration.
    pub field: Option<Sym>,
    pub role: Role,
    pub span: SourceSpan,
    pub precision: Precision,
}

impl Occurrence {
    /// The sort key: file, then position, then what it names.
    fn key(&self) -> (Sym, usize, usize, usize, usize, Sym, Sym, Role, Option<Sym>) {
        (
            self.span.file,
            self.span.start_line,
            self.span.start_col,
            self.span.end_line,
            self.span.end_col,
            self.holder,
            self.target,
            self.role,
            self.field,
        )
    }
}

/// Sort occurrences by (file, line, column) and drop repeats.
fn sorted(mut occurrences: Vec<Occurrence>) -> Vec<Occurrence> {
    occurrences.sort_by_key(Occurrence::key);
    occurrences.dedup();
    occurrences
}

impl<F: Fn(&str) -> Option<String>> Navigator<'_, F> {
    /// The entity's block and its name token.
    pub fn definition(&self, id: &str) -> Result<Definition, Refusal<EntityField>> {
        let node = self
            .node(id)
            .map_err(|error| error.about(EntityField::EntityId))?;
        Ok(self.definition_of(node))
    }

    pub(crate) fn definition_of(&self, node: &Node) -> Definition {
        let block = node.source_span.clone();
        let name = self
            .text(block.file)
            .and_then(|text| declaration_name(&text, &block, node.id.raw.as_str()));
        let (name, precision) = match name {
            Some(name) => (name, Precision::Token),
            None => (block.clone(), Precision::Entity),
        };
        Definition {
            id: node.id.raw,
            kind: node.kind.raw,
            block,
            name,
            precision,
        }
    }

    /// The occurrences of `id`, sorted by (file, line, column).
    pub fn references(
        &self,
        id: &str,
        query: ReferenceQuery,
    ) -> Result<Vec<Occurrence>, Refusal<EntityField>> {
        let node = self
            .node(id)
            .map_err(|error| error.about(EntityField::EntityId))?;
        let mut occurrences: Vec<Occurrence> =
            reference_edges(self.view.graph(), id, query.direction)
                .flat_map(|(holder, edge)| self.edge_occurrences(holder, edge))
                .collect();
        if query.include_declaration {
            occurrences.push(self.declaration(node));
        }
        Ok(sorted(occurrences))
    }

    /// The declaration of `node`, as an occurrence.
    fn declaration(&self, node: &Node) -> Occurrence {
        let definition = self.definition_of(node);
        Occurrence {
            holder: node.id.raw,
            target: node.id.raw,
            field: None,
            role: Role::Declaration,
            span: definition.name,
            precision: definition.precision,
        }
    }

    /// The occurrence (declaration or reference) whose token holds the
    /// byte position (1-based `line` and `col` of `file`; the position
    /// just past a token's end counts): what prepareRename and a precise
    /// "go to" read. Only tokens as written answer.
    pub fn occurrence_at(&self, file: &str, line: usize, col: usize) -> Option<Occurrence> {
        let graph = self.view.graph();
        let at = SourceSpan {
            file: Sym::new(file),
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: col,
        };
        let holders = graph
            .nodes_in_file(file)
            .into_iter()
            .filter(|n| contains(&n.source_span, &at));
        for holder in holders {
            let declaration = self.declaration(holder);
            let mut candidates = vec![declaration];
            for edge in graph.edges_from(holder.id.raw.as_str()) {
                candidates.extend(self.edge_occurrences(holder, edge));
            }
            let hit = candidates
                .into_iter()
                .find(|o| o.precision == Precision::Token && contains(&o.span, &at));
            if hit.is_some() {
                return hit;
            }
        }
        None
    }

    /// The occurrences of `edge`'s target in its source: each token its
    /// field holds (a reference list item, a single reference's value; for
    /// a derived field, the names in the holder's type expressions or
    /// method signatures). Without any, the target's tokens in the
    /// holder's block. A token the text does not spell, or no token at
    /// all, is one occurrence at the holder's block, with
    /// [`Precision::Entity`]. `holder` is the edge's source.
    pub(crate) fn edge_occurrences(&self, holder: &Node, edge: &Edge) -> Vec<Occurrence> {
        let target = edge.target.as_str();
        let block = &holder.source_span;
        let occurrence = |span: SourceSpan, precision: Precision| Occurrence {
            holder: edge.source,
            target: edge.target,
            field: Some(edge.label),
            role: Role::Reference,
            span,
            precision,
        };
        let entity = || vec![occurrence(block.clone(), Precision::Entity)];
        let Some(text) = self.text(block.file) else {
            return entity();
        };

        let mut spans = written_references(holder, edge, self.view.registries().field_forms());
        if spans.is_empty() {
            spans = self.derived_references(&text, holder, edge);
        }
        if spans.is_empty() {
            // Unknown provenance (a graph built by hand): the target's
            // tokens anywhere in the holder's block but its name.
            let name = declaration_name(&text, block, holder.id.raw.as_str());
            spans = tokens_named(&text, block, target)
                .into_iter()
                .filter(|span| Some(span) != name.as_ref())
                .collect();
        }
        let mut occurrences = Vec::new();
        let mut stale = false;
        for span in spans {
            if text.spells(&span, target) {
                occurrences.push(occurrence(span, Precision::Token));
            } else {
                stale = true;
            }
        }
        if stale || occurrences.is_empty() {
            occurrences.extend(entity());
        }
        occurrences
    }

    /// The tokens a derived field's edge stands for: the target's names in
    /// the holder's field values written as type syntax, or in its method
    /// signatures (what `link_derived_references` read them from).
    fn derived_references(&self, text: &SourceText, holder: &Node, edge: &Edge) -> Vec<SourceSpan> {
        let registries = self.view.registries();
        let kind = holder.kind.raw.as_str();
        let derived = registries
            .fields()
            .get(kind, edge.label.as_str())
            .and_then(|entry| entry.declared().derived_from.as_deref())
            .and_then(DerivedFrom::parse);
        let target = edge.target.as_str();
        let forms = registries.field_forms();
        let mut spans = Vec::new();
        match derived {
            Some(DerivedFrom::TypeExpressions) => {
                for entry in holder.fields.entries() {
                    let single = forms
                        .reading(kind, entry.key.as_str())
                        .is_single_reference();
                    let typed = matches!(
                        entry.value,
                        FieldValue::Identifier(_)
                            | FieldValue::TypeUnion(_)
                            | FieldValue::VariantList(_)
                    );
                    if let (false, true, Some(span)) = (single, typed, &entry.value_span) {
                        spans.extend(tokens_named(text, span, target));
                    }
                }
            }
            Some(DerivedFrom::MethodSignatures) => {
                for method in &holder.methods {
                    // The signature: what follows the method's name.
                    let tokens = text.tokens(&method.span);
                    let open = tokens
                        .iter()
                        .position(|t| t.is_punct('('))
                        .unwrap_or(tokens.len());
                    spans.extend(
                        tokens[open..]
                            .iter()
                            .filter(|t| t.is_name() && text.token_text(t) == target)
                            .map(|t| text.span(method.span.file, t.start, t.end)),
                    );
                }
            }
            None => {}
        }
        spans
    }
}

/// The tokens `holder` writes for `edge` in the field the edge is labeled
/// with: each reference-list item naming the target, or the value of a
/// single-reference field naming it.
fn written_references(holder: &Node, edge: &Edge, forms: &FieldForms) -> Vec<SourceSpan> {
    let target = edge.target.as_str();
    let reading = forms.reading(holder.kind.raw.as_str(), edge.label.as_str());
    let mut spans = Vec::new();
    for entry in holder
        .fields
        .entries()
        .iter()
        .filter(|e| e.key == edge.label)
    {
        spans.extend(
            reading
                .references(entry)
                .iter()
                .filter(|item| item.id == target)
                .filter_map(|item| item.span.cloned()),
        );
        if let FieldValue::Identifier(value) = &entry.value
            && value == target
        {
            spans.extend(entry.value_span.clone());
        }
    }
    spans
}

/// The tokens spelling `word` in `span`, outside strings and comments.
fn tokens_named(text: &SourceText, span: &SourceSpan, word: &str) -> Vec<SourceSpan> {
    text.tokens(span)
        .into_iter()
        .filter(|t| t.is_name() && text.token_text(t) == word)
        .map(|t| text.span(span.file, t.start, t.end))
        .collect()
}

/// The entity's name in its declaration: the first token spelling `id` on
/// the block's first line after the kind keyword.
fn declaration_name(text: &SourceText, block: &SourceSpan, id: &str) -> Option<SourceSpan> {
    let tokens: Vec<Lexeme> = text.tokens(block);
    let first_line = block.start_line;
    tokens
        .iter()
        .filter(|t| t.is_name())
        .skip(1)
        .take_while(|t| text.position(t.start).0 == first_line)
        .find(|t| text.token_text(t) == id)
        .map(|t| text.span(block.file, t.start, t.end))
}
