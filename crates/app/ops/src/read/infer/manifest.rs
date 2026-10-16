//! The inference manifest: `<root>/specforge-infer.json`, the source files
//! an agent analyzed and the inference sessions it ran. Every reader gets
//! it from one reader ([`InferenceManifest::read`]) and every write goes
//! through one writer ([`InferenceManifest::write`]); a key this version
//! does not define, at any level, is kept as read and written back.

use std::collections::HashSet;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use specforge_common::{Diagnostic, codes};

use crate::{Containment, Held, OpError, OpErrorKind, Outside, ReadFailure, Writes};

const CURRENT_VERSION: u32 = 1;
/// The inference manifest, at the project root.
pub const MANIFEST_FILENAME: &str = "specforge-infer.json";

/// `specforge-infer.json` could not be written.
pub const MANIFEST_WRITE_FAILED: &str = "infer_manifest_write_failed";

/// `<root>/specforge-infer.json`: the source files an agent analyzed and
/// the inference sessions it ran. A key this version does not define, at
/// any level, is kept as read and written back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceManifest {
    pub version: u32,
    pub source_roots: Vec<String>,
    /// Sorted by path ([`Self::upsert_source_entry`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_index: Vec<SourceFileEntry>,
    /// In the order they were started. At most one is `Active`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<InferenceSession>,
    /// Keys this version does not define, written back as read.
    #[serde(flatten)]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

/// One analyzed source file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceFileEntry {
    pub path: String,
    /// SHA-256 of the file's bytes, lowercase hex.
    pub content_hash: String,
    pub entities_produced: Vec<String>,
    /// RFC 3339, UTC, whole seconds.
    pub analyzed_at: String,
    /// Keys this version does not define, written back as read.
    #[serde(flatten)]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl SourceFileEntry {
    /// An entry with no keys beyond the ones this version defines.
    pub fn new(
        path: impl Into<String>,
        content_hash: impl Into<String>,
        entities_produced: Vec<String>,
        analyzed_at: impl Into<String>,
    ) -> Self {
        SourceFileEntry {
            path: path.into(),
            content_hash: content_hash.into(),
            entities_produced,
            analyzed_at: analyzed_at.into(),
            unknown: serde_json::Map::new(),
        }
    }
}

/// One inference session an agent ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceSession {
    /// A random (version 4) UUID.
    pub session_id: String,
    /// RFC 3339, UTC, whole seconds.
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    pub agent: String,
    pub status: SessionStatus,
    /// Keys this version does not define, written back as read.
    #[serde(flatten)]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

/// Where a session stands (the spec's `SessionStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Paused,
    Completed,
}

impl SessionStatus {
    /// `active`, `paused`, `completed`: as the file and every reply spell
    /// it.
    pub fn name(self) -> &'static str {
        match self {
            SessionStatus::Active => "active",
            SessionStatus::Paused => "paused",
            SessionStatus::Completed => "completed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InferenceSummary {
    pub files_total: usize,
    pub files_analyzed: usize,
    pub entities_produced: usize,
}

impl Default for InferenceManifest {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            source_roots: Vec::new(),
            source_index: Vec::new(),
            sessions: Vec::new(),
            unknown: serde_json::Map::new(),
        }
    }
}

/// Why a manifest inference keeps at the project root
/// (`specforge-infer.json`, `specforge-anchors.json`) cannot be used.
#[derive(Debug)]
pub(crate) struct ManifestProblem {
    file: &'static str,
    why: Why,
}

#[derive(Debug)]
enum Why {
    /// The file lands outside the project: it is not read (ADR 0112 D3).
    Outside(Outside),
    Unreadable(std::io::Error),
    Invalid(serde_json::Error),
    UnsupportedVersion(u32),
}

impl ManifestProblem {
    /// What is wrong, naming the file: `failed to read {file}: {e}`,
    /// `failed to parse {file}: {e}` (serde_json's message names line and
    /// column), `unsupported {file} version: {n} (expected 1)`, `{file} resolves to {landed},
    /// outside {root}: it is not read`.
    pub(crate) fn message(&self) -> String {
        let file = self.file;
        match &self.why {
            Why::Outside(outside) => format!(
                "{file} resolves to {}, outside {}: it is not read",
                outside.landed().display(),
                outside.root().display()
            ),
            Why::Unreadable(e) => format!("failed to read {file}: {e}"),
            Why::Invalid(e) => format!("failed to parse {file}: {e}"),
            Why::UnsupportedVersion(n) => {
                format!("unsupported {file} version: {n} (expected {CURRENT_VERSION})")
            }
        }
    }
}

impl ManifestProblem {
    /// What to do about it.
    fn suggestion(&self) -> String {
        format!(
            "fix {} where the message says, or move it aside to start over",
            self.file
        )
    }

    /// The problem as the diagnostic the `inferred` lint reports.
    pub(crate) fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(codes::E071, self.message()).with_suggestion(self.suggestion())
    }
}

/// E071, of the kind of the failure: `OpErrorKind::of_io` when the file
/// could not be read, `SchemaMismatch` when it was read and is not usable.
impl From<ManifestProblem> for OpError {
    fn from(problem: ManifestProblem) -> OpError {
        let kind = match &problem.why {
            Why::Outside(_) => OpErrorKind::InvalidInput,
            Why::Unreadable(e) => OpErrorKind::of_io(e),
            Why::Invalid(_) | Why::UnsupportedVersion(_) => OpErrorKind::SchemaMismatch,
        };
        let suggestion = problem.suggestion();
        OpError::coded(kind, codes::E071, problem.message()).with_suggestion(suggestion)
    }
}

/// The one reader of a JSON manifest at the project root: `None` when the
/// file is absent, the typed value, or the problem. The file is held by the
/// project's own containment on every surface (a manifest only the bounded
/// `infer_session` writes is never one the project wrote when it leaves the
/// project: ADR 0112 D3), read once and parsed once.
pub(crate) fn read_manifest<T: DeserializeOwned>(
    root: &Path,
    file: &'static str,
) -> Result<Option<T>, ManifestProblem> {
    match Containment::within(root).read(Path::new(file)) {
        Ok((_, bytes)) => parse_manifest(file, &bytes),
        Err(ReadFailure::Outside(outside)) => Err(ManifestProblem {
            file,
            why: Why::Outside(outside),
        }),
        Err(ReadFailure::Io { error, .. }) => unreadable(file, error),
    }
}

/// [`read_manifest`] over a file already held.
fn read_manifest_held<T: DeserializeOwned>(
    held: &Held,
    file: &'static str,
) -> Result<Option<T>, ManifestProblem> {
    match held.read() {
        Ok(bytes) => parse_manifest(file, &bytes),
        Err(error) => unreadable(file, error),
    }
}

fn unreadable<T>(file: &'static str, error: std::io::Error) -> Result<Option<T>, ManifestProblem> {
    if error.kind() == std::io::ErrorKind::NotFound {
        return Ok(None);
    }
    Err(ManifestProblem {
        file,
        why: Why::Unreadable(error),
    })
}

fn parse_manifest<T: DeserializeOwned>(
    file: &'static str,
    bytes: &[u8],
) -> Result<Option<T>, ManifestProblem> {
    serde_json::from_slice(bytes)
        .map(Some)
        .map_err(|e| ManifestProblem {
            file,
            why: Why::Invalid(e),
        })
}

impl InferenceManifest {
    /// The manifest at `root`. `Ok(None)` when there is none; the problem
    /// when the file is there and cannot be used: unreadable, not JSON, not
    /// this shape (a session's status included), or another `version`.
    /// Never an empty manifest in place of one that did not read.
    pub(crate) fn read(root: &Path) -> Result<Option<Self>, ManifestProblem> {
        Self::versioned(read_manifest::<Self>(root, MANIFEST_FILENAME)?)
    }

    /// [`Self::read`] for the file a containment already held (the session's).
    pub(crate) fn read_held(held: &Held) -> Result<Option<Self>, ManifestProblem> {
        Self::versioned(read_manifest_held::<Self>(held, MANIFEST_FILENAME)?)
    }

    fn versioned(manifest: Option<Self>) -> Result<Option<Self>, ManifestProblem> {
        match manifest {
            Some(manifest) if manifest.version != CURRENT_VERSION => Err(ManifestProblem {
                file: MANIFEST_FILENAME,
                why: Why::UnsupportedVersion(manifest.version),
            }),
            other => Ok(other),
        }
    }

    /// [`Self::read`] for an operation: an empty manifest when there is
    /// none, the problem as an `OpError`.
    pub(crate) fn at(root: &Path) -> Result<Self, OpError> {
        Ok(Self::read(root)?.unwrap_or_default())
    }

    /// [`Self::at`] for the file a containment already held.
    pub(crate) fn at_held(held: &Held) -> Result<Self, OpError> {
        Ok(Self::read_held(held)?.unwrap_or_default())
    }

    /// Replace `file` with this manifest: pretty-printed JSON,
    /// keys sorted, written to `specforge-infer.json.tmp`, synced, then
    /// renamed (`Held::replace`: the temporary file is created new, so
    /// whatever is at its name is removed first and nothing is written
    /// through it). Returns the file it wrote. A failure removes the temporary
    /// file, leaves the old manifest and is `infer_manifest_write_failed`.
    pub(crate) fn write(&self, file: &Held) -> Result<Writes, OpError> {
        // `to_value` orders the keys (a `BTreeMap`), as the spec's
        // `json_formatted` promises.
        let text = serde_json::to_value(self)
            .and_then(|value| serde_json::to_string_pretty(&value))
            .map_err(|e| {
                OpError::new(
                    OpErrorKind::Internal,
                    MANIFEST_WRITE_FAILED,
                    format!("failed to serialize {MANIFEST_FILENAME}: {e}"),
                )
            })?;
        if let Err(e) = file.replace(text) {
            return Err(OpError::new(
                OpErrorKind::of_io(&e),
                MANIFEST_WRITE_FAILED,
                format!("failed to write {MANIFEST_FILENAME}: {e}"),
            ));
        }
        Ok(Writes::from_iter([file.spelled.clone()]))
    }

    /// The session in progress, if any.
    pub fn active_session(&self) -> Option<&InferenceSession> {
        self.sessions
            .iter()
            .find(|s| s.status == SessionStatus::Active)
    }

    /// The paths the index records, by the source path rule (`./src/a.rs`,
    /// as an older version recorded it, is `src/a.rs`).
    pub(crate) fn indexed_paths(&self) -> HashSet<String> {
        self.source_index
            .iter()
            .map(|entry| normal_path(&entry.path))
            .collect()
    }

    /// Whether `path` is indexed, by the source path rule.
    #[cfg(test)]
    pub(crate) fn indexes(&self, path: &str) -> bool {
        self.indexed_paths().contains(&normal_path(path))
    }

    /// Record `entry`, replacing the entry for the same path (by the
    /// source path rule); the index stays sorted by path.
    pub fn upsert_source_entry(&mut self, entry: SourceFileEntry) {
        let path = normal_path(&entry.path);
        if let Some(existing) = self
            .source_index
            .iter_mut()
            .find(|e| normal_path(&e.path) == path)
        {
            *existing = entry;
        } else {
            self.source_index.push(entry);
        }
        self.source_index.sort_by(|a, b| a.path.cmp(&b.path));
    }

    pub fn compute_summary(&self, total_source_files: usize) -> InferenceSummary {
        let entities_produced = self
            .source_index
            .iter()
            .map(|e| e.entities_produced.len())
            .sum();

        InferenceSummary {
            files_total: total_source_files,
            files_analyzed: self.source_index.len(),
            entities_produced,
        }
    }
}

/// A path outside the project root, or naming nothing: the source path
/// rule refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OutsideRoot;

/// The components of `query` under the source path rule's separators (`/`
/// and `\`), without the empty and `.` ones. A path that is absolute, has
/// a drive prefix or has a `..` component is outside the root.
fn components(query: &str) -> Result<Vec<&str>, OutsideRoot> {
    let bytes = query.as_bytes();
    let absolute = query.starts_with(['/', '\\']);
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if absolute || drive {
        return Err(OutsideRoot);
    }
    let parts: Vec<&str> = query
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.contains(&"..") {
        return Err(OutsideRoot);
    }
    Ok(parts)
}

/// The source path rule: `query` as the root-relative, `/`-separated path
/// the manifest records. Separators `/` and `\` are both accepted; empty
/// and `.` components are dropped. A path that is absolute, has a drive
/// prefix, has a `..` component, or names nothing is outside the root.
pub(crate) fn source_path(query: &str) -> Result<String, OutsideRoot> {
    let parts = components(query)?;
    if parts.is_empty() {
        return Err(OutsideRoot);
    }
    Ok(parts.join("/"))
}

/// [`source_path`] for a source root: the project root itself (`.`) is a
/// root, naming nothing at all is not.
pub(crate) fn source_root(query: &str) -> Result<String, OutsideRoot> {
    let parts = components(query)?;
    if parts.is_empty() {
        return if query.is_empty() {
            Err(OutsideRoot)
        } else {
            Ok(".".to_string())
        };
    }
    Ok(parts.join("/"))
}

/// A recorded path as the rule spells it, leniently: an entry an older
/// version recorded as the agent spelled it (`./src/a.rs`) compares as
/// the path it names.
fn normal_path(path: &str) -> String {
    components(path).map_or_else(|_| path.to_string(), |parts| parts.join("/"))
}

/// The SHA-256 of the file's bytes, lowercase hex.
pub(crate) fn compute_content_hash(file_path: &Path) -> std::io::Result<String> {
    let content = std::fs::read(file_path)?;
    let hash = Sha256::digest(&content);
    Ok(format!("{:x}", hash))
}

pub fn detect_stale_entries(
    project_root: &Path,
    manifest: &InferenceManifest,
) -> (Vec<String>, Vec<String>) {
    let mut stale = Vec::new();
    let mut deleted = Vec::new();

    for entry in &manifest.source_index {
        let abs_path = project_root.join(&entry.path);
        if !abs_path.exists() {
            deleted.push(entry.path.clone());
            continue;
        }
        match compute_content_hash(&abs_path) {
            Ok(hash) if hash != entry.content_hash => {
                stale.push(entry.path.clone());
            }
            _ => {}
        }
    }

    (stale, deleted)
}

#[cfg(test)]
mod tests;
