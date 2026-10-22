//! `specforge migrate` and the MCP `specforge.migrate` tool: one migration: the format version header and the
//! migration steps (`titles`, `defaults`).
//!
//! The run loads the environment first and plans with its field forms (the header, then the `titles` and `defaults`
//! steps): a project with nothing to migrate, or with a file that cannot be migrated, is left alone (no compile, no
//! hooks, nothing written). Otherwise it compiles the project in that environment and judges the plan in memory
//! ([`judge`], ADR 0186): the extensions' migration hooks run, and the planned texts are compiled over the project
//! and compared with it. A dry run, or a migration the judgement refuses, ends there and writes nothing; a kept one is
//! written as one project change (each source and the record staged, each backup a name the migration owns, ADR 0171).

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, DiagnosticList, Severity, codes};
use specforge_graph::{GraphDelta, compute_graph_delta};
/// The migration crate's report types, as this operation's interface.
pub use specforge_migrate::{
    MigrationBackup, MigrationDiff, MigrationRecord, MigrationResult, MigrationStatus,
    MigrationSummary, RecordChange, RollbackSummary,
};
use specforge_migrate::{
    backup_of, beyond_steps, migrate_project, run_rollback, structural_differences,
};
use specforge_parser::{
    CURRENT_FORMAT_VERSION, FormatVersion, MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION,
};
use specforge_project::{CompiledProject, Environment, SharedRuntime};
use specforge_registry::{Disk as _, RegistryBuild};
use specforge_wasm::WasmRuntime;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::authoring::edit::validate::added;
use crate::change::{ProjectChange, unreadable, unwritable};
use crate::{Caller, Held, OpError, OpErrorKind, ReadFailure, Refusal, Writes, Wrote};

/// The field of a migration request a refusal can be about (ADR 0072).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateField {
    /// The format version to migrate to.
    Target,
}

/// The format version to migrate to: `raw`, checked, else the current one.
/// A version that doesn't parse, or one newer than this build supports, is
/// E019, about the target.
pub fn parse_target(raw: Option<&str>) -> Result<FormatVersion, Refusal<MigrateField>> {
    let Some(raw) = raw else {
        return Ok(CURRENT_FORMAT_VERSION);
    };
    let supported = format!(
        "Use a format version between {MIN_SUPPORTED_VERSION} and {MAX_SUPPORTED_VERSION}."
    );
    match raw.parse::<FormatVersion>() {
        Ok(version) if version > MAX_SUPPORTED_VERSION => Err(OpError::coded(
            crate::OpErrorKind::InvalidInput,
            codes::E019,
            format!("unsupported target version {raw} (max supported: {MAX_SUPPORTED_VERSION})"),
        )
        .with_suggestion(supported)
        .about(MigrateField::Target)),
        Ok(version) => Ok(version),
        Err(e) => Err(OpError::coded(
            crate::OpErrorKind::InvalidInput,
            codes::E019,
            format!("invalid target version '{raw}': {e}"),
        )
        .with_suggestion(supported)
        .about(MigrateField::Target)),
    }
}

pub struct Request<'a> {
    pub root: &'a specforge_common::ProjectRoot,
    pub target: FormatVersion,
    /// Report what would change; write nothing.
    pub dry_run: bool,
    /// Skip the `.bak` copy of each migrated file (nothing to roll back to).
    pub no_backup: bool,
    /// What the migration may reach (`caller.containment()`, ADR 0104): a source, a backup or the record that
    /// lands outside it, by a symlink in the project, refuses the run (`outside_root`, nothing read or
    /// written; ADR 0092 D8); a source with more than one name fails that file, and the compiles before and
    /// after read none it refuses.
    pub caller: Caller<'a>,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    /// The oldest format version among the project's files.
    pub from: FormatVersion,
    pub to: FormatVersion,
    /// The run was a preview: nothing was written.
    pub dry_run: bool,
    /// Whether any file needed migrating. When not, nothing else ran.
    pub pending: bool,
    /// Whether the migration was written: kept and committed (not a dry run, no file failed to migrate, and the
    /// verdict did not refuse it).
    pub applied: bool,
    /// The per-file migration (the preview, in a dry run or with nothing
    /// pending).
    pub summary: MigrationSummary,
    /// What judging the plan found: the same for a dry run and a real run. Not judged (the default) with nothing
    /// pending or a file that could not be migrated.
    pub verdict: Verdict,
    /// One E069 per problem in `specforge.json` the run went around.
    pub diagnostics: Vec<Diagnostic>,
    /// The files the run left changed: each backup, each migrated file and the record (or the removed record
    /// under `--no-backup`). Nothing for a dry run, a refused migration, with nothing pending, or when a file could
    /// not be migrated.
    pub writes: Writes,
}

/// Why a migration run failed ([`Outcome::failure`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// A migration hook failed (each failure as the hook's call reported it); nothing was written.
    HookFailed { hooks: Vec<String> },
    /// The migrated project reports errors; nothing was written.
    DoesNotCompile { errors: usize },
    /// The migrated project compiles to another graph (W054); nothing was written.
    ChangedGraph { differences: usize },
    /// Files that could not be migrated (each shown path and its error); nothing was written.
    FilesFailed { files: Vec<(String, String)> },
}

impl From<Failure> for OpError {
    /// The failure as every operation reports one: its cause picks the kind and the code.
    fn from(failure: Failure) -> OpError {
        match failure {
            Failure::HookFailed { hooks } => OpError::new(
                OpErrorKind::Internal,
                "migration_hook_failed",
                format!(
                    "a migration hook failed ({}); nothing was written",
                    hooks.join("; ")
                ),
            ),
            Failure::DoesNotCompile { errors } => OpError::new(
                OpErrorKind::CompilationFailed,
                "migration_does_not_compile",
                format!(
                    "the migrated project does not compile ({errors} error(s) it would add); nothing was written"
                ),
            ),
            Failure::ChangedGraph { differences } => OpError::new(
                OpErrorKind::CompilationFailed,
                "migration_changed_graph",
                format!(
                    "the migration changed the graph ({differences} structural difference(s), W054); nothing was written"
                ),
            ),
            Failure::FilesFailed { files } => {
                let first = files
                    .first()
                    .map(|(path, error)| format!(": {path}: {error}"))
                    .unwrap_or_default();
                OpError::new(
                    OpErrorKind::SchemaMismatch,
                    "migration_file_failed",
                    format!(
                        "{} file(s) could not be migrated{first}; nothing was written",
                        files.len()
                    ),
                )
            }
        }
    }
}

/// What `specforge migrate --format json` and `specforge.migrate` print
/// (`McpMigrateResult`): one record whether anything was pending or not.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct MigrateDocument {
    /// No file failed, the migration is not refused and `specforge.json` is used
    /// as written ([`Outcome::ok`]).
    pub ok: bool,
    /// Some file was behind `to_version`; when false nothing else ran.
    pub pending: bool,
    pub from_version: String,
    pub to_version: String,
    /// One E069 per problem in `specforge.json` the run went around; any
    /// fails `ok`.
    pub diagnostics: DiagnosticList,
    /// The migrated files are on disk now ([`Outcome::migrated`]).
    pub migrated: bool,
    pub dry_run: bool,
    pub files_migrated: usize,
    pub files_skipped: usize,
    pub files_failed: usize,
    pub results: Vec<MigrationResult>,
    pub diffs: Vec<MigrationDiff>,
    pub backups: Vec<MigrationBackup>,
    /// `extension:hook` for each migration hook that ran.
    pub hooks_invoked: Vec<String>,
    /// Why each failing hook failed.
    pub hook_failures: Vec<String>,
    /// How the migrated graph differs from the one before (W054).
    pub structural_differences: DiagnosticList,
    pub post_migration_validated: bool,
    /// The errors compiling the migrated project reported.
    pub post_migration_errors: DiagnosticList,
}

impl Outcome {
    /// Whether the migration only previewed, or ran and wrote [`Self::writes`].
    pub fn wrote(&self) -> Wrote {
        Wrote::of(self.dry_run, self.writes.clone())
    }

    /// How the migrated graph differs from the one before: one W054 per change in the verdict's graph delta;
    /// empty exactly when it is.
    pub fn structural_differences(&self) -> Vec<Diagnostic> {
        structural_differences(&self.verdict.graph_delta)
    }

    /// The document both surfaces print.
    pub fn document(&self) -> MigrateDocument {
        MigrateDocument {
            ok: self.ok(),
            pending: self.pending,
            from_version: self.from.to_string(),
            to_version: self.to.to_string(),
            diagnostics: DiagnosticList(self.diagnostics.clone()),
            migrated: self.migrated(),
            dry_run: self.dry_run,
            files_migrated: self.summary.migrated_count,
            files_skipped: self.summary.skipped_count,
            files_failed: self.summary.failed_count,
            results: self.summary.results.clone(),
            diffs: self.summary.diffs.clone(),
            backups: self.summary.backups.clone(),
            hooks_invoked: self.verdict.hooks_invoked.clone(),
            hook_failures: self.verdict.hook_failures.clone(),
            structural_differences: DiagnosticList(self.structural_differences()),
            post_migration_validated: self.verdict.compared,
            post_migration_errors: DiagnosticList(self.verdict.post_errors().cloned().collect()),
        }
    }

    /// Whether the migrated files are on disk now: a kept migration that rewrote a file.
    pub fn migrated(&self) -> bool {
        self.applied && self.summary.migrated_count > 0
    }

    /// The run's verdict, the same for a dry run and a real run: no file failed to migrate, the migration is not
    /// refused, and `specforge.json` is used as written (no E069). `specforge migrate` exits by it; MCP
    /// `specforge.migrate` returns it as `ok`. A failed run ([`Self::failure`]) always fails it; an E069 alone
    /// fails only it.
    pub fn ok(&self) -> bool {
        self.failure().is_none() && self.diagnostics.is_empty()
    }

    /// Why the run failed (ADR 0170): files that could not be migrated, else the verdict's refusal (a hook that
    /// failed, a project that does not compile, a graph that changed). `None` when every file migrated and the
    /// migration is kept: an E069 the run went around fails [`Self::ok`], not the run.
    pub fn failure(&self) -> Option<Failure> {
        if self.summary.failed_count > 0 {
            let files = self
                .summary
                .results
                .iter()
                .filter(|r| r.status == MigrationStatus::Failed)
                .map(|r| (r.file_path.to_string(), r.error.clone().unwrap_or_default()))
                .collect();
            return Some(Failure::FilesFailed { files });
        }
        self.verdict.refusal()
    }

    /// The run failed (ADR 0170), as every operation reports a failure: [`Self::failure`]'s kind,
    /// code and message, its data the migration document ([`Self::document`]) and its writes what
    /// the run left changed ([`Self::writes`]). `None` when [`Self::failure`] is.
    pub fn failed(&self) -> Option<OpError> {
        let failure = self.failure()?;
        let data = serde_json::to_value(self.document()).expect("a migration document serializes");
        Some(
            OpError::from(failure)
                .with_data(data)
                .with_writes(self.writes.clone()),
        )
    }
}

/// The migration hooks that ran (`extension:hook`), and why any failed.
pub type HookRun = (Vec<String>, Vec<String>);

/// What a migration hook is called with (the protocol's
/// `MigrationInput`): the format versions the project moves between and
/// the files the core migration rewrote.
pub use specforge_protocol_types::MigrationInput;

/// What judging a planned migration found (ADR 0186): the hooks that ran and why any failed; when none failed, what
/// the edited compile of the planned texts reports and its graph delta from the graph before, less the steps' own
/// changes.
#[derive(Debug, Clone, Default)]
pub struct Verdict {
    /// `extension:hook` for each migration hook that ran.
    pub hooks_invoked: Vec<String>,
    /// Why each failing hook failed.
    pub hook_failures: Vec<String>,
    /// The planned texts were compiled and compared: every hook passed.
    pub compared: bool,
    /// The delta from the graph before to the planned graph, source positions ignored, less the changes the
    /// migration steps declare (a title where a block had none). Empty when the run did not compare.
    pub graph_delta: GraphDelta,
    /// What the edited compile reports, in `specforge check`'s order.
    pub post_diagnostics: Vec<Diagnostic>,
    /// How many of its errors the project does not report now (`edit::validate::added`).
    pub added_errors: usize,
}

impl Verdict {
    /// Why the migration is refused, or `None` when it is kept: a hook that failed; else errors the planned
    /// project has that the project has not; else a graph that changed (as W054s).
    pub fn refusal(&self) -> Option<Failure> {
        if !self.hook_failures.is_empty() {
            return Some(Failure::HookFailed {
                hooks: self.hook_failures.clone(),
            });
        }
        if self.added_errors > 0 {
            return Some(Failure::DoesNotCompile {
                errors: self.added_errors,
            });
        }
        if self.graph_delta.is_empty() {
            return None;
        }
        Some(Failure::ChangedGraph {
            differences: structural_differences(&self.graph_delta).len(),
        })
    }

    /// The errors the planned project reports.
    pub fn post_errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.post_diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }
}

/// Judge `planned` (the planned text of each migrated source, by source key) against `pre`, the project compiled
/// before: run each extension's migration hook in `pre`'s runtime with `input`, in load order ([`invoke_hooks`]);
/// when none failed, compile the planned texts over `pre` in memory (`CompiledProject::edited`: reads no source,
/// shares `pre`'s environment) and compare graphs. Writes nothing; the hooks get no file system (ADR 0037).
pub fn judge(
    pre: &CompiledProject,
    planned: &BTreeMap<String, String>,
    input: &MigrationInput,
) -> Verdict {
    let (hooks_invoked, hook_failures) = match pre.environment().runtime().map(|r| r.as_ref()) {
        Some(runtime) => invoke_hooks(pre.environment().registries(), runtime, input),
        None => (Vec::new(), Vec::new()),
    };
    let mut verdict = Verdict {
        hooks_invoked,
        hook_failures,
        ..Verdict::default()
    };
    if !verdict.hook_failures.is_empty() {
        return verdict;
    }
    let post = pre.edited(planned);
    verdict.compared = true;
    verdict.graph_delta = beyond_steps(compute_graph_delta(pre.graph(), post.graph()), pre.graph());
    verdict.post_diagnostics = post.diagnostics();
    verdict.added_errors = added(&pre.checked_diagnostics(), verdict.post_diagnostics.clone())
        .0
        .len();
    verdict
}

/// What the hooks are called with: the format versions and the files the core migration rewrites.
fn hook_input(outcome: &Outcome, project: &specforge_common::ProjectRoot) -> MigrationInput {
    MigrationInput {
        from: outcome.from.to_string(),
        to: outcome.to.to_string(),
        files: outcome
            .summary
            .results
            .iter()
            .filter(|r| r.status == MigrationStatus::Migrated)
            .map(|r| r.file_path.under(project.dir()).display().to_string())
            .collect(),
    }
}

/// Remove the migration record, which a kept migration made without backups leaves out of date.
fn remove_record(record: &Held) -> Result<(), OpError> {
    match record.remove() {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(unwritable(&record.spelled, &e).prefixed("cannot migrate: "))
        }
        _ => Ok(()),
    }
}

/// Migrate the project at `request.root`: load the environment, plan with its field forms (the header and the
/// `titles` and `defaults` steps), compile in it, judge the plan in memory ([`judge`]: the extensions'
/// migration hooks in `runtime` in dependency order, then the planned texts compiled and compared with the project),
/// and write a kept migration as one project change. A dry run, or a migration whose hook fails or whose graph delta
/// is not empty, ends at the judgement: nothing is written. With no runtime no extension is loaded, so no hook runs.
///
/// Its `specforge.json` is read once, through [`crate::config::rewritable`]:
/// refused (`config_invalid`, nothing read or written) when it does not say
/// which files are the project's sources or which extensions it loads; each
/// other problem is one E069 in [`Outcome::diagnostics`] and fails
/// [`Outcome::ok`]. Each source is read twice, through the caller's containment: over MCP a source with more
/// than one name fails its file (ADR 0146, ADR 0158), and the project is compiled through it (ADR 0166). A file
/// that cannot be migrated leaves every file as it was. A write that fails (a read-only source, a changed one) is
/// the project change's `Err` (ADR 0155, ADR 0171).
pub fn run(request: &Request, runtime: Option<SharedRuntime>) -> Result<Outcome, OpError> {
    // The project the files are migrated in and the one the hooks and the checks compile.
    let project = request.root;
    let target = &request.target;
    let read = crate::config::rewritable(project.dir())?;
    let sources = read.config.sources(project.dir()).files();
    let containment = request.caller.containment();
    // Every source is held before any is read: one that lands outside the bound refuses the run (ADR 0092 D8).
    let held: HashMap<&Path, Held> = sources
        .iter()
        .map(|source| Ok((source.as_path(), containment.resolve(source)?)))
        .collect::<Result<_, OpError>>()?;
    // Every source is read, and the project compiled, through the caller's containment: over MCP a file with more
    // than one name fails its file and is E025 in the compile (ADR 0146, 0158, 0166).
    let bound = containment.disk();
    let read_source = |path: &Path| -> std::io::Result<String> {
        String::from_utf8(bound.read(path)?).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )
        })
    };
    // The environment the plan's defaults step reads its field forms from and the judge compiles in: loaded once,
    // before the plan (ADR 0186 D5, amended). It reads no source.
    let env = Environment::load(project, runtime);
    let plan = migrate_project(
        project,
        &sources,
        target,
        &read_source,
        env.registries().field_forms(),
    );
    let from = plan
        .results
        .iter()
        .filter_map(|r| r.from_version.clone())
        .min()
        .unwrap_or_else(|| target.clone());
    let pending = plan.migrated_count > 0 || plan.failed_count > 0;
    let mut outcome = Outcome {
        from,
        to: target.clone(),
        dry_run: request.dry_run,
        pending,
        applied: false,
        summary: plan,
        verdict: Verdict::default(),
        diagnostics: read.diagnostics(),
        writes: Writes::none(),
    };
    // Nothing pending, or a file that cannot be migrated: nothing is compiled, judged or written (ADR 0171): the
    // plan reports each one.
    if !pending || outcome.summary.failed_count > 0 {
        return Ok(outcome);
    }

    // The graph before any file is touched, read through the same bound: a refused source is its E025 (ADR 0166).
    // The plan is judged against it in memory (ADR 0186).
    let pre = CompiledProject::compile_through(env, &bound);
    let planned: BTreeMap<String, String> = outcome
        .summary
        .rewrites
        .iter()
        .map(|r| (pre.environment().source_key(&r.path), r.after.clone()))
        .collect();
    debug_assert!(
        planned
            .keys()
            .all(|key| pre.source_keys().contains(key.as_str())),
        "a planned source is one the compile holds"
    );
    outcome.verdict = judge(&pre, &planned, &hook_input(&outcome, project));
    // A preview reports the verdict; a refused migration writes nothing (ADR 0186).
    if request.dry_run || outcome.verdict.refusal().is_some() {
        return Ok(outcome);
    }

    // The kept plan as one change: per file its backup (a name the migration owns, with its source's permissions),
    // then the source; then the record.
    let rewrites = std::mem::take(&mut outcome.summary.rewrites);
    let record_name = project.dir().join(MigrationRecord::PATH);
    let (record, record_was) = if request.no_backup {
        (containment.resolve(&record_name)?, None)
    } else {
        match containment.read(&record_name) {
            Ok((record, was)) => (record, Some(was)),
            Err(ReadFailure::Outside(outside)) => return Err(outside.into()),
            Err(ReadFailure::Io { held, error })
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                (held, None)
            }
            Err(ReadFailure::Io { error, .. }) => {
                return Err(unreadable(&record_name, &error).prefixed("cannot migrate: "));
            }
        }
    };
    let recorded = (!request.no_backup).then(|| {
        MigrationRecord::of(project.dir(), target, &rewrites)
            .render()
            .into_bytes()
    });
    let mut change = ProjectChange::new(project.dir(), "cannot migrate");
    let mut staged: Vec<std::path::PathBuf> = Vec::new();
    for rewrite in rewrites {
        let source = held[rewrite.path.as_path()].clone();
        // Two names for one file (an inside link that is also a source) are written once.
        if staged.contains(&source.real) {
            continue;
        }
        if !request.no_backup {
            let backup = containment.resolve(&backup_of(&rewrite.path))?;
            let mode = source
                .permissions()
                .map_err(|e| unreadable(&source.spelled, &e).prefixed("cannot migrate: "))?;
            outcome.summary.backups.push(MigrationBackup {
                original_path: rewrite.name.clone(),
                backup_path: specforge_common::ProjectPath::of(project.dir(), &backup.spelled),
            });
            change.stage_owned(backup, rewrite.before.clone().into_bytes(), mode);
        }
        staged.push(source.real.clone());
        change.stage(
            source,
            Some(rewrite.before.into_bytes()),
            rewrite.after.into_bytes(),
        );
    }
    if let Some(recorded) = recorded {
        change.stage(record.clone(), record_was, recorded);
    }

    // A kept migration made without backups removes the record, since a rollback after it would mix versions.
    let written = if request.no_backup {
        change.commit_then(|| remove_record(&record))
    } else {
        change.commit()
    }?;
    outcome.writes = written;
    outcome.applied = true;
    Ok(outcome)
}

/// What a rollback rewrote: each file it restored from its backup.
fn restored_writes(root: &Path, summary: &RollbackSummary) -> Writes {
    summary
        .results
        .iter()
        .filter(|r| r.status == MigrationStatus::Restored)
        .map(|r| r.file_path.under(root))
        .collect()
}

/// What a rollback did: the restore, and the files it rewrote.
#[derive(Debug, Clone)]
pub struct RollbackOutcome {
    pub summary: RollbackSummary,
    /// Each file restored from its backup (ADR 0022 D1).
    pub writes: Writes,
}

impl RollbackOutcome {
    /// The run's verdict: no file failed to restore. `specforge migrate
    /// --rollback` exits by it.
    pub fn ok(&self) -> bool {
        self.summary.failed_count == 0
    }
}

/// Restore every migrated file of the project `root` is in from its `.bak` backup, each by replacing its name
/// through `caller`'s containment: a file keeps its permissions and one the process may not write fails its own
/// restore (ADR 0171). The record is read, and rewritten or removed, through the same containment; a record that
/// lands outside it refuses the call (`outside_root`, nothing read or written).
pub fn rollback(
    root: &specforge_common::ProjectRoot,
    caller: Caller<'_>,
) -> Result<RollbackOutcome, OpError> {
    let containment = caller.containment();
    let bound = containment.disk();
    let record_name = root.dir().join(MigrationRecord::PATH);
    let (held, record) = match containment.read(&record_name) {
        Ok((held, bytes)) => {
            let record = String::from_utf8(bytes)
                .map_err(|e| e.to_string())
                .and_then(|text| MigrationRecord::parse(&text))
                .map(Some);
            (held, record)
        }
        Err(ReadFailure::Outside(outside)) => return Err(outside.into()),
        Err(ReadFailure::Io { held, error }) if error.kind() == std::io::ErrorKind::NotFound => {
            (held, Ok(None))
        }
        Err(ReadFailure::Io { held, error }) => (
            held,
            Err(format!("can't read {}: {error}", MigrationRecord::PATH)),
        ),
    };
    let read = |path: &Path| -> std::io::Result<String> {
        String::from_utf8(bound.read(path)?).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )
        })
    };
    let write = |target: &Path, text: &str| -> Result<(), String> {
        containment
            .resolve(target)
            .map_err(|outside| OpError::from(outside).message)
            .and_then(|held| held.replace(text).map_err(|e| e.to_string()))
            .map_err(|e| format!("failed to restore: {e}"))
    };
    let mut summary = run_rollback(root, record, &read, &write);
    let applied = match &summary.record {
        RecordChange::Unchanged => Ok(()),
        RecordChange::Removed => held.remove().or_else(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(e),
        }),
        RecordChange::Rewritten(kept) => held.replace(kept.render()),
    };
    if let Err(e) = applied {
        summary.record = RecordChange::Unchanged;
        summary
            .warnings
            .push(format!("could not update {}: {e}", MigrationRecord::PATH));
    }
    let mut writes = restored_writes(root.dir(), &summary);
    if summary.record != RecordChange::Unchanged {
        writes.record(&record_name);
    }
    Ok(RollbackOutcome { summary, writes })
}

/// Run each extension's declared migration hook, in the registry build's load order (ADR 0041).
/// A hook that fails (E028: it trapped, or the extension does not route it) is recorded and the
/// rest still run. Returns the hooks run (`extension:hook`) and the failures.
pub fn invoke_hooks(
    build: &RegistryBuild,
    runtime: &dyn WasmRuntime,
    input: &MigrationInput,
) -> HookRun {
    let calls = specforge_wasm::ExtensionCalls::new(runtime);

    let mut invoked = Vec::new();
    let mut failures = Vec::new();
    for declaration in build.declarations() {
        let name = declaration.name();
        let Some(hook) = declaration
            .handshake
            .migration_hook
            .as_deref()
            .filter(|h| !h.is_empty())
        else {
            continue;
        };
        // The hook's answer is not read: the protocol defines none.
        match calls.migrate(name, hook, input) {
            Ok(()) => invoked.push(format!("{name}:{hook}")),
            Err(error) => failures.push(error.to_string()),
        }
    }
    (invoked, failures)
}
