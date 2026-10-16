//! `specforge.remove_extension`: remove an extension (`specforge_ops::extension::remove`).

use crate::args::Arguments;
use crate::mutation::{Mutated, Mutation, MutationArgs, MutationReply, NoDiagnostics, Written};
use crate::refusal::ArgumentOf;
use crate::target::ProjectRef;
use crate::target::{AnyProject, Project};
use crate::tool::{CoreTool, Effect, ReplySpec, WriteHints};
use specforge_ops::extension::RemoveField;

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

/// `specforge.remove_extension`'s reply: the
/// remove document `specforge remove --format json` prints (ADR 0052).
type Reply = specforge_ops::extension::RemoveDocument;

pub(crate) struct Tool;

impl ArgumentOf<RemoveField> for Tool {
    fn argument(field: RemoveField) -> Option<&'static str> {
        match field {
            RemoveField::Name => Some("name"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.remove_extension";
    const DESCRIPTION: &'static str = "Remove an installed extension";
    const EFFECT: Effect = Effect::Mutates {
        hints: WriteHints {
            destructive: true,
            idempotent: true,
            open_world: false,
        },
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpRemoveExtensionResult");
    type Args = Args;
    type Input = Project<AnyProject>;
    type Returns = Mutation<Reply>;

    fn call(project: ProjectRef<'_>, args: Args) -> Mutation<Reply> {
        // The shared operation, over the view of the call's project: its
        // dependents and its stranded entities, the served project's or those
        // of the project `path` names. A removal that failed after editing
        // specforge.json reports it.
        let request = specforge_ops::extension::RemoveRequest {
            name: &args.name,
            force: args.force,
            dry_run: args.dry_run,
            caller: project.caller(),
        };
        let outcome =
            specforge_ops::extension::remove(&project.view(), &request).map_err(Self::failed)?;
        Ok(Mutated::new(
            outcome.document(),
            Written::files(outcome.writes)
                .with_entities(outcome.stranded.into_iter().map(|entity| entity.entity_id)),
        ))
    }
}
