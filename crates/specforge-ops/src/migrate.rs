//! `specforge migrate` and the MCP `specforge.migrate` tool: one migration.
//!
//! The run previews first: a project with nothing to migrate is left alone
//! (no compile, no hooks). Otherwise it compiles the project, migrates the
//! files, runs the extensions' migration hooks, and compiles again. When a
//! hook fails, or the graph's structure changed, the migrated files are
//! restored from their backups.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, DiagnosticList, Severity, codes};
/// The migration crate's report types, as this operation's interface.
pub use specforge_migrate::{
    MigrationBackup, MigrationDiff, MigrationRecord, MigrationResult, MigrationStatus,
    MigrationSummary, RecordChange, RollbackSummary,
};
use specforge_migrate::{
    check_schema_compatibility, compare_graphs, migrate_project, restore, run_rollback,
};
use specforge_parser::{
    CURRENT_FORMAT_VERSION, FormatVersion, MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION,
};
use specforge_project::{CompiledProject, SharedRuntime};
use specforge_registry::RegistryBuild;
use specforge_wasm::WasmRuntime;
use std::path::Path;

use crate::{OpError, OpErrorKind, Writes};

/// The format version to migrate to: `raw`, checked, else the current one.
/// A version that doesn't parse, or one newer than this build supports, is
/// E019.
pub fn parse_target(raw: Option<&str>) -> Result<FormatVersion, OpError> {
    let Some(raw) = raw else {
        return Ok(CURRENT_FORMAT_VERSION);
    };
    let supported = format!(
        "Use a format version between {MIN_SUPPORTED_VERSION} and {MAX_SUPPORTED_VERSION}."
    );
    match raw.parse::<FormatVersion>() {
        Ok(version) if version > MAX_SUPPORTED_VERSION => Err(OpError::diagnostic(
            codes::E019,
            format!("unsupported target version {raw} (max supported: {MAX_SUPPORTED_VERSION})"),
        )
        .with_suggestion(supported)),
        Ok(version) => Ok(version),
        Err(e) => Err(OpError::diagnostic(
            codes::E019,
            format!("invalid target version '{raw}': {e}"),
        )
        .with_suggestion(supported)),
    }
}

pub struct Request<'a> {
    pub root: &'a specforge_common::ProjectRoot,
    pub target: FormatVersion,
    /// Report what would change; write nothing.
    pub dry_run: bool,
    /// Skip the `.bak` copy of each migrated file (nothing to roll back to).
    pub no_backup: bool,
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
    /// Whether files were rewritten (not a dry run, and not every file
    /// failed). A rolled-back migration was applied, then undone.
    pub applied: bool,
    /// The per-file migration (the preview, in a dry run or with nothing
    /// pending).
    pub summary: MigrationSummary,
    /// `extension:hook` for each migration hook that ran.
    pub hooks_invoked: Vec<String>,
    /// Why each failing hook failed.
    pub hook_failures: Vec<String>,
    /// Whether the migrated project was compiled and compared.
    pub validated: bool,
    /// Breaking Graph Protocol schema changes (W053).
    pub schema_warnings: Vec<Diagnostic>,
    /// How the migrated graph differs from the one before (W054).
    pub structural_differences: Vec<Diagnostic>,
    /// What compiling the migrated project reported.
    pub post_diagnostics: Vec<Diagnostic>,
    /// The restore, when the migration was rolled back.
    pub rollback: Option<RollbackSummary>,
    /// The files the run left changed: each migrated file and each backup;
    /// after a rollback, the backups (the migrated files hold their old
    /// text again). Nothing for a dry run or with nothing pending.
    pub writes: Writes,
}

/// What `specforge migrate --format json` and `specforge.migrate` print
/// (`McpMigrateResult`): one record whether anything was pending or not.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct MigrateDocument {
    /// No file failed and nothing was rolled back ([`Outcome::ok`]).
    pub ok: bool,
    /// Some file was behind `to_version`; when false nothing else ran.
    pub pending: bool,
    pub from_version: String,
    pub to_version: String,
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
    /// Breaking Graph Protocol schema changes (W053).
    pub schema_warnings: DiagnosticList,
    /// How the migrated graph differs from the one before (W054).
    pub structural_differences: DiagnosticList,
    pub rolled_back: bool,
    /// The restore, when the migration was rolled back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback: Option<RollbackSummary>,
    pub post_migration_validated: bool,
    /// The errors compiling the migrated project reported.
    pub post_migration_errors: DiagnosticList,
}

impl Outcome {
    /// The document both surfaces print.
    pub fn document(&self) -> MigrateDocument {
        MigrateDocument {
            ok: self.ok(),
            pending: self.pending,
            from_version: self.from.to_string(),
            to_version: self.to.to_string(),
            migrated: self.migrated(),
            dry_run: self.dry_run,
            files_migrated: self.summary.migrated_count,
            files_skipped: self.summary.skipped_count,
            files_failed: self.summary.failed_count,
            results: self.summary.results.clone(),
            diffs: self.summary.diffs.clone(),
            backups: self.summary.backups.clone(),
            hooks_invoked: self.hooks_invoked.clone(),
            hook_failures: self.hook_failures.clone(),
            schema_warnings: DiagnosticList(self.schema_warnings.clone()),
            structural_differences: DiagnosticList(self.structural_differences.clone()),
            rolled_back: self.rollback.is_some(),
            rollback: self.rollback.clone(),
            post_migration_validated: self.validated,
            post_migration_errors: DiagnosticList(self.post_errors().cloned().collect()),
        }
    }

    /// Whether the migrated files are on disk now.
    pub fn migrated(&self) -> bool {
        self.applied && self.summary.migrated_count > 0 && self.rollback.is_none()
    }

    /// The run's verdict: no file failed to migrate and nothing was rolled
    /// back. `specforge migrate` exits by it; MCP `specforge.migrate`
    /// returns it as `ok`.
    pub fn ok(&self) -> bool {
        self.summary.failed_count == 0 && self.rollback.is_none()
    }

    /// Why the run failed, as every operation reports a failure; `None`
    /// when [`Self::ok`]. Code `migration_failed`; kind `CompilationFailed`
    /// when the migrated project reported errors before it was rolled
    /// back, else `Internal`; message "the migrated project does not
    /// compile" or "the migration failed".
    pub fn failure(&self) -> Option<OpError> {
        if self.ok() {
            return None;
        }
        let (kind, message) = if self.post_errors().next().is_some() {
            (
                OpErrorKind::CompilationFailed,
                "the migrated project does not compile",
            )
        } else {
            (OpErrorKind::Internal, "the migration failed")
        };
        Some(OpError::new(kind, "migration_failed", message))
    }

    /// The errors compiling the migrated project reported.
    pub fn post_errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.post_diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }
}

/// The migration hooks that ran (`extension:hook`), and why any failed.
pub type HookRun = (Vec<String>, Vec<String>);

/// What a migration hook is called with (the protocol's
/// `MigrationInput`): the format versions the project moves between and
/// the files the core migration rewrote.
pub use specforge_protocol_types::MigrationInput;

/// Migrate the project at `request.root`: preview, compile, rewrite the files,
/// run the extensions' migration hooks in `runtime` in dependency order,
/// compile again and compare. A failing hook or a changed graph structure
/// restores every migrated file from its backup. With no runtime no
/// extension is loaded, so no hook runs.
pub fn run(request: &Request, runtime: Option<SharedRuntime>) -> Outcome {
    // The project the files are migrated in and the one the hooks and the checks compile.
    let project = request.root;
    let target = &request.target;
    let preview = migrate_project(project, target, true, true);
    let from = preview
        .results
        .iter()
        .filter_map(|r| r.from_version.clone())
        .min()
        .unwrap_or_else(|| target.clone());
    let pending = preview.migrated_count > 0 || preview.failed_count > 0;
    let mut outcome = Outcome {
        from,
        to: target.clone(),
        dry_run: request.dry_run,
        pending,
        applied: false,
        summary: preview,
        hooks_invoked: Vec::new(),
        hook_failures: Vec::new(),
        validated: false,
        schema_warnings: Vec::new(),
        structural_differences: Vec::new(),
        post_diagnostics: Vec::new(),
        rollback: None,
        writes: Writes::none(),
    };
    if !pending || request.dry_run {
        return outcome;
    }

    // The graph and schema before any file is touched.
    let pre = CompiledProject::compile(project, runtime.clone());
    let pre_schema = schema_of(&pre);

    outcome.summary = migrate_project(project, target, false, request.no_backup);
    outcome.applied = true;
    outcome.writes = summary_writes(&outcome.summary);
    if outcome.summary.failed_count > 0 {
        return outcome;
    }

    // Extension hooks run on the migrated files, before validation.
    let input = MigrationInput {
        from: outcome.from.to_string(),
        to: outcome.to.to_string(),
        files: outcome
            .summary
            .results
            .iter()
            .filter(|r| r.status == MigrationStatus::Migrated)
            .map(|r| r.file_path.clone())
            .collect(),
    };
    let (invoked, failures) = match pre.environment().runtime().map(|r| r.as_ref()) {
        Some(runtime) => invoke_hooks(pre.environment().registries(), runtime, &input),
        None => (Vec::new(), Vec::new()),
    };
    outcome.hooks_invoked = invoked;
    outcome.hook_failures = failures;
    if !outcome.hook_failures.is_empty() {
        roll_back(&mut outcome);
        return outcome;
    }

    // The migrated project must have the graph it had before.
    let post = CompiledProject::compile(project, runtime);
    outcome.validated = true;
    outcome.schema_warnings = check_schema_compatibility(&pre_schema, &schema_of(&post));
    outcome.structural_differences = compare_graphs(pre.graph(), post.graph());
    outcome.post_diagnostics = post.diagnostics();
    if !outcome.structural_differences.is_empty() {
        roll_back(&mut outcome);
        return outcome;
    }
    keep_record(project.dir(), request, &mut outcome);
    outcome
}

/// A kept migration made with backups is recorded for a later `--rollback`; one made without removes
/// the record, since a rollback after it would mix versions.
fn keep_record(root: &Path, request: &Request, outcome: &mut Outcome) {
    if outcome.summary.migrated_count == 0 {
        return;
    }
    let record = root.join(MigrationRecord::PATH);
    if request.no_backup {
        if matches!(MigrationRecord::remove(root), Ok(true)) {
            outcome.writes.record(&record);
        }
        return;
    }
    let made = MigrationRecord::of(root, &request.target, &outcome.summary.backups)
        .and_then(|record| record.write(root));
    if made.is_ok() {
        outcome.writes.record(&record);
    }
}

/// What `migrate_project` wrote: each file it migrated and each backup
/// it made.
fn summary_writes(summary: &MigrationSummary) -> Writes {
    let migrated = summary
        .results
        .iter()
        .filter(|r| r.status == MigrationStatus::Migrated)
        .map(|r| r.file_path.as_str());
    let backups = summary.backups.iter().map(|b| b.backup_path.as_str());
    migrated.chain(backups).collect()
}

/// Restore the files this run migrated, with the text it read before them (backups or not) after a
/// failed check: each is forgotten as a write. No other file is touched.
fn roll_back(outcome: &mut Outcome) {
    let summary = restore(&outcome.summary.originals);
    for restored in summary
        .results
        .iter()
        .filter(|r| r.status == MigrationStatus::Restored)
    {
        outcome.writes.forget(Path::new(&restored.file_path));
    }
    outcome.rollback = Some(summary);
}

/// What a rollback rewrote: each file it restored from its backup.
fn restored_writes(summary: &RollbackSummary) -> Writes {
    summary
        .results
        .iter()
        .filter(|r| r.status == MigrationStatus::Restored)
        .map(|r| r.file_path.as_str())
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

/// Restore every migrated file of the project `root` is in from its `.bak`
/// backup.
pub fn rollback(root: &specforge_common::ProjectRoot) -> RollbackOutcome {
    let summary = run_rollback(root);
    let mut writes = restored_writes(&summary);
    if summary.record != RecordChange::Unchanged {
        writes.record(root.dir().join(MigrationRecord::PATH));
    }
    RollbackOutcome { summary, writes }
}

fn schema_of(project: &CompiledProject) -> specforge_emitter::GraphProtocolSchema {
    specforge_emitter::generate_schema(project.environment().registries())
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
