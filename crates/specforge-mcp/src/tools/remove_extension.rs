//! `specforge.remove_extension`: remove an extension (`specforge_ops::extension::remove`).

use serde::Serialize;
use specforge_common::shape::Shape;

use crate::args::Arguments;
use crate::mutation::{Mutated, Mutation, MutationArgs, Written};
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
    let name = args.name.clone();
    let force = args.force;
    let dry_run = args.dry_run;

    // The shared operation, over the view of the call's project: its
    // dependents and its stranded entities, the served project's or those
    // of the project `path` names.
    let request = specforge_ops::extension::RemoveRequest {
        name: &name,
        force,
        dry_run,
    };
    match specforge_ops::extension::remove(&project.view(), &request) {
        Ok(outcome) => {
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
            if outcome.dry_run {
                return Ok(Mutated::preview(reply));
            }
            Ok(Mutated::wrote(
                reply,
                Written::files(outcome.writes)
                    .with_entities(outcome.stranded.into_iter().map(|entity| entity.entity_id)),
            ))
        }
        // A removal that failed after editing specforge.json reports it.
        Err(error) => Ok(Mutated::refused_after(dry_run, error)),
    }
}
