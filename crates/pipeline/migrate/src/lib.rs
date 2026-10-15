//! Format-version migration of a project's `.spec` files: the rewrite to a target format version, the backups and
//! the `.specforge/migration.json` record a rollback undoes, and W054 for each thing the rewrite changed in the
//! graph (`structural_differences`).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, ProjectRoot, Severity, codes};
use specforge_formatter::unified_diff;
use specforge_graph::{EdgeChange, GraphDelta, ModifiedNodeChange, NodeAspect};
use specforge_parser::{FormatVersion, detect_format_version, with_format_version};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Migration Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Shape)]
#[shape(define)]
pub struct MigrationResult {
    pub file_path: String,
    pub status: MigrationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_version: Option<FormatVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_version: Option<FormatVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    Migrated,
    Skipped,
    Failed,
    Restored,
}

#[derive(Debug, Clone, Serialize, Shape)]
pub struct MigrationBackup {
    pub original_path: String,
    pub backup_path: String,
}

#[derive(Debug, Clone, Serialize, Shape)]
#[shape(define)]
pub struct MigrationDiff {
    pub file_path: String,
    pub before_hash: String,
    pub after_hash: String,
    pub unified_text: String,
}

#[derive(Debug, Clone, Serialize, Shape)]
pub struct MigrationSummary {
    pub migrated_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub target_version: FormatVersion,
    pub results: Vec<MigrationResult>,
    pub backups: Vec<MigrationBackup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diffs: Vec<MigrationDiff>,
    /// Every file this run migrated, with the text it read before (empty for a dry run): what an
    /// automatic rollback restores, backups or not.
    #[serde(skip)]
    pub originals: Vec<Original>,
}

/// A file a migration rewrote, with the text it held before.
#[derive(Debug, Clone)]
pub struct Original {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Shape)]
#[shape(define)]
pub struct RollbackSummary {
    pub restored_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub results: Vec<MigrationResult>,
    /// One warning per file skipped (its backup missing, or the file edited since the migration), and
    /// one when no migration is recorded.
    pub warnings: Vec<String>,
    /// What the rollback did to the migration record.
    #[serde(skip)]
    pub record: RecordChange,
}

/// What a rollback did to `.specforge/migration.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordChange {
    #[default]
    Unchanged,
    /// It keeps only the files not restored.
    Rewritten,
    /// Every recorded file was restored.
    Removed,
}

/// What `specforge migrate --rollback` undoes: the last kept migration of the project, made with
/// backups, recorded at `<root>/.specforge/migration.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationRecord {
    /// The format version the files were migrated to.
    pub target: String,
    pub files: Vec<RecordedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedFile {
    /// Relative to the project root, `/`-separated.
    pub path: String,
    /// The backup holding the file as it was before, relative to the project root.
    pub backup: String,
    /// SHA-256 (hex) of the file as the migration left it, extension hooks included.
    pub sha256: String,
}

impl MigrationRecord {
    /// `.specforge/migration.json`.
    pub const PATH: &'static str = ".specforge/migration.json";

    /// The record of the files `backups` hold the old text of, hashed as they are now.
    pub fn of(
        root: &Path,
        target: &FormatVersion,
        backups: &[MigrationBackup],
    ) -> std::io::Result<Self> {
        let files = backups
            .iter()
            .map(|backup| {
                let file = Path::new(&backup.original_path);
                Ok(RecordedFile {
                    path: diff_label(file, root),
                    backup: diff_label(Path::new(&backup.backup_path), root),
                    sha256: sha256_hash(&std::fs::read_to_string(file)?),
                })
            })
            .collect::<std::io::Result<Vec<_>>>()?;
        Ok(Self {
            target: target.to_string(),
            files,
        })
    }

    /// The project's record; `None` when there is none. `Err` names a file that can't be read as one.
    pub fn read(root: &Path) -> Result<Option<Self>, String> {
        let path = root.join(Self::PATH);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("can't read {}: {e}", Self::PATH)),
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{} is not a migration record: {e}", Self::PATH))
    }

    pub fn write(&self, root: &Path) -> std::io::Result<()> {
        let path = root.join(Self::PATH);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        text.push('\n');
        std::fs::write(path, text)
    }

    /// Remove the record; `true` when there was one.
    pub fn remove(root: &Path) -> std::io::Result<bool> {
        match std::fs::remove_file(root.join(Self::PATH)) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Transform Functions (pure)
// ---------------------------------------------------------------------------

/// Transform content to version `to`.
/// Currently v1 is the only version, so this just ensures the header is set.
fn transform_content(content: &str, to: &FormatVersion) -> String {
    with_format_version(content, to)
}

// ---------------------------------------------------------------------------
// SHA256 Hashing
// ---------------------------------------------------------------------------

fn sha256_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// Post-migration checks
// ---------------------------------------------------------------------------

/// What a migration changed in the graph, as W054 warnings, from the graph delta between the graph before it and the
/// graph after (source positions ignored, ADR 0032): one per entity removed, one per entity added, one per entity
/// whose content changed (naming its changed fields, then what else changed: `kind`, `title`, `methods` or `edges`),
/// then one per edge removed and one per edge added. Empty exactly when `delta` is: a migration keeps the graph only
/// then.
pub fn structural_differences(delta: &GraphDelta) -> Vec<Diagnostic> {
    let warn = |message: String| Diagnostic::new(codes::W054, message);
    let edge = |e: &EdgeChange| format!("edge {}-[{}]->{}", e.source, e.label, e.target);
    let removed = delta.removed_nodes.iter().map(|n| {
        warn(format!(
            "entity '{}' ({}) present before migration but missing after",
            n.id, n.kind
        ))
    });
    let added = delta.added_nodes.iter().map(|n| {
        warn(format!(
            "entity '{}' ({}) appeared after migration but was not present before",
            n.id, n.kind
        ))
    });
    let modified = delta.modified_nodes.iter().map(|n| {
        warn(format!(
            "entity '{}' changed during migration: {}",
            n.id,
            changed(n)
        ))
    });
    let lost = delta.removed_edges.iter().map(|e| {
        warn(format!(
            "{} present before migration but missing after",
            edge(e)
        ))
    });
    let gained = delta.added_edges.iter().map(|e| {
        warn(format!(
            "{} appeared after migration but was not present before",
            edge(e)
        ))
    });
    removed
        .chain(added)
        .chain(modified)
        .chain(lost)
        .chain(gained)
        .collect()
}

/// What changed in `node`: its fields by name, then its aspects. A field named like an aspect (`kind`, `methods`, `obligations`)
/// is written `field 'kind'`, so the two read apart.
fn changed(node: &ModifiedNodeChange) -> String {
    let aspect = |a: &NodeAspect| match a {
        NodeAspect::Kind => "kind",
        NodeAspect::Title => "title",
        NodeAspect::Methods => "methods",
        NodeAspect::Obligations => "obligations",
        NodeAspect::Edges => "edges",
    };
    let aspects: Vec<&str> = node.changed_aspects.iter().map(aspect).collect();
    let fields = node.changed_fields.iter().map(|f| {
        if ["kind", "title", "methods", "obligations", "edges"].contains(&f.as_str()) {
            format!("field '{f}'")
        } else {
            f.clone()
        }
    });
    fields
        .chain(aspects.iter().map(|a| a.to_string()))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// Core Migration Logic
// ---------------------------------------------------------------------------

/// `path` from `root`, `/`-separated; `path` itself when it is not under `root`.
fn diff_label(path: &Path, root: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// What migrating one file did.
struct FileMigration {
    result: MigrationResult,
    backup: Option<MigrationBackup>,
    diff: Option<MigrationDiff>,
    /// The file as read, when this run rewrote it: what an automatic rollback restores.
    original: Option<Original>,
}

impl FileMigration {
    /// A file this run did not rewrite.
    fn of(
        result: MigrationResult,
        backup: Option<MigrationBackup>,
        diff: Option<MigrationDiff>,
    ) -> Self {
        Self {
            result,
            backup,
            diff,
            original: None,
        }
    }
}

/// Migrate one file of the project at `root`, reading it once: the version detected, the transform, the diff, the
/// backup and what a rollback restores are all that one text. The diff labels the file with its path from `root`,
/// so `patch -p1` applies it there.
fn migrate_file(
    path: &Path,
    root: &Path,
    target_version: &FormatVersion,
    dry_run: bool,
    no_backup: bool,
) -> FileMigration {
    let path_str = path.display().to_string();
    let label = diff_label(path, root);

    // Read file
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return FileMigration::of(
                MigrationResult {
                    file_path: path_str,
                    status: MigrationStatus::Failed,
                    from_version: None,
                    to_version: None,
                    error: Some(format!("failed to read file: {e}")),
                },
                None,
                None,
            );
        }
    };

    // Detect version; a header this build can't read fails the file.
    let (detected_version, diags) = detect_format_version(&content, &path_str);
    if let Some(error) = diags.iter().find(|d| d.severity == Severity::Error) {
        let guidance = error
            .suggestion
            .as_deref()
            .map(|s| format!(" — {s}"))
            .unwrap_or_default();
        return FileMigration::of(
            MigrationResult {
                file_path: path_str,
                status: MigrationStatus::Failed,
                from_version: Some(detected_version),
                to_version: None,
                error: Some(format!("{}: {}{guidance}", error.code, error.message)),
            },
            None,
            None,
        );
    }

    // Skip if already at target version
    if detected_version >= *target_version {
        return FileMigration::of(
            MigrationResult {
                file_path: path_str,
                status: MigrationStatus::Skipped,
                from_version: Some(detected_version.clone()),
                to_version: Some(detected_version),
                error: None,
            },
            None,
            None,
        );
    }

    // Transform
    let transformed = transform_content(&content, target_version);

    // Build diff
    let diff = if content != transformed {
        let diff_text = unified_diff(&format!("a/{label}"), &content, &transformed);
        // unified_diff uses the same path for both --- and +++.
        // We need +++ to use b/ prefix per POSIX convention.
        let unified_text = diff_text
            .diff_text
            .replace(&format!("+++ a/{label}"), &format!("+++ b/{label}"));
        Some(MigrationDiff {
            file_path: path_str.clone(),
            before_hash: sha256_hash(&content),
            after_hash: sha256_hash(&transformed),
            unified_text,
        })
    } else {
        None
    };

    if dry_run {
        return FileMigration::of(
            MigrationResult {
                file_path: path_str,
                status: MigrationStatus::Migrated,
                from_version: Some(detected_version),
                to_version: Some(target_version.clone()),
                error: None,
            },
            None,
            diff,
        );
    }

    // Create backup
    let backup = if !no_backup {
        let backup_path = path.with_extension("spec.bak");
        if let Err(e) = std::fs::write(&backup_path, &content) {
            return FileMigration::of(
                MigrationResult {
                    file_path: path_str,
                    status: MigrationStatus::Failed,
                    from_version: Some(detected_version),
                    to_version: None,
                    error: Some(format!("failed to create backup: {e}")),
                },
                None,
                None,
            );
        }
        Some(MigrationBackup {
            original_path: path_str.clone(),
            backup_path: backup_path.display().to_string(),
        })
    } else {
        None
    };

    // Atomic write: temp + rename
    let tmp_path = path.with_extension("spec.tmp");
    if let Err(e) = std::fs::write(&tmp_path, &transformed) {
        return FileMigration::of(
            MigrationResult {
                file_path: path_str,
                status: MigrationStatus::Failed,
                from_version: Some(detected_version),
                to_version: None,
                error: Some(format!("failed to write temp file: {e}")),
            },
            backup,
            None,
        );
    }

    if let Err(e) = std::fs::rename(&tmp_path, path) {
        // Clean up temp file on rename failure
        let _ = std::fs::remove_file(&tmp_path);
        return FileMigration::of(
            MigrationResult {
                file_path: path_str,
                status: MigrationStatus::Failed,
                from_version: Some(detected_version),
                to_version: None,
                error: Some(format!("failed to rename temp file: {e}")),
            },
            backup,
            None,
        );
    }

    FileMigration {
        result: MigrationResult {
            file_path: path_str,
            status: MigrationStatus::Migrated,
            from_version: Some(detected_version),
            to_version: Some(target_version.clone()),
            error: None,
        },
        backup,
        diff,
        original: Some(Original {
            path: path.to_path_buf(),
            text: content,
        }),
    }
}

/// Write `text` over `target` atomically (temp file, then rename).
fn write_atomically(target: &Path, text: &str) -> Result<(), String> {
    let tmp_path = target.with_extension("spec.restore.tmp");
    std::fs::write(&tmp_path, text).map_err(|e| format!("failed to write restore temp: {e}"))?;
    if let Err(e) = std::fs::rename(&tmp_path, target) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("failed to rename restore: {e}"));
    }
    Ok(())
}

fn rollback_result(
    file_path: String,
    status: MigrationStatus,
    error: Option<String>,
) -> MigrationResult {
    MigrationResult {
        file_path,
        status,
        from_version: None,
        to_version: None,
        error,
    }
}

/// Write each `original` back atomically (temp file, then rename): the automatic rollback of the run
/// that read them. Touches no backup and no other file.
pub fn restore(originals: &[Original]) -> RollbackSummary {
    let mut results = Vec::new();
    let (mut restored, mut failed) = (0, 0);
    for original in originals {
        let path = original.path.display().to_string();
        match write_atomically(&original.path, &original.text) {
            Ok(()) => {
                restored += 1;
                results.push(rollback_result(path, MigrationStatus::Restored, None));
            }
            Err(e) => {
                failed += 1;
                results.push(rollback_result(path, MigrationStatus::Failed, Some(e)));
            }
        }
    }
    RollbackSummary {
        restored_count: restored,
        skipped_count: 0,
        failed_count: failed,
        results,
        warnings: Vec::new(),
        record: RecordChange::Unchanged,
    }
}

/// Undo the recorded migration of the project at `root`. Each recorded file that still hashes as
/// the migration left it is restored from its backup, atomically. One whose backup is missing is
/// skipped with a warning naming the backup. One that changed since is skipped with a warning and
/// left as it is. Backups are never removed. The record then keeps only the files not restored, and
/// goes when none is left. With no record, nothing is restored and the one warning says so.
pub fn run_rollback(root: &ProjectRoot) -> RollbackSummary {
    let root = root.dir();
    let mut summary = RollbackSummary {
        restored_count: 0,
        skipped_count: 0,
        failed_count: 0,
        results: Vec::new(),
        warnings: Vec::new(),
        record: RecordChange::Unchanged,
    };
    let record = match MigrationRecord::read(root) {
        Ok(Some(record)) => record,
        Ok(None) => {
            summary.warnings.push(format!(
                "nothing to roll back: no migration is recorded ({})",
                MigrationRecord::PATH
            ));
            return summary;
        }
        Err(message) => {
            summary.failed_count = 1;
            summary.warnings.push(message);
            return summary;
        }
    };

    let mut remaining = Vec::new();
    for recorded in &record.files {
        let target = root.join(&recorded.path);
        let backup = root.join(&recorded.backup);
        let shown = target.display().to_string();

        if !backup.exists() {
            summary.skipped_count += 1;
            summary.warnings.push(format!(
                "no backup {} for {}; skipped",
                recorded.backup, recorded.path
            ));
            summary
                .results
                .push(rollback_result(shown, MigrationStatus::Skipped, None));
            remaining.push(recorded.clone());
            continue;
        }
        let current = std::fs::read_to_string(&target).map(|text| sha256_hash(&text));
        if current.as_deref().ok() != Some(recorded.sha256.as_str()) {
            summary.skipped_count += 1;
            summary.warnings.push(format!(
                "{} changed since the migration; left as it is (its backup is {})",
                recorded.path, recorded.backup
            ));
            summary
                .results
                .push(rollback_result(shown, MigrationStatus::Skipped, None));
            remaining.push(recorded.clone());
            continue;
        }
        let outcome = std::fs::read_to_string(&backup)
            .map_err(|e| format!("failed to read backup: {e}"))
            .and_then(|text| write_atomically(&target, &text));
        match outcome {
            Ok(()) => {
                summary.restored_count += 1;
                summary
                    .results
                    .push(rollback_result(shown, MigrationStatus::Restored, None));
            }
            Err(e) => {
                summary.failed_count += 1;
                summary
                    .results
                    .push(rollback_result(shown, MigrationStatus::Failed, Some(e)));
                remaining.push(recorded.clone());
            }
        }
    }

    if remaining.is_empty() {
        if MigrationRecord::remove(root).is_ok() {
            summary.record = RecordChange::Removed;
        }
    } else if summary.restored_count > 0 {
        let kept = MigrationRecord {
            target: record.target,
            files: remaining,
        };
        if kept.write(root).is_ok() {
            summary.record = RecordChange::Rewritten;
        }
    }
    summary
}

/// Migrate `sources`, files of the project at `root` (the project's
/// sources, which the caller selected), in order. `root` labels the diffs
/// and backups.
pub fn migrate_project(
    root: &ProjectRoot,
    sources: &[PathBuf],
    target_version: &FormatVersion,
    dry_run: bool,
    no_backup: bool,
) -> MigrationSummary {
    let project_root = root.dir();

    let mut results = Vec::new();
    let mut backups = Vec::new();
    let mut diffs = Vec::new();
    let mut originals = Vec::new();
    let mut migrated = 0;
    let mut skipped = 0;
    let mut failed = 0;

    for target_path in sources {
        let file = migrate_file(
            target_path,
            project_root,
            target_version,
            dry_run,
            no_backup,
        );

        match file.result.status {
            MigrationStatus::Migrated => migrated += 1,
            MigrationStatus::Skipped => skipped += 1,
            MigrationStatus::Failed => failed += 1,
            _ => {}
        }

        originals.extend(file.original);
        diffs.extend(file.diff);
        backups.extend(file.backup);
        results.push(file.result);
    }

    MigrationSummary {
        migrated_count: migrated,
        skipped_count: skipped,
        failed_count: failed,
        target_version: target_version.clone(),
        results,
        backups,
        diffs,
        originals,
    }
}
