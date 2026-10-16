//! `specforge.migrate`: run the migration pipeline (`specforge_ops::authoring::migrate`).

use crate::args::Arguments;
use crate::mutation::{
    Failed, Mutated, Mutation, MutationArgs, MutationReply, NoDiagnostics, Written,
};
use crate::refusal::ArgumentOf;
use crate::target::ProjectRef;
use crate::target::{AnyProject, Project};
use crate::tool::McpError;
use crate::tool::{CoreTool, Effect, ReplySpec, WriteHints};
use specforge_ops::OpError;
use specforge_ops::authoring::migrate::MigrateField;

/// `specforge.migrate`'s arguments.
#[derive(Debug, Arguments)]
pub struct Args {
    /// Return the diffs without changing any file
    dry_run: bool,
    /// Format version to migrate to, as MAJOR.MINOR (defaults to the current format version)
    target_version: Option<String>,
    /// Skip the .bak backup of each migrated file
    no_backup: bool,
}

impl MutationArgs for Args {
    fn previews(&self) -> bool {
        self.dry_run
    }
}

impl MutationReply for Reply {
    type After = NoDiagnostics;
}

/// `specforge.migrate`'s reply: the migration document
/// `specforge migrate --format json` prints, whether anything was pending
/// or not (a failed run carries it in its error's `data`; ADR 0052).
type Reply = specforge_ops::authoring::migrate::MigrateDocument;

pub(crate) struct Tool;

impl ArgumentOf<MigrateField> for Tool {
    fn argument(field: MigrateField) -> Option<&'static str> {
        match field {
            MigrateField::Target => Some("target_version"),
        }
    }
}

impl CoreTool for Tool {
    const NAME: &'static str = "specforge.migrate";
    const DESCRIPTION: &'static str = "Run migration pipeline";
    const EFFECT: Effect = Effect::Mutates {
        hints: WriteHints {
            destructive: true,
            idempotent: true,
            open_world: false,
        },
    };
    const SPEC: ReplySpec = ReplySpec::Type("McpMigrateResult");
    type Args = Args;
    type Input = Project<AnyProject>;
    type Returns = Mutation<Reply>;

    fn call(project: ProjectRef<'_>, args: Args) -> Mutation<Reply> {
        // The project the call migrates, and the runtime its hooks run in.
        let path = project.root;
        let dry_run = args.dry_run;
        let no_backup = args.no_backup;
        // The format version to migrate to, checked as `specforge migrate
        // --target-version` checks it.
        let target =
            specforge_ops::authoring::migrate::parse_target(args.target_version.as_deref())
                .map_err(Self::failed)?;

        let runtime = project.runtime;
        // The migration `specforge migrate` runs, hooks and rollback included.
        let request = specforge_ops::authoring::migrate::Request {
            root: path,
            target,
            dry_run,
            no_backup,
            containment: &specforge_ops::Containment::within(path.dir()),
        };
        let outcome = specforge_ops::authoring::migrate::run(&request, Some(runtime.clone()))?;
        // A failed run's report rides in `data`, and what it left written (its
        // backups after a rollback, the files migrated before a failure) is
        // reported.
        let reply = outcome.document();
        match outcome.failure() {
            None => Ok(Mutated::new(reply, Written::files(outcome.writes))),
            Some(failure) => Err(Failed::after(
                McpError::from(OpError::from(failure))
                    .with_data(serde_json::to_value(&reply).expect("a reply serializes")),
                outcome.writes,
            )),
        }
    }
}
