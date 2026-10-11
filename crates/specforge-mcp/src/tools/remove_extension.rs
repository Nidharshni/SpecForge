//! `specforge.remove_extension`: remove an extension (`specforge_ops::extension::remove`).

use serde::Serialize;
use specforge_common::shape::Shape;

use crate::args::Arguments;
use crate::mutation::{Mutated, Mutation, MutationArgs, MutationReply, NoDiagnostics, Written};
use crate::target::ProjectRef;

/// `specforge.remove_extension`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Extension name
    name: String,
    /// Force removal
    force: bool,
    /// Preview the removal, stranded entities included, without changing any file
    dry_run: bool,
}

impl MutationArgs for Args {
    fn previews(&self) -> bool {
        self.dry_run
    }
}

impl MutationReply for Reply {
    type After = NoDiagnostics;
}

/// `specforge.remove_extension`'s reply (`McpRemoveExtensionResult`).
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    removed_extension: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    /// The entities of the removed extension's kinds, which the project
    /// still holds.
    stranded: Vec<Stranded>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dry_run: Option<bool>,
}

/// One entity the removal strands.
#[derive(Debug, Serialize, Shape)]
pub struct Stranded {
    entity_id: String,
    kind: String,
}

pub(crate) fn call(project: &ProjectRef<'_>, args: Args) -> Mutation<Reply> {
    // The shared operation, over the view of the call's project: its
    // dependents and its stranded entities, the served project's or those
    // of the project `path` names. A removal that failed after editing
    // specforge.json reports it.
    let request = specforge_ops::extension::RemoveRequest {
        name: &args.name,
        force: args.force,
        dry_run: args.dry_run,
    };
    let outcome = specforge_ops::extension::remove(&project.view(), &request)?;
    let reply = Reply {
        removed_extension: outcome.name.to_string(),
        version: outcome.version.as_ref().map(ToString::to_string),
        stranded: outcome
            .stranded
            .iter()
            .map(|entity| Stranded {
                entity_id: entity.entity_id.to_string(),
                kind: entity.kind.to_string(),
            })
            .collect(),
        dry_run: outcome.dry_run.then_some(true),
    };
    Ok(Mutated::new(
        reply,
        Written::files(outcome.writes)
            .with_entities(outcome.stranded.into_iter().map(|entity| entity.entity_id)),
    ))
}
