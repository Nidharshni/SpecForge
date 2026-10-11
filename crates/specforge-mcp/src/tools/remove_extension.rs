//! `specforge.remove_extension`: remove an extension (`specforge_ops::extension::remove`).

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

/// `specforge.remove_extension`'s reply (`McpRemoveExtensionResult`): the
/// remove document `specforge remove --format json` prints (ADR 0052).
pub use specforge_ops::extension::RemoveDocument as Reply;

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
    Ok(Mutated::new(
        outcome.document(),
        Written::files(outcome.writes)
            .with_entities(outcome.stranded.into_iter().map(|entity| entity.entity_id)),
    ))
}
