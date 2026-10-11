use crate::OutputFormat;
use crate::outcome::{Exit, Refusal, Written, print_json};
use specforge_ops::migrate::{self, MigrateDocument, MigrationStatus, Request, RollbackSummary};
use std::path::Path;

pub fn run(
    path: &Path,
    dry_run: bool,
    no_backup: bool,
    rollback: bool,
    target_version: Option<&str>,
    format: OutputFormat,
) -> Exit {
    // Handle rollback mode
    if rollback {
        let outcome = migrate::rollback(path);
        print_rollback(&outcome.summary, &outcome.writes.names_under(path), format);
        return Exit::of_verdict(outcome.ok());
    }

    let target = match migrate::parse_target(target_version) {
        Ok(target) => target,
        Err(error) => return Refusal::of(format).report(&error),
    };

    // The shared migration: migrate, run the extensions' hooks, then check
    // the graph kept its structure, rolling back when it didn't.
    let runtime: specforge_project::SharedRuntime =
        std::sync::Arc::new(specforge_component::ComponentRuntime::with_user_cache());
    let request = Request {
        root: path,
        target,
        dry_run,
        no_backup,
    };
    let outcome = migrate::run(&request, Some(runtime));
    // Each migrated file and each backup (none for a dry run).
    let written = (!dry_run).then(|| outcome.writes.names_under(path));
    print_migration(&outcome.document(), written, format);

    if outcome.summary.failed_count != 0 {
        return Exit::of_verdict(outcome.ok());
    }
    for failure in &outcome.hook_failures {
        eprintln!("migration hook failure: {failure}");
    }
    // Graph Protocol compatibility: breaking schema changes warn W053.
    for warning in &outcome.schema_warnings {
        eprintln!("warning[{}]: {}", warning.code, warning.message);
    }
    for d in &outcome.structural_differences {
        eprintln!("{}: {}", d.code, d.message);
    }
    if outcome.rollback.is_some() {
        if outcome.hook_failures.is_empty() {
            eprintln!("migration changed the graph structure; files restored from backups");
        } else {
            eprintln!("files restored from backups");
        }
    }

    Exit::of_verdict(outcome.ok())
}

/// The restore; its JSON lists each file restored in `files_written`.
fn print_rollback(summary: &RollbackSummary, files_written: &[String], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            print_json(&Written::of(summary, files_written.to_vec()));
        }
        OutputFormat::Human => {
            for w in &summary.warnings {
                eprintln!("warning: {w}");
            }
            for r in &summary.results {
                match r.status {
                    MigrationStatus::Restored => eprintln!("  restored: {}", r.file_path),
                    MigrationStatus::Skipped => {}
                    MigrationStatus::Failed => {
                        eprintln!(
                            "  failed: {} ({})",
                            r.file_path,
                            r.error.as_deref().unwrap_or("unknown")
                        );
                    }
                    _ => {}
                }
            }
            eprintln!(
                "{} restored, {} skipped, {} failed",
                summary.restored_count, summary.skipped_count, summary.failed_count
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
) {
    match format {
        OutputFormat::Json => print_json(&Written {
            document,
            files_written,
        }),
        OutputFormat::Human => {
            if document.dry_run {
                for d in &document.diffs {
                    println!("{}", d.unified_text);
                }
            }

            eprintln!(
                "{} migrated, {} skipped, {} failed",
                document.files_migrated, document.files_skipped, document.files_failed
            );
        }
    }
}
