//! `specforge.add_extension`: install extensions (`specforge_ops::extension::add`).

use crate::target::{AnyProject, Project};
use crate::tool::{CoreTool, Effect, RefusalArgument, ReplySpec, WriteHints};
use serde::Serialize;
use specforge_common::shape::Shape;

use crate::args::{Arguments, NonEmpty};
use crate::events::ExtensionAdded;
use crate::mutation::{
    Failed, Mutated, Mutation, MutationArgs, MutationEvent, MutationReply, NoDiagnostics, Written,
};
use crate::reply::Answer;
use crate::target::ProjectRef;
use specforge_ops::extension::AddDocument;

/// `specforge.add_extension`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Extension specifiers (@scope/name[@version], a builtin's name or a .wasm path), added as one change
    specifiers: NonEmpty,
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

/// `specforge.add_extension`'s reply: the add
/// document `specforge add --format json` prints (ADR 0052), and whether the
/// server serves the extensions (absent for a preview).
#[derive(Debug, Serialize, Shape)]
pub struct Reply {
    #[serde(flatten)]
    added: AddDocument,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<&'static str>,
}

/// What the server does with extensions once they are enabled in the
/// project it serves.
const SERVED: &str = "the server serves the extensions added from the next call on";
/// What it does with ones installed in another project the path names.
const ELSEWHERE: &str = "installed in the project the path names; the server keeps serving its own";

/// `specforge.add_extension`: the shared add, its reply, the files it
/// wrote and one `extension_added` per specifier (for an extension already
/// there, or a specifier repeating an earlier one, `wasDuplicate`).
pub(crate) struct Tool;

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.add_extension";
    const DESCRIPTION: &'static str = "Install extensions, as one change";
    const EFFECT: Effect = Effect::Mutates {
        hints: WriteHints {
            destructive: true,
            idempotent: true,
            open_world: true,
        },
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpAddExtensionResult");
    type Args = Args;
    type Input = Project<AnyProject>;
    type Returns = Mutation<Reply>;
    const REFUSAL_ARGUMENTS: &'static [RefusalArgument] = &[
        RefusalArgument {
            code: specforge_common::codes::E054.id(),
            argument: "specifiers",
        },
        RefusalArgument {
            code: specforge_common::codes::R_RES_003.id(),
            argument: "specifiers",
        },
    ];

    fn call(project: ProjectRef<'_>, args: Args) -> Mutation<Reply> {
        use specforge_ops::extension::{self, AddRequest, Trust};

        // The project the call installs into: the served one, or the one
        // `path` names.
        let root = project.root;
        // A relative local path is read from the project's root: the server has no working
        // directory of its own to mean it.
        let extensions = extension::specified(&args.specifiers.0)?
            .into_iter()
            .map(|specified| specified.relative_to(root.dir()))
            .collect();

        let registry =
            specforge_ops_registry::ConfiguredRegistry::for_project(root, "add_extension");
        // The shared operation `specforge add` runs. An agent can't be asked,
        // so a publisher key change is refused rather than re-pinned.
        let request = AddRequest {
            root,
            extensions,
            allow_unsigned: args.allow_unsigned,
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
        // Once they are enabled, the server serves them (the next request brings the
        // project up to date); another project only has them on disk.
        let reply = Reply {
            added: added.document(),
            note: (!args.dry_run).then_some(if project.is_served() {
                SERVED
            } else {
                ELSEWHERE
            }),
        };
        let answer = Answer::new(reply).with_diagnostics(reported);
        if args.dry_run {
            return Ok(Mutated::new(answer, Written::nothing()));
        }
        let written = added.added.iter().fold(
            Written::files(added.writes.clone()),
            |written, extension| {
                written.with_event(MutationEvent::ExtensionAdded(ExtensionAdded {
                    extension_specifier: extension.specifier.clone(),
                    total_extensions: added.extensions_enabled,
                    was_duplicate: extension.outcome.is_duplicate(),
                }))
            },
        );
        Ok(Mutated::new(answer, written))
    }
}
