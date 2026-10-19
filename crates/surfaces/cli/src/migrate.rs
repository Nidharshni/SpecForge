use crate::notes::Notes;
use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal, Written};
use crate::terminal::{Terminal, outln};
use specforge_ops::authoring::migrate::{
    self, MigrateDocument, MigrationStatus, Request, RollbackSummary,
};
use specforge_ops::{OpError, Writes, Wrote};
use std::path::Path;

// `specforge migrate`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: crate::project::ProjectArg,

    /// Show unified diff without modifying files
    #[arg(long)]
    dry_run: bool,

    /// Skip creating .spec.bak backup files
    #[arg(long)]
    no_backup: bool,

    /// Restore files from .spec.bak backups
    #[arg(long)]
    rollback: bool,

    /// Target format version (defaults to current)
    #[arg(long)]
    target_version: Option<String>,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        self.run_in(
            std::sync::Arc::new(specforge_component::ComponentRuntime::with_user_cache()),
            terminal,
        )
    }

    /// [`Self::run`] over `runtime`: the extensions' hooks run in it (a test serves them in process).
    fn run_in(self, runtime: specforge_project::SharedRuntime, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let root = match self.project.root() {
            Ok(root) => root,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let path = root.dir();
        let (dry_run, no_backup) = (self.dry_run, self.no_backup);
        let notes = Notes::new(terminal, format);
        // Handle rollback mode
        if self.rollback {
            let outcome = migrate::rollback(&root);
            print_rollback(
                path,
                &outcome.summary,
                &outcome.writes,
                format,
                terminal,
                &notes,
            );
            return Exit::of_verdict(outcome.ok());
        }

        let target = match migrate::parse_target(self.target_version.as_deref()) {
            Ok(target) => target,
            Err(error) => return Refusal::of(format).at(path).report(terminal, &error),
        };

        // The shared migration: migrate, run the extensions' hooks, then check
        // the graph kept its structure, rolling back when it didn't.
        let request = Request {
            root: &root,
            target,
            dry_run,
            no_backup,
            caller: crate::caller::of(false, format, terminal),
        };
        let outcome = match migrate::run(&request, Some(runtime)) {
            Ok(outcome) => outcome,
            // A config that does not say which files are the sources or
            // which extensions load: nothing was read or written.
            Err(error) => return Refusal::of(format).at(path).report(terminal, &error),
        };
        // Each migrated file and each backup (none for a dry run).
        print_migration(
            &outcome.document(),
            &outcome.wrote(),
            path,
            format,
            terminal,
            &notes,
        );

        if let Some(failure) = outcome.failure().filter(|_| outcome.rollback.is_none()) {
            notes.person(format_args!("{}", OpError::from(failure).message));
            return Exit::of_verdict(outcome.ok());
        }
        // The document carries the hook failures and the structural differences; the restore is a person's line.
        for failure in &outcome.hook_failures {
            notes.person(format_args!("migration hook failure: {failure}"));
        }
        let differences = outcome.structural_differences();
        notes.carried(&differences);
        if outcome.rollback.is_some() {
            if outcome.hook_failures.is_empty() {
                notes.person(format_args!(
                    "migration changed the graph structure; files restored from backups"
                ));
            } else {
                notes.person(format_args!("files restored from backups"));
            }
        }

        Exit::of_verdict(outcome.ok())
    }
}

/// The restore; its JSON lists each file restored in `files_written`.
fn print_rollback(
    root: &Path,
    summary: &RollbackSummary,
    writes: &Writes,
    format: OutputFormat,
    terminal: &Terminal,
    notes: &Notes<'_>,
) {
    match format {
        OutputFormat::Json => {
            terminal.json(&Written::ran(summary, writes, root));
        }
        OutputFormat::Human => {
            for w in &summary.warnings {
                notes.person(format_args!("warning: {w}"));
            }
            for r in &summary.results {
                match r.status {
                    MigrationStatus::Restored => {
                        notes.person(format_args!(
                            "  restored: {}",
                            r.file_path.under(root).display()
                        ));
                    }
                    MigrationStatus::Skipped => {}
                    MigrationStatus::Failed => {
                        notes.person(format_args!(
                            "  failed: {} ({})",
                            r.file_path.under(root).display(),
                            r.error.as_deref().unwrap_or("unknown")
                        ));
                    }
                    _ => {}
                }
            }
            notes.person(format_args!(
                "{} restored, {} skipped, {} failed",
                summary.restored_count, summary.skipped_count, summary.failed_count
            ));
        }
    }
}

/// The migration; its JSON is the migration document with `files_written`
/// (absent from a dry run).
fn print_migration(
    document: &MigrateDocument,
    wrote: &Wrote,
    root: &Path,
    format: OutputFormat,
    terminal: &Terminal,
    notes: &Notes<'_>,
) {
    match format {
        OutputFormat::Json => {
            terminal.json(&Written::of(document, wrote, root));
        }
        OutputFormat::Human => {
            if document.dry_run {
                for d in &document.diffs {
                    outln!(terminal, "{}", d.unified_text);
                }
            }
            for d in &document.diagnostics.0 {
                notes.person(format_args!("{}[{}]: {}", d.severity, d.code, d.message));
            }

            notes.person(format_args!(
                "{} migrated, {} skipped, {} failed",
                document.files_migrated, document.files_skipped, document.files_failed
            ));
        }
    }
}

#[cfg(test)]
mod tests;
