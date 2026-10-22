//! `specforge://prompts/context`: what implementing one entity needs.

use std::collections::BTreeMap;

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_graph::{AnnotationJson, FieldJson, Members, MethodJson};

use crate::args::{Arguments, EntityIds};
use crate::prompt::{CorePrompt, Prompted};
use crate::refusal::ArgumentOf;
use crate::tool::McpError;
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
    const OPEN_VALUES: &'static [&'static str] = &[
        "$.fields",
        "$.annotations",
        "$.structural_constraint_entities[].fields",
        "$.structural_constraint_entities[].annotations",
    ];
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
    /// Each annotated field's annotations, by its name: an open value.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, Vec<AnnotationJson>>,
    /// Its methods, as the graph export writes them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<MethodJson>,
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
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, Vec<AnnotationJson>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<MethodJson>,
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
    let facts = specforge_ops::read::inspect::inspect(&view.reporting(&[]), entity_id)
        .map_err(|refusal| Box::new(Prompt::refused(refusal)))?;
    let node = facts.node;

    // The statement the extension declares (headline and normative), e.g.
    // a behavior's `contract`; empty for a kind that declares none.
    let contract_text = facts
        .headline
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
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
        .map(specforge_ops::read::inspect::obligation_text)
        .collect();

    // Structural constraints: entities the caller wants in the context even
    // when no edge connects them to this one.
    let mut constraint_entities: Vec<ConstraintEntity> = Vec::new();
    for constraint_id in &args.structural_constraints.0 {
        let Some(constraint) = graph.node(constraint_id) else {
            // The one refusal of an entity the graph lacks (E003, did-you-mean), about the argument that named it.
            return Err(Box::new(
                crate::tool::entity_not_found(graph, constraint_id)
                    .with_argument("structural_constraints"),
            ));
        };
        let Members {
            fields,
            annotations,
            methods,
        } = Members::of(constraint);
        constraint_entities.push(ConstraintEntity {
            entity_id: constraint.id.raw.to_string(),
            kind: constraint.kind.raw.to_string(),
            title: constraint.title.clone().unwrap_or_default(),
            fields,
            annotations,
            methods,
        });
    }

    let payload = Payload {
        entity_id: entity_id.to_string(),
        kind: node.kind.raw.to_string(),
        contract_text: contract_text.to_string(),
        // Every field, whatever the kind names its text: an invariant's
        // `guarantee`, a decision's `rationale`.
        fields: facts.members.fields,
        annotations: facts.members.annotations,
        methods: facts.members.methods,
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
