//! `specforge.rename`: rename an entity across the project (`specforge_ops::rename`).

use serde::Serialize;
use specforge_common::DiagnosticList;
use specforge_common::shape::Shape;

use crate::args::Arguments;
use crate::mutation::{Mutated, Mutation, MutationArgs, Written};
use crate::target::ProjectRef;
use crate::tool::McpError;
use specforge_ops::OpErrorKind;

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

/// `specforge.rename`'s reply (`McpRenameResult`).
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    old_name: String,
    new_name: String,
    affected_files: Vec<String>,
    edits: Vec<Edit>,
    /// Present (true) for a preview.
    #[serde(skip_serializing_if = "Option::is_none")]
    dry_run: Option<bool>,
    /// What `specforge check` reports for the project once the edits are
    /// made; filled in when the target is brought up to date.
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostics: Option<DiagnosticList>,
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

pub(crate) fn call(project: &ProjectRef<'_>, args: Args) -> Mutation<Reply> {
    use specforge_ops::rename;
    let dry_run = args.dry_run;

    // Planned on the call's project as it is on disk (the target brought
    // the served project up to date, or compiled the project `path`
    // names), whose spans are relative to its spec root.
    let spec_root = project.spec_root().to_path_buf();
    let planned = rename::plan(
        &crate::tools::navigator(project.view()),
        &args.entity_id,
        &args.new_name,
    );
    let plan = match planned {
        Ok(plan) => plan,
        // The operation decided what kind of failure it is, and which
        // entity it is about; an invalid new ID is the argument's fault.
        Err(e) => {
            let argument = (e.kind == OpErrorKind::InvalidInput).then_some("new_name");
            let error = McpError::from(e);
            return Ok(Mutated::refused_unless_preview(
                dry_run,
                match argument {
                    Some(argument) => error.with_argument(argument),
                    None => error,
                },
            ));
        }
    };

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
        diagnostics: None,
    };
    if dry_run {
        reply.dry_run = Some(true);
        return Ok(Mutated::preview(reply));
    }
    // A failed write restores what it wrote: nothing is left written.
    let writes = match rename::apply(&plan, &spec_root) {
        Ok(writes) => writes,
        Err(e) => return Ok(Mutated::refused_unless_preview(false, McpError::from(e))),
    };
    // The reply's `diagnostics` are what `specforge check` reports for the
    // project as it is on disk now, edits made since the last call
    // included (filled in once the target is brought up to date).
    Ok(Mutated::wrote(
        reply,
        Written::files(writes)
            .with_entities([args.new_name])
            .with_fresh_diagnostics(),
    ))
}
