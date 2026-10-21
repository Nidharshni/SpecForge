//! Format-version migration of a project's `.spec` files: what each file becomes at a target format version and the
//! migration steps (`titles`, `defaults`), the `.specforge/migration.json` record a rollback undoes, and W054 for
//! each thing the rewrite changed in the graph (`structural_differences`). It writes nothing: the caller writes the
//! plan, and hands `run_rollback` the reads and writes of a rollback (ADR 0171).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, ProjectPath, ProjectRoot, Severity, codes};
use specforge_formatter::unified_diff;
use specforge_graph::{EdgeChange, GraphDelta, ModifiedNodeChange, NodeAspect};
use specforge_parser::{FormatVersion, detect_format_version, with_format_version};
use std::path::{Path, PathBuf};

mod defaults;
mod titles;

// ---------------------------------------------------------------------------
// Migration Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Shape)]
#[shape(define)]
pub struct MigrationResult {
    /// The file, named from the project root (ADR 0102).
    pub file_path: ProjectPath,
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
    /// The file the backup holds the old text of, named from the project root.
    pub original_path: ProjectPath,
    /// The backup, named from the project root.
    pub backup_path: ProjectPath,
}

#[derive(Debug, Clone, Serialize, Shape)]
#[shape(define)]
pub struct MigrationDiff {
    /// The file, named from the project root.
    pub file_path: ProjectPath,
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
    /// Every file the plan rewrites, with the text it read and the text it becomes: what the caller judges and
    /// writes.
    #[serde(skip)]
    pub rewrites: Vec<Rewrite>,
}

/// A file the plan rewrites.
#[derive(Debug, Clone)]
pub struct Rewrite {
    pub path: PathBuf,
    /// The file as a document names it.
    pub name: ProjectPath,
    /// The text read.
    pub before: String,
    /// The text it becomes.
    pub after: String,
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

/// What a rollback decided `.specforge/migration.json` must become; the caller applies it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RecordChange {
    #[default]
    Unchanged,
    /// It becomes this one: only the files not restored.
    Rewritten(MigrationRecord),
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
    /// SHA-256 (hex) of the file as the migration left it.
    pub sha256: String,
}

impl MigrationRecord {
    /// `.specforge/migration.json`.
    pub const PATH: &'static str = ".specforge/migration.json";

    /// The record of the files `rewrites` rewrite: each file, its backup (`backup_of`) and the hash of the text it
    /// becomes. Hooks cannot write a file (ADR 0171), so that text is the one the plan holds.
    pub fn of(root: &Path, target: &FormatVersion, rewrites: &[Rewrite]) -> Self {
        Self {
            target: target.to_string(),
            files: rewrites
                .iter()
                .map(|rewrite| RecordedFile {
                    path: diff_label(&rewrite.path, root),
                    backup: diff_label(&backup_of(&rewrite.path), root),
                    sha256: sha256_hash(&rewrite.after),
                })
                .collect(),
        }
    }

    /// The record `text` holds. `Err` says it is not one.
    pub fn parse(text: &str) -> Result<Self, String> {
        serde_json::from_str(text)
            .map_err(|e| format!("{} is not a migration record: {e}", Self::PATH))
    }

    /// The record as its file holds it: pretty JSON and a newline.
    pub fn render(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("a migration record serializes");
        text.push('\n');
        text
    }
}

// ---------------------------------------------------------------------------
// Transform Functions (pure)
// ---------------------------------------------------------------------------

/// Transform content to version `to`: the header when the file is below it, then every migration step (titles, then
/// the defaults hoist). A step runs on
/// any file it changes, whatever the file's version (ADR 0148 D5).
fn transform_content(content: &str, from: &FormatVersion, to: &FormatVersion) -> String {
    if from < to {
        defaults::hoist_defaults(&titles::add_titles(&with_format_version(content, to)))
    } else {
        defaults::hoist_defaults(&titles::add_titles(content))
    }
}

/// `delta` without the changes the migration steps declare (`titles`: a title where the node had none), the
/// graph `before` the migration being the one to compare to. A migration keeps the graph only when this is empty.
pub fn beyond_steps(mut delta: GraphDelta, before: &specforge_graph::Graph) -> GraphDelta {
    delta
        .modified_nodes
        .retain(|change| !titles::added(change, before));
    delta
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

/// What the plan makes of one file.
struct FileMigration {
    result: MigrationResult,
    diff: Option<MigrationDiff>,
    /// The file as read and as it becomes, when the plan rewrites it.
    rewrite: Option<Rewrite>,
}

impl FileMigration {
    /// A file the plan does not rewrite.
    fn of(result: MigrationResult, diff: Option<MigrationDiff>) -> Self {
        Self {
            result,
            diff,
            rewrite: None,
        }
    }
}

/// Plan one file of the project at `root`, reading it once, through `read`: the version detected, the transform, the
/// diff and what the caller writes and puts back are all that one text. The diff labels the file with its path from
/// `root`, so `patch -p1` applies it there.
fn migrate_file(
    path: &Path,
    root: &Path,
    target_version: &FormatVersion,
    read: &dyn Fn(&Path) -> std::io::Result<String>,
) -> FileMigration {
    let name = ProjectPath::of(root, path);
    let label = diff_label(path, root);

    // Read file
    let content = match read(path) {
        Ok(c) => c,
        Err(e) => {
            return FileMigration::of(
                MigrationResult {
                    file_path: name,
                    status: MigrationStatus::Failed,
                    from_version: None,
                    to_version: None,
                    error: Some(format!("failed to read file: {e}")),
                },
                None,
            );
        }
    };

    // Detect version; a header this build can't read fails the file.
    let (detected_version, diags) = detect_format_version(&content, name.as_str());
    if let Some(error) = diags.iter().find(|d| d.severity == Severity::Error) {
        let guidance = error
            .suggestion
            .as_deref()
            .map(|s| format!(" — {s}"))
            .unwrap_or_default();
        return FileMigration::of(
            MigrationResult {
                file_path: name,
                status: MigrationStatus::Failed,
                from_version: Some(detected_version),
                to_version: None,
                error: Some(format!("{}: {}{guidance}", error.code, error.message)),
            },
            None,
        );
    }

    // Transform; a file the header and the steps leave as it is has nothing to migrate.
    let transformed = transform_content(&content, &detected_version, target_version);
    if transformed == content {
        return FileMigration::of(
            MigrationResult {
                file_path: name,
                status: MigrationStatus::Skipped,
                from_version: Some(detected_version.clone()),
                to_version: Some(detected_version),
                error: None,
            },
            None,
        );
    }

    let diff_text = unified_diff(&format!("a/{label}"), &content, &transformed);
    // unified_diff uses the same path for both --- and +++.
    // We need +++ to use b/ prefix per POSIX convention.
    let unified_text = diff_text
        .diff_text
        .replace(&format!("+++ a/{label}"), &format!("+++ b/{label}"));
    let diff = MigrationDiff {
        file_path: name.clone(),
        before_hash: sha256_hash(&content),
        after_hash: sha256_hash(&transformed),
        unified_text,
    };
    FileMigration {
        result: MigrationResult {
            file_path: name.clone(),
            status: MigrationStatus::Migrated,
            from_version: Some(detected_version),
            to_version: Some(target_version.clone()),
            error: None,
        },
        diff: Some(diff),
        rewrite: Some(Rewrite {
            path: path.to_path_buf(),
            name,
            before: content,
            after: transformed,
        }),
    }
}

/// The backup of `source`, beside it (`.spec.bak`): the one place the name is made.
pub fn backup_of(source: &Path) -> PathBuf {
    source.with_extension("spec.bak")
}

fn rollback_result(
    file_path: ProjectPath,
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

/// Undo the recorded migration of the project at `root` (`record`: the project's record, `None` when there is none,
/// `Err` the reason it can't be read). Each recorded file that still hashes as the migration left it is restored
/// from its backup. One whose backup is missing is skipped with a warning naming the backup. One that changed
/// since is skipped with a warning and left as it is. Backups are never removed. `summary.record` says what the
/// record must become: it keeps only the files not restored, and goes when none is left. With no record, nothing
/// is restored and the one warning says so. Every file is read through `read` and written through `write`; this
/// function opens none (ops hands it the caller's containment, ADR 0158).
pub fn run_rollback(
    root: &ProjectRoot,
    record: Result<Option<MigrationRecord>, String>,
    read: &dyn Fn(&Path) -> std::io::Result<String>,
    write: &dyn Fn(&Path, &str) -> Result<(), String>,
) -> RollbackSummary {
    let root = root.dir();
    let mut summary = RollbackSummary {
        restored_count: 0,
        skipped_count: 0,
        failed_count: 0,
        results: Vec::new(),
        warnings: Vec::new(),
        record: RecordChange::Unchanged,
    };
    let record = match record {
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
        let shown = ProjectPath::of(root, &target);

        let text = match read(&backup) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
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
            Err(e) => {
                summary.failed_count += 1;
                summary.results.push(rollback_result(
                    shown,
                    MigrationStatus::Failed,
                    Some(format!("failed to read backup: {e}")),
                ));
                remaining.push(recorded.clone());
                continue;
            }
        };
        let current = read(&target).map(|text| sha256_hash(&text));
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
        match write(&target, &text) {
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
        summary.record = RecordChange::Removed;
    } else if summary.restored_count > 0 {
        summary.record = RecordChange::Rewritten(MigrationRecord {
            target: record.target,
            files: remaining,
        });
    }
    summary
}

/// Plan the migration of `sources`, files of the project at `root` (the project's sources, which the caller
/// selected), in order. `root` labels the diffs. Each file is read once, through `read`: the caller decides the
/// disk (ops reads through the request's containment, ADR 0158). Nothing is written.
pub fn migrate_project(
    root: &ProjectRoot,
    sources: &[PathBuf],
    target_version: &FormatVersion,
    read: &dyn Fn(&Path) -> std::io::Result<String>,
) -> MigrationSummary {
    let project_root = root.dir();

    let mut results = Vec::new();
    let mut diffs = Vec::new();
    let mut rewrites = Vec::new();
    let mut migrated = 0;
    let mut skipped = 0;
    let mut failed = 0;

    for target_path in sources {
        let file = migrate_file(target_path, project_root, target_version, read);

        match file.result.status {
            MigrationStatus::Migrated => migrated += 1,
            MigrationStatus::Skipped => skipped += 1,
            MigrationStatus::Failed => failed += 1,
            _ => {}
        }

        rewrites.extend(file.rewrite);
        diffs.extend(file.diff);
        results.push(file.result);
    }

    MigrationSummary {
        migrated_count: migrated,
        skipped_count: skipped,
        failed_count: failed,
        target_version: target_version.clone(),
        results,
        backups: Vec::new(),
        diffs,
        rewrites,
    }
}
