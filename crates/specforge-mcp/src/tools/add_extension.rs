//! `specforge.add_extension`: install an extension (`specforge_ops::extension::add`).

use serde::Serialize;
use specforge_common::shape::Shape;

use crate::args::Arguments;
use crate::mutation::{
    Failed, Mutated, Mutation, MutationArgs, MutationEvent, MutationReply, NoDiagnostics, Written,
};
use crate::reply::Answer;
use crate::target::ProjectRef;
use specforge_ops::extension::AddDocument;

/// `specforge.add_extension`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Extension specifier
    specifier: String,
    /// Preview the install without changing any file
    dry_run: bool,
    /// Accept a registry package with no publisher signature (publisher verification skipped)
    allow_unsigned: bool,
}

impl MutationArgs for Args {
    fn previews(&self) -> bool {
        self.dry_run
    }
}

impl MutationReply for Reply {
    type After = NoDiagnostics;
}

/// `specforge.add_extension`'s reply (`McpAddExtensionResult`): the add
/// document `specforge add --format json` prints (ADR 0052), and whether the
/// server serves the extension (absent for a preview).
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    #[serde(flatten)]
    added: AddDocument,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<&'static str>,
}

/// What the server does with an extension once it is enabled in the
/// project it serves.
const SERVED: &str = "the server serves it from the next call on";
/// What it does with one installed in another project the path names.
const ELSEWHERE: &str = "installed in the project the path names; the server keeps serving its own";

/// `specforge.add_extension`: the shared add, its reply, the files it
/// wrote and `extension_added` (for an extension already there too,
/// `wasDuplicate`).
pub(crate) fn call(project: &ProjectRef<'_>, args: Args) -> Mutation<Reply> {
    use specforge_ops::extension::{self, AddOutcome, AddRequest, Trust};

    let allow_unsigned = args.allow_unsigned;

    // The project the call installs into: the served one, or the one
    // `path` names.
    let root = project.root.to_path_buf();
    let source = extension::parse(&args.specifier)?;

    let registry = specforge_ops_registry::ConfiguredRegistry::for_project(&root, "add_extension");
    // The shared operation `specforge add` runs. An agent can't be asked,
    // so a publisher key change is refused rather than re-pinned.
    let request = AddRequest {
        root: &root,
        source,
        allow_unsigned,
        trust: Trust::Refuse,
        dry_run: args.dry_run,
    };
    let added = extension::add(&request, &registry, project.runtime.as_ref());
    // What reading the registry configuration reported (E067, W140, I003), once the add asked a
    // registry, as `specforge add` shows it.
    let reported = registry.reported();
    // An install that failed after placing its module reports it.
    let added = match added {
        Ok(added) => added,
        Err(error) => return Err(Failed::from(error).with_diagnostics(reported)),
    };
    let preview = args.dry_run || matches!(added.outcome, AddOutcome::Planned { .. });
    // Once it is enabled, the server serves it (the next request brings the
    // project up to date); another project only has it on disk.
    let reply = Reply {
        added: added.document(),
        note: (!preview).then_some(if project.is_served() {
            SERVED
        } else {
            ELSEWHERE
        }),
    };
    let answer = Answer::new(reply).with_diagnostics(reported);
    if preview {
        return Ok(Mutated::new(answer, Written::nothing()));
    }
    let event = MutationEvent::ExtensionAdded {
        specifier: args.specifier,
        total_extensions: added.extensions_enabled,
        was_duplicate: added.outcome.is_duplicate(),
    };
    Ok(Mutated::new(
        answer,
        Written::files(added.writes).with_event(event),
    ))
}
