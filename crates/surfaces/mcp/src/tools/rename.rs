//! `specforge.rename`: rename an entity across the project (`specforge_ops::authoring::rename`).

use crate::target::{AnyProject, Project};
use crate::tool::{CoreTool, Effect, ReplySpec, WriteHints};
use serde::Serialize;
use specforge_common::shape::Shape;

use crate::args::Arguments;
use crate::mutation::{FreshDiagnostics, Mutated, Mutation, MutationArgs, MutationReply, Written};
use crate::refusal::ArgumentOf;
use crate::target::ProjectRef;
use specforge_ops::authoring::rename::{RenameDocument, RenameField};
use specforge_ops::read::navigate::Navigator;

/// `specforge.rename`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Current entity ID
    entity_id: String,
    /// New entity ID
    new_name: String,
    /// Return the rename plan without changing any file
    dry_run: bool,
}

impl MutationArgs for Args {
    fn previews(&self) -> bool {
        self.dry_run
    }
}

impl MutationReply for Reply {
    type After = FreshDiagnostics;
}

/// `specforge.rename`'s reply (`McpRenameResult`): the rename's document, and its edits as the spec's `TextEdit`
/// (ADR 0164 D3).
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    #[serde(flatten)]
    renamed: RenameDocument,
    edits: Vec<crate::reply::TextEdit>,
}

pub(crate) struct Tool;

impl ArgumentOf<RenameField> for Tool {
    fn argument(field: RenameField) -> Option<&'static str> {
        match field {
            RenameField::OldId => Some("entity_id"),
            RenameField::NewId => Some("new_name"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.rename";
    const DESCRIPTION: &'static str = "Rename an entity across all files";
    const EFFECT: Effect = Effect::Mutates {
        hints: WriteHints {
            destructive: true,
            idempotent: true,
            open_world: false,
        },
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpRenameResult");
    type Args = Args;
    type Input = Project<AnyProject>;
    type Returns = Mutation<Reply>;

    fn call(project: ProjectRef<'_>, args: Args) -> Mutation<Reply> {
        use specforge_ops::authoring::rename;

        // Planned on the call's project as it is on disk (the target brought
        // the served project up to date, or compiled the project `path`
        // names), whose spans are relative to its spec root, which the plan keeps.
        //
        // The operation decided what kind of failure it is, and which entity it
        // is about; an invalid new ID is the argument's fault.
        //
        // A failed write puts back what it wrote; a file it could not put back is the failure's `files_written`.
        let renamed = rename::run(
            &Navigator::new(project.view()),
            &args.entity_id,
            &args.new_name,
            args.dry_run,
            project.caller(),
        )
        .map_err(Self::failed)?;
        let reply = Reply {
            renamed: renamed.document(),
            edits: renamed.plan.edits().iter().map(Into::into).collect(),
        };
        // The reply's `diagnostics` are what `specforge check` reports for the
        // project as it is on disk now, edits made since the last call
        // included (`FreshDiagnostics`, filled in once the target is brought up
        // to date).
        Ok(Mutated::new(
            reply,
            Written::of(renamed.wrote).with_entities([renamed.plan.new_id]),
        ))
    }
}
