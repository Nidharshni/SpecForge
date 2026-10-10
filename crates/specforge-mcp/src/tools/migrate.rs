//! `specforge.migrate`: run the migration pipeline (`specforge_ops::migrate`).

use serde::Serialize;
use specforge_common::DiagnosticList;
use specforge_common::shape::Shape;
use specforge_ops::migrate::{MigrationDiff, MigrationResult, RollbackSummary};

use crate::args::Arguments;
use crate::mutation::{Failed, Mutated, Mutation, MutationArgs, Written};
use crate::target::ProjectRef;
use crate::tool::McpError;

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

/// `specforge.migrate`'s reply (`McpMigrateResult`): a project already at
/// the latest format version, or the run's report (a failed run carries it
/// in its error's `data`).
#[derive(Debug, Serialize, Shape)]
#[serde(untagged)]
pub enum Reply {
    Current(Current),
    Ran(Box<Ran>),
}

/// Nothing to migrate: no file is behind the target version.
#[derive(Debug, Serialize, Shape)]
pub struct Current {
    ok: bool,
    from_version: String,
    to_version: String,
    migrated: bool,
    dry_run: bool,
    message: String,
}

/// A migration that ran (or, with `dry_run`, was previewed).
#[derive(Debug, Serialize, Shape)]
pub struct Ran {
    ok: bool,
    from_version: String,
    to_version: String,
    migrated: bool,
    dry_run: bool,
    files_migrated: usize,
    files_skipped: usize,
    files_failed: usize,
    results: Vec<MigrationResult>,
    diffs: Vec<MigrationDiff>,
    /// `extension:hook` for each migration hook that ran.
    hooks_invoked: Vec<String>,
    /// Why each failing hook failed.
    hook_failures: Vec<String>,
    /// Breaking Graph Protocol schema changes (W053).
    schema_warnings: DiagnosticList,
    /// How the migrated graph differs from the one before (W054).
    structural_differences: DiagnosticList,
    rolled_back: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    rollback: Option<RollbackSummary>,
    post_migration_validated: bool,
    /// What compiling the migrated project reported as errors.
    post_migration_errors: DiagnosticList,
}

pub(crate) fn call(project: &ProjectRef<'_>, args: Args) -> Mutation<Reply> {
    // The project the call migrates, and the runtime its hooks run in.
    let path = project.root;
    let dry_run = args.dry_run;
    let no_backup = args.no_backup;
    // The format version to migrate to, checked as `specforge migrate
    // --target-version` checks it.
    let target = specforge_ops::migrate::parse_target(args.target_version.as_deref())?;

    let runtime = project.runtime;
    // The migration `specforge migrate` runs, hooks and rollback included.
    let request = specforge_ops::migrate::Request {
        root: path,
        target,
        dry_run,
        no_backup,
    };
    let outcome = specforge_ops::migrate::run(&request, Some(runtime.clone()));
    let (from, to) = (outcome.from.to_string(), outcome.to.to_string());
    // The format version lives in each spec file's header: with no file
    // behind the target, the project is current and nothing ran: a
    // migration that wrote nothing.
    if !outcome.pending {
        let current = Reply::Current(Current {
            ok: outcome.ok(),
            from_version: from,
            to_version: to,
            migrated: false,
            dry_run,
            message: "project is already at the latest format version".to_string(),
        });
        return Ok(Mutated::new(current, Written::files(outcome.writes)));
    }

    let summary = &outcome.summary;
    let reply = Reply::Ran(Box::new(Ran {
        ok: outcome.ok(),
        from_version: from,
        to_version: to,
        migrated: outcome.migrated(),
        dry_run,
        files_migrated: summary.migrated_count,
        files_skipped: summary.skipped_count,
        files_failed: summary.failed_count,
        results: summary.results.clone(),
        diffs: summary.diffs.clone(),
        hooks_invoked: outcome.hooks_invoked.clone(),
        hook_failures: outcome.hook_failures.clone(),
        schema_warnings: DiagnosticList(outcome.schema_warnings.clone()),
        structural_differences: DiagnosticList(outcome.structural_differences.clone()),
        rolled_back: outcome.rollback.is_some(),
        rollback: outcome.rollback.clone(),
        post_migration_validated: outcome.validated,
        post_migration_errors: DiagnosticList(outcome.post_errors().cloned().collect()),
    }));
    // A failed run's report rides in `data`, and what it left written (its
    // backups after a rollback, the files migrated before a failure) is
    // reported.
    match outcome.failure() {
        None => Ok(Mutated::new(reply, Written::files(outcome.writes))),
        Some(failure) => Err(Failed::after(
            McpError::from(failure)
                .with_data(serde_json::to_value(&reply).expect("a reply serializes")),
            outcome.writes,
        )),
    }
}
