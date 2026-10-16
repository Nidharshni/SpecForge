//! `specforge.rename`: rename an entity across the project (`specforge_ops::authoring::rename`).

use crate::target::{AnyProject, Project};
use crate::tool::{CoreTool, Effect, ReplySpec, WriteHints};
use serde::Serialize;
use specforge_common::shape::Shape;

use crate::args::Arguments;
use crate::mutation::{FreshDiagnostics, Mutated, Mutation, MutationArgs, MutationReply, Written};
use crate::refusal::ArgumentOf;
use crate::target::ProjectRef;
use specforge_ops::authoring::rename::RenameField;

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

/// `specforge.rename`'s reply.
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    old_name: String,
    new_name: String,
    affected_files: Vec<String>,
    edits: Vec<Edit>,
    /// Present (true) for a preview.
    #[serde(skip_serializing_if = "Option::is_none")]
    dry_run: Option<bool>,
}

/// One text edit (`McpRenameEdit`).
#[derive(Debug, Serialize, Shape)]
pub struct Edit {
    file: String,
    line: usize,
    start_col: usize,
    end_col: usize,
    new_text: String,
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
        // names), whose spans are relative to its spec root.
        let spec_root = project.spec_root().to_path_buf();
        //
        // The operation decided what kind of failure it is, and which entity it
        // is about; an invalid new ID is the argument's fault.
        let plan = rename::plan(
            &crate::tools::navigator(project.view()),
            &args.entity_id,
            &args.new_name,
        )
        .map_err(Self::failed)?;

        let mut reply = Reply {
            old_name: args.entity_id,
            new_name: args.new_name.clone(),
            affected_files: plan
                .affected_files()
                .into_iter()
                .map(String::from)
                .collect(),
            edits: plan
                .edits
                .iter()
                .map(|e| Edit {
                    file: e.file.to_string(),
                    line: e.line,
                    start_col: e.start_col,
                    end_col: e.end_col,
                    new_text: e.new_text.clone(),
                })
                .collect(),
            dry_run: None,
        };
        if args.dry_run {
            reply.dry_run = Some(true);
            return Ok(Mutated::new(reply, Written::nothing()));
        }
        // A failed write restores what it wrote: nothing is left written.
        let writes = rename::apply(&plan, &spec_root, project.containment())?;
        // The reply's `diagnostics` are what `specforge check` reports for the
        // project as it is on disk now, edits made since the last call
        // included (`FreshDiagnostics`, filled in once the target is brought up
        // to date).
        Ok(Mutated::new(
            reply,
            Written::files(writes).with_entities([args.new_name]),
        ))
    }
}
