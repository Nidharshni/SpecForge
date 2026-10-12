use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal, Written};
use crate::terminal::{Terminal, errln, outln};
use specforge_ops::migrate::{self, MigrateDocument, MigrationStatus, Request, RollbackSummary};
use std::path::PathBuf;

// `specforge migrate`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Path to the spec root directory
    #[arg(long, default_value = ".")]
    path: PathBuf,

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
        let format = self.output.format;
        let root = match crate::project::root(&self.path) {
            Ok(root) => root,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        let path = root.dir();
        let (dry_run, no_backup) = (self.dry_run, self.no_backup);
        // Handle rollback mode
        if self.rollback {
            let outcome = migrate::rollback(&root);
            print_rollback(
                &outcome.summary,
                &outcome.writes.names_under(path),
                format,
                terminal,
            );
            return Exit::of_verdict(outcome.ok());
        }

        let target = match migrate::parse_target(self.target_version.as_deref()) {
            Ok(target) => target,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };

        // The shared migration: migrate, run the extensions' hooks, then check
        // the graph kept its structure, rolling back when it didn't.
        let runtime: specforge_project::SharedRuntime =
            std::sync::Arc::new(specforge_component::ComponentRuntime::with_user_cache());
        let request = Request {
            root: &root,
            target,
            dry_run,
            no_backup,
        };
        let outcome = migrate::run(&request, Some(runtime));
        // Each migrated file and each backup (none for a dry run).
        let written = (!dry_run).then(|| outcome.writes.names_under(path));
        print_migration(&outcome.document(), written, format, terminal);

        if outcome.summary.failed_count != 0 {
            return Exit::of_verdict(outcome.ok());
        }
        for failure in &outcome.hook_failures {
            errln!(terminal, "migration hook failure: {failure}");
        }
        // Graph Protocol compatibility: breaking schema changes warn W053.
        for warning in &outcome.schema_warnings {
            errln!(terminal, "warning[{}]: {}", warning.code, warning.message);
        }
        for d in &outcome.structural_differences {
            errln!(terminal, "{}: {}", d.code, d.message);
        }
        if outcome.rollback.is_some() {
            if outcome.hook_failures.is_empty() {
                errln!(
                    terminal,
                    "migration changed the graph structure; files restored from backups"
                );
            } else {
                errln!(terminal, "files restored from backups");
            }
        }

        Exit::of_verdict(outcome.ok())
    }
}

/// The restore; its JSON lists each file restored in `files_written`.
fn print_rollback(
    summary: &RollbackSummary,
    files_written: &[String],
    format: OutputFormat,
    terminal: &Terminal,
) {
    match format {
        OutputFormat::Json => {
            terminal.json(&Written::of(summary, files_written.to_vec()));
        }
        OutputFormat::Human => {
            for w in &summary.warnings {
                errln!(terminal, "warning: {w}");
            }
            for r in &summary.results {
                match r.status {
                    MigrationStatus::Restored => errln!(terminal, "  restored: {}", r.file_path),
                    MigrationStatus::Skipped => {}
                    MigrationStatus::Failed => {
                        errln!(
                            terminal,
                            "  failed: {} ({})",
                            r.file_path,
                            r.error.as_deref().unwrap_or("unknown")
                        );
                    }
                    _ => {}
                }
            }
            errln!(
                terminal,
                "{} restored, {} skipped, {} failed",
                summary.restored_count,
                summary.skipped_count,
                summary.failed_count
            );
        }
    }
}

/// The migration; its JSON is the migration document with `files_written`
/// (absent from a dry run).
fn print_migration(
    document: &MigrateDocument,
    files_written: Option<Vec<String>>,
    format: OutputFormat,
    terminal: &Terminal,
) {
    match format {
        OutputFormat::Json => terminal.json(&Written {
            document,
            files_written,
        }),
        OutputFormat::Human => {
            if document.dry_run {
                for d in &document.diffs {
                    outln!(terminal, "{}", d.unified_text);
                }
            }

            errln!(
                terminal,
                "{} migrated, {} skipped, {} failed",
                document.files_migrated,
                document.files_skipped,
                document.files_failed
            );
        }
    }
}
