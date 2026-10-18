//! `specforge.edit`: typed edits of the project's entities (`specforge_ops::authoring::edit`), applied all or nothing.

use crate::target::{AnyProject, Project};
use crate::tool::{CoreTool, Effect, ReplySpec, WriteHints};
use serde::Serialize;
use specforge_common::DiagnosticList;
use specforge_common::shape::Shape;

use crate::args::{Arguments, Edits};
use crate::mutation::{FreshDiagnostics, Mutated, Mutation, MutationArgs, MutationReply, Written};
use crate::refusal::ArgumentOf;
use crate::target::ProjectRef;
use specforge_ops::authoring::edit::{self, EditField};

/// `specforge.edit`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// The edits, in order: each names its `op` (create, set, unset, retitle, link, unlink, add_verify,
    /// remove_verify or delete) and what it changes
    edits: Edits,
    /// Return the plan (files and diff) without changing any file
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

/// `specforge.edit`'s reply.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    entities: Vec<String>,
    affected_files: Vec<String>,
    diff: String,
    added: DiagnosticList,
    /// Present (true) for a preview.
    #[serde(skip_serializing_if = "Option::is_none")]
    dry_run: Option<bool>,
}

pub(crate) struct Tool;

impl ArgumentOf<EditField> for Tool {
    /// Every refusal of a list of edits is about the list.
    fn argument(_: EditField) -> Option<&'static str> {
        Some("edits")
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.edit";
    const DESCRIPTION: &'static str = "Create, change or delete entities: a list of typed edits, validated and written all or nothing";
    const EFFECT: Effect = Effect::Mutates {
        hints: WriteHints {
            destructive: true,
            idempotent: false,
            open_world: false,
        },
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpEditResult");
    type Args = Args;
    type Input = Project<AnyProject>;
    type Returns = Mutation<Reply>;

    fn call(project: ProjectRef<'_>, args: Args) -> Mutation<Reply> {
        // Planned on the call's project as it is on disk; an edit list that leaves an error the project did not
        // have is refused (E080) and nothing is written.
        let plan = edit::plan(&project.view(), &args.edits.0).map_err(Self::failed)?;
        let report = plan.report();
        let mut reply = Reply {
            entities: report.entities.clone(),
            affected_files: report.affected_files.clone(),
            diff: report.diff.clone(),
            added: report.added.clone(),
            dry_run: None,
        };
        if args.dry_run {
            reply.dry_run = Some(true);
            return Ok(Mutated::new(reply, Written::nothing()));
        }
        let writes = edit::apply(&plan, project.caller())?;
        Ok(Mutated::new(
            reply,
            Written::files(writes).with_entities(plan.report().entities.iter().cloned()),
        ))
    }
}
