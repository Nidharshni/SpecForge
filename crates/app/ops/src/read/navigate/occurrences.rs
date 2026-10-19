//! Where entity IDs are written: an entity's declaration name, and each
//! reference (one edge of the graph) as the identifier token its field
//! holds.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, Node};

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

impl Navigator<'_> {
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

    /// The occurrences of `id` once per written token, sorted by (file, line, column): what a rename edits and an
    /// editor shows. An inherited reference is one token that [`Self::references`] answers once per block that
    /// inherits it (ADR 0149 D11); here the first of those blocks, in sort order, holds it.
    pub fn tokens(
        &self,
        id: &str,
        query: ReferenceQuery,
    ) -> Result<Vec<Occurrence>, Refusal<EntityField>> {
        let mut occurrences = self.references(id, query)?;
        // `Occurrence::key` starts with the span, so the occurrences of one token are adjacent.
        occurrences.dedup_by(|a, b| a.span == b.span);
        Ok(occurrences)
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
        // Every node of the file: a token in a defaults block lies outside every block and answers for the first
        // block that inherits it (ADR 0149).
        for holder in graph.nodes_in_file(file) {
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

    /// The occurrences of `edge`'s target in its source: each place the graph records it is written
    /// (`Graph::written`: a reference list item, a single reference's value, a name in a type expression a derived
    /// field reads), when the text spells the target there. Without any, or with one the text does not spell, the
    /// holder's block with [`Precision::Entity`]. `holder` is the edge's source.
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

        let mut occurrences = Vec::new();
        let mut stale = false;
        for span in self.view.graph().written(edge) {
            if text.spells(span, target) {
                occurrences.push(occurrence(span.clone(), Precision::Token));
            } else {
                stale = true;
            }
        }
        if stale || occurrences.is_empty() {
            occurrences.extend(entity());
        }
        occurrences
    }
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
