//! `specforge://prompts/context`: what implementing one entity needs.

use std::collections::BTreeMap;

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_graph::FieldJson;
use specforge_graph::field_form::write_fields;

use crate::args::{Arguments, EntityIds};
use crate::prompt::{CorePrompt, Prompted};
use crate::refusal::ArgumentOf;
use crate::tool::{ErrorCode, McpError};
use specforge_ops::view::EntityField;
use specforge_ops::view::ProjectView;

/// The context prompt (`McpContextPromptResult`).
pub struct Prompt;

impl ArgumentOf<EntityField> for Prompt {
    fn argument(field: EntityField) -> Option<&'static str> {
        match field {
            EntityField::EntityId => Some("entity_id"),
        }
    }
}

impl CorePrompt for Prompt {
    const NAME: &'static str = "specforge://prompts/context";
    const DESCRIPTION: &'static str = "Get structured context for implementing an entity";
    const SPEC: &'static str = "McpContextPromptResult";
    const OPEN_VALUES: &'static [&'static str] =
        &["$.fields", "$.structural_constraint_entities[].fields"];
    type Args = Args;
    type Payload = Payload;

    fn render(view: ProjectView<'_>, args: Args) -> Result<Prompted<Payload>, Box<McpError>> {
        render(view, args)
    }
}

/// The context prompt's payload: what implementing one entity needs.
#[derive(Debug, Serialize, Shape)]
pub struct Payload {
    pub entity_id: String,
    pub kind: String,
    /// The statement the extension declares (headline and normative);
    /// empty for a kind that declares none.
    pub contract_text: String,
    /// Every field the entity declares, by name: an open value.
    pub fields: BTreeMap<String, FieldJson>,
    pub upstream_entities: Vec<String>,
    pub downstream_entities: Vec<String>,
    pub verify_expectations: Vec<String>,
    /// The entity IDs the request named as structural constraints.
    pub structural_constraints: Vec<String>,
    /// Each of them, whether or not an edge connects it to `entity_id`.
    pub structural_constraint_entities: Vec<ConstraintEntity>,
}

/// An entity a request names as a structural constraint
/// (`McpContextConstraint`).
#[derive(Debug, Serialize, Shape)]
pub struct ConstraintEntity {
    pub entity_id: String,
    pub kind: String,
    pub title: String,
    /// Every field it declares, by name: an open value.
    pub fields: BTreeMap<String, FieldJson>,
}

/// `specforge://prompts/context`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Entity ID to get context for
    entity_id: String,
    /// Entity IDs to include as context even when not connected (array or comma-separated)
    structural_constraints: EntityIds,
}

fn render(view: ProjectView<'_>, args: Args) -> Result<Prompted<Payload>, Box<McpError>> {
    let graph = view.graph();
    let entity_id = args.entity_id.as_str();
    // The inspect read view, without the reported diagnostics (the prompt
    // shows none); it shows no coverage, so it reads no report.
    let facts = specforge_ops::inspect::inspect(&view.reporting(&[]), entity_id)
        .map_err(|refusal| Box::new(Prompt::refused(refusal)))?;
    let node = facts.node;

    // The statement the extension declares (headline and normative), e.g.
    // a behavior's `contract`; empty for a kind that declares none.
    let contract_text = facts.headline.unwrap_or_default();
    let upstream: Vec<String> = facts
        .references
        .incoming
        .iter()
        .map(|r| r.peer.to_string())
        .collect();
    let downstream: Vec<String> = facts
        .references
        .outgoing
        .iter()
        .map(|r| r.peer.to_string())
        .collect();
    let verify_expectations: Vec<String> = facts
        .obligations
        .iter()
        .map(specforge_ops::inspect::obligation_text)
        .collect();

    // Structural constraints: entities the caller wants in the context even
    // when no edge connects them to this one.
    let mut constraint_entities: Vec<ConstraintEntity> = Vec::new();
    for constraint_id in &args.structural_constraints.0 {
        let Some(constraint) = graph.node(constraint_id) else {
            return Err(Box::new(
                McpError::new(
                    ErrorCode::EntityNotFound,
                    format!("Structural constraint entity not found: {constraint_id}"),
                )
                .with_entity(constraint_id.as_str())
                .with_argument("structural_constraints"),
            ));
        };
        constraint_entities.push(ConstraintEntity {
            entity_id: constraint.id.raw.to_string(),
            kind: constraint.kind.raw.to_string(),
            title: constraint.title.clone().unwrap_or_default(),
            fields: write_fields(&constraint.fields),
        });
    }

    let payload = Payload {
        entity_id: entity_id.to_string(),
        kind: node.kind.raw.to_string(),
        contract_text: contract_text.to_string(),
        // Every field, whatever the kind names its text: an invariant's
        // `guarantee`, a decision's `rationale`.
        fields: write_fields(&node.fields),
        upstream_entities: upstream,
        downstream_entities: downstream,
        verify_expectations,
        structural_constraints: args.structural_constraints.0.clone(),
        structural_constraint_entities: constraint_entities,
    };

    let instruction = format!(
        "You are implementing the entity '{}' (kind: {}). \
         Use the structured context below to guide your implementation. \
         Respect the contract (or the guarantee, rationale or other text in its fields), satisfy verify expectations, and consider upstream/downstream dependencies.",
        entity_id, node.kind.raw
    );

    Ok(Prompted {
        instruction,
        payload,
    })
}
