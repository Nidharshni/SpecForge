//! Where entity IDs are written: an entity's declaration name, and each
//! reference (one edge of the graph) as the identifier token its field
//! holds.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Node, SpecFile, WrittenToken, written_tokens};

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

/// One place an entity's ID is written: what a rename edits and an editor shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The entity it names.
    pub target: Sym,
    pub span: SourceSpan,
    /// [`Precision::Entity`] when the compiled text does not spell the ID there (a stale or unread text).
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

    /// The occurrences of `id`, sorted by (file, line, column): each token the graph's finder reads, once per block
    /// that holds it (`held_by`: an inherited reference once per block that takes it; a default no block takes not at
    /// all), and the declaration when asked. A token the compiled text does not spell is its holder's block with
    /// [`Precision::Entity`].
    ///
    /// shortcut: this walks every compiled file per call; add a per-project token index if a project of ~20k
    /// entities makes find_references slow.
    pub fn references(
        &self,
        id: &str,
        query: ReferenceQuery,
    ) -> Result<Vec<Occurrence>, Refusal<EntityField>> {
        let node = self
            .node(id)
            .map_err(|error| error.about(EntityField::EntityId))?;
        let incoming = matches!(query.direction, Direction::Incoming | Direction::Both);
        let outgoing = matches!(query.direction, Direction::Outgoing | Direction::Both);
        let graph = self.view.graph();
        let mut occurrences = Vec::new();
        for (_, file) in self.view.project().parsed() {
            for token in self.found(file) {
                let shown = self.token(&token);
                for &holder in &token.held_by {
                    let wanted = (incoming && token.target.as_str() == id)
                        || (outgoing && holder.as_str() == id);
                    let Some(block) = graph.node(holder.as_str()).filter(|_| wanted) else {
                        continue;
                    };
                    occurrences.push(Occurrence {
                        holder,
                        target: token.target,
                        field: Some(token.field),
                        role: Role::Reference,
                        span: match shown.precision {
                            Precision::Token => shown.span.clone(),
                            Precision::Entity => block.source_span.clone(),
                        },
                        precision: shown.precision,
                    });
                }
            }
        }
        if query.include_declaration {
            occurrences.push(self.declaration(node));
        }
        Ok(sorted(occurrences))
    }

    /// Every written token that names `id`, once each, sorted by (file, line, column): each reference the graph's
    /// finder reads in the compiled parses (`specforge_graph::written_tokens`, ADR 0163), a `defaults` entry no block
    /// takes included, and its declaration's name when `include_declaration`. What a rename edits and an editor
    /// shows; who holds each reference is [`Self::references`]' answer.
    pub fn tokens(
        &self,
        id: &str,
        include_declaration: bool,
    ) -> Result<Vec<Token>, Refusal<EntityField>> {
        let node = self
            .node(id)
            .map_err(|error| error.about(EntityField::EntityId))?;
        let mut tokens: Vec<Token> = self
            .view
            .project()
            .parsed()
            .flat_map(|(_, file)| self.found(file))
            .filter(|t| t.target.as_str() == id)
            .map(|t| self.token(&t))
            .collect();
        if include_declaration {
            let definition = self.definition_of(node);
            tokens.push(Token {
                target: node.id.raw,
                span: definition.name,
                precision: definition.precision,
            });
        }
        tokens.sort_by_key(|t| {
            (
                t.span.file,
                t.span.start_line,
                t.span.start_col,
                t.span.end_line,
                t.span.end_col,
            )
        });
        tokens.dedup();
        Ok(tokens)
    }

    /// The tokens the graph's finder reads in `file` (a compiled parse) that name an entity of the graph.
    fn found(&self, file: &SpecFile) -> Vec<WrittenToken> {
        let graph = self.view.graph();
        let inputs = self.view.registries().graph_inputs();
        let kind_of = |name: &str| graph.node(name).map(|n| n.kind.raw);
        let mut tokens = written_tokens(file, inputs, kind_of);
        tokens.retain(|t| graph.node(t.target.as_str()).is_some());
        tokens
    }

    /// `token`'s span as a [`Token`]: [`Precision::Entity`] when the compiled text does not spell its target there.
    fn token(&self, token: &WrittenToken) -> Token {
        let spelled = self
            .text(token.span.file)
            .is_some_and(|text| text.spells(&token.span, token.target.as_str()));
        Token {
            target: token.target,
            span: token.span.clone(),
            precision: if spelled {
                Precision::Token
            } else {
                Precision::Entity
            },
        }
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

    /// The token (declaration or reference) whose span holds the byte position (1-based `line` and `col` of `file`;
    /// the position just past a token's end counts): each declaration of a node of the file, then each token the
    /// graph's finder reads in the file's compiled parse (a default no block takes and a duplicate declaration's
    /// references included). What prepareRename and a precise "go to" read. Only tokens as written answer.
    pub fn occurrence_at(&self, file: &str, line: usize, col: usize) -> Option<Token> {
        let at = SourceSpan {
            file: Sym::new(file),
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: col,
        };
        let declarations = self
            .view
            .graph()
            .nodes_in_file(file)
            .into_iter()
            .map(|node| {
                let definition = self.definition_of(node);
                Token {
                    target: node.id.raw,
                    span: definition.name,
                    precision: definition.precision,
                }
            });
        let parsed = self
            .view
            .project()
            .parsed()
            .find(|(key, _)| *key == file)
            .map(|(_, parse)| parse);
        let references = parsed
            .into_iter()
            .flat_map(|parse| self.found(parse))
            .map(|t| self.token(&t));
        declarations
            .chain(references)
            .find(|t| t.precision == Precision::Token && contains(&t.span, &at))
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
