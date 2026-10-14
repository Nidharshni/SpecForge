//! The inference manifest: `<root>/specforge-infer.json`, the source files
//! an agent analyzed and the inference sessions it ran. Every reader gets
//! it from one reader ([`InferenceManifest::read`]) and every write goes
//! through one writer ([`InferenceManifest::write`]); a key this version
//! does not define, at any level, is kept as read and written back.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use specforge_common::{Diagnostic, codes};

use crate::{OpError, OpErrorKind, Writes};

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
    Unreadable(std::io::Error),
    Invalid(serde_json::Error),
    UnsupportedVersion(u32),
}

impl ManifestProblem {
    /// What is wrong, naming the file: `failed to read {file}: {e}`,
    /// `failed to parse {file}: {e}` (serde_json's message names line and
    /// column), `unsupported {file} version: {n} (expected 1)`.
    pub(crate) fn message(&self) -> String {
        let file = self.file;
        match &self.why {
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
            Why::Unreadable(e) => OpErrorKind::of_io(e),
            Why::Invalid(_) | Why::UnsupportedVersion(_) => OpErrorKind::SchemaMismatch,
        };
        let suggestion = problem.suggestion();
        OpError::coded(kind, codes::E071, problem.message()).with_suggestion(suggestion)
    }
}

/// The one reader of a JSON manifest at the project root: `None` when the
/// file is absent, the typed value, or the problem. The file is read once
/// and parsed once.
pub(crate) fn read_manifest<T: DeserializeOwned>(
    root: &Path,
    file: &'static str,
) -> Result<Option<T>, ManifestProblem> {
    let text = match fs::read_to_string(root.join(file)) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(ManifestProblem {
                file,
                why: Why::Unreadable(e),
            });
        }
    };
    serde_json::from_str(&text)
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
        let manifest = read_manifest::<Self>(root, MANIFEST_FILENAME)?;
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

    /// Replace the file at `root` with this manifest: pretty-printed JSON,
    /// keys sorted, written to `specforge-infer.json.tmp`, synced, then
    /// renamed. Returns the file it wrote. A failure removes the temporary
    /// file, leaves the old manifest and is `infer_manifest_write_failed`.
    pub(crate) fn write(&self, root: &Path) -> Result<Writes, OpError> {
        let path = root.join(MANIFEST_FILENAME);
        let tmp = path.with_extension("json.tmp");
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
        let written = fs::File::create(&tmp).and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        });
        let result = written.and_then(|()| fs::rename(&tmp, &path));
        if let Err(e) = result {
            let _ = fs::remove_file(&tmp);
            return Err(OpError::new(
                OpErrorKind::of_io(&e),
                MANIFEST_WRITE_FAILED,
                format!("failed to write {MANIFEST_FILENAME}: {e}"),
            ));
        }
        Ok(Writes::from_iter([path]))
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
    let content = fs::read(file_path)?;
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
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;
    use tempfile::TempDir;

    fn entry(path: &str, entities: &[&str]) -> SourceFileEntry {
        SourceFileEntry::new(
            path,
            "h",
            entities.iter().map(|e| e.to_string()).collect(),
            "t",
        )
    }

    fn session(id: &str, status: SessionStatus) -> InferenceSession {
        InferenceSession {
            session_id: id.into(),
            started_at: "2026-10-01T00:00:00Z".into(),
            ended_at: None,
            agent: "claude".into(),
            status,
            unknown: serde_json::Map::new(),
        }
    }

    #[test]
    fn default_manifest_has_current_version() {
        let m = InferenceManifest::default();
        assert_eq!(m.version, CURRENT_VERSION);
        assert!(m.source_roots.is_empty());
        assert!(m.source_index.is_empty());
        assert!(m.sessions.is_empty());
    }

    #[test]
    fn round_trip_serialization() {
        let mut m = InferenceManifest {
            source_roots: vec!["src/".to_string()],
            ..Default::default()
        };
        m.upsert_source_entry(SourceFileEntry::new(
            "src/main.rs",
            "abc123",
            vec!["my_behavior".to_string()],
            "2026-04-24T10:00:00Z",
        ));

        let json = serde_json::to_string_pretty(&m).unwrap();
        let loaded: InferenceManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded, m);
        assert_eq!(loaded.version, CURRENT_VERSION);
        assert_eq!(loaded.source_roots, vec!["src/"]);
        assert_eq!(
            loaded.source_index[0].entities_produced,
            vec!["my_behavior"]
        );
    }

    #[specforge_test(
        type = "InferenceManifest",
        verify = "InferenceManifest round-trips through JSON serialization"
    )]
    fn sessions_round_trip_with_their_status() {
        let mut m = InferenceManifest::default();
        m.sessions.push(session("s-1", SessionStatus::Completed));
        m.sessions.push(session("s-2", SessionStatus::Active));
        m.sessions[0].ended_at = Some("2026-10-01T01:00:00Z".into());

        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains(r#""status":"completed""#), "{json}");
        let loaded: InferenceManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, m);
        assert_eq!(loaded.active_session().unwrap().session_id, "s-2");
    }

    #[test]
    fn a_status_outside_the_three_is_refused() {
        let dir = TempDir::new().unwrap();
        let text = r#"{
  "version": 1,
  "source_roots": [],
  "sessions": [
    {"session_id": "s", "started_at": "t", "agent": "a", "status": "Completed"}
  ]
}"#;
        fs::write(dir.path().join(MANIFEST_FILENAME), text).unwrap();
        let problem = InferenceManifest::read(dir.path()).unwrap_err();
        let message = problem.message();
        assert!(
            message
                .starts_with("failed to parse specforge-infer.json: unknown variant `Completed`"),
            "{message}"
        );
        assert!(message.contains("line 5 column"), "{message}");
    }

    #[specforge_test(
        behavior = "load_inference_manifest",
        verify = "load returns default manifest when file is missing"
    )]
    fn read_of_an_absent_file_is_none() {
        let dir = TempDir::new().unwrap();
        assert_eq!(InferenceManifest::read(dir.path()).unwrap(), None);
        let m = InferenceManifest::at(dir.path()).unwrap();
        assert_eq!(m.version, CURRENT_VERSION);
        assert!(m.source_index.is_empty());
    }

    #[specforge_test(
        behavior = "load_inference_manifest",
        verify = "load rejects unsupported version"
    )]
    fn read_rejects_unsupported_version() {
        let dir = TempDir::new().unwrap();
        let content = r#"{"version": 999, "source_roots": [], "source_index": []}"#;
        fs::write(dir.path().join(MANIFEST_FILENAME), content).unwrap();

        let message = InferenceManifest::read(dir.path()).unwrap_err().message();
        assert!(message.contains("unsupported"), "{message}");
    }

    #[test]
    fn write_and_read_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut m = InferenceManifest {
            source_roots: vec!["crates/my-crate/src".to_string()],
            ..Default::default()
        };
        m.upsert_source_entry(entry("crates/my-crate/src/lib.rs", &["a", "b"]));

        let writes = m.write(dir.path()).unwrap();
        assert_eq!(writes.names_under(dir.path()), ["specforge-infer.json"]);
        let loaded = InferenceManifest::read(dir.path()).unwrap().unwrap();
        assert_eq!(loaded, m);
    }

    #[specforge_test(
        behavior = "save_inference_manifest",
        verify = "save uses atomic write (temp file + rename)"
    )]
    fn write_sorts_keys_and_leaves_no_temporary_file() {
        let dir = TempDir::new().unwrap();
        let mut m = InferenceManifest::default();
        m.sessions.push(session("s-1", SessionStatus::Paused));
        m.unknown.insert("notes".into(), "kept".into());
        m.write(dir.path()).unwrap();

        let text = fs::read_to_string(dir.path().join(MANIFEST_FILENAME)).unwrap();
        let keys = ["notes", "sessions", "source_roots", "version"];
        let positions: Vec<usize> = keys
            .iter()
            .map(|k| text.find(&format!("\"{k}\"")).unwrap())
            .collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{text}");
        assert!(!dir.path().join("specforge-infer.json.tmp").exists());
    }

    #[specforge_test(
        behavior = "save_inference_manifest",
        verify = "save keeps keys the manifest does not define, at every level"
    )]
    fn a_rewrite_keeps_the_keys_it_does_not_define() {
        let dir = TempDir::new().unwrap();
        let text = r#"{
  "version": 1,
  "source_roots": ["src"],
  "notes": "by hand",
  "source_index": [
    {"path": "a.rs", "content_hash": "h", "entities_produced": [], "analyzed_at": "t", "note": "x"}
  ],
  "sessions": [
    {"session_id": "s", "started_at": "t", "agent": "a", "status": "active", "model": "m"}
  ]
}"#;
        fs::write(dir.path().join(MANIFEST_FILENAME), text).unwrap();
        let mut m = InferenceManifest::at(dir.path()).unwrap();
        m.sessions[0].status = SessionStatus::Completed;
        m.write(dir.path()).unwrap();

        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.path().join(MANIFEST_FILENAME)).unwrap())
                .unwrap();
        assert_eq!(value["notes"], "by hand");
        assert_eq!(value["source_index"][0]["note"], "x");
        assert_eq!(value["sessions"][0]["model"], "m");
        assert_eq!(value["sessions"][0]["status"], "completed");
    }

    #[test]
    fn a_failed_write_leaves_the_old_manifest_and_no_temporary_file() {
        let dir = TempDir::new().unwrap();
        // A directory where the manifest goes: the rename fails.
        fs::create_dir(dir.path().join(MANIFEST_FILENAME)).unwrap();
        let error = InferenceManifest::default().write(dir.path()).unwrap_err();
        assert_eq!(error.code, MANIFEST_WRITE_FAILED);
        assert!(!dir.path().join("specforge-infer.json.tmp").exists());
    }

    #[test]
    fn source_index_sorted_after_upsert() {
        let mut m = InferenceManifest::default();
        m.upsert_source_entry(entry("z.rs", &[]));
        m.upsert_source_entry(entry("a.rs", &[]));
        m.upsert_source_entry(entry("m.rs", &[]));

        let paths: Vec<&str> = m.source_index.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["a.rs", "m.rs", "z.rs"]);
    }

    #[test]
    fn upsert_replaces_existing_entry() {
        let mut m = InferenceManifest::default();
        m.upsert_source_entry(SourceFileEntry::new(
            "src/lib.rs",
            "old",
            vec!["a".to_string()],
            "t1",
        ));
        m.upsert_source_entry(SourceFileEntry::new(
            "src/lib.rs",
            "new",
            vec!["a".to_string(), "b".to_string()],
            "t2",
        ));

        assert_eq!(m.source_index.len(), 1);
        assert_eq!(m.source_index[0].content_hash, "new");
        assert_eq!(m.source_index[0].entities_produced.len(), 2);
    }

    #[test]
    fn compute_summary_counts() {
        let mut m = InferenceManifest::default();
        m.upsert_source_entry(entry("a.rs", &["e1", "e2"]));
        m.upsert_source_entry(entry("b.rs", &["e3"]));

        let summary = m.compute_summary(10);
        assert_eq!(summary.files_total, 10);
        assert_eq!(summary.files_analyzed, 2);
        assert_eq!(summary.entities_produced, 3);
    }

    #[test]
    fn content_hash_is_sha256() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.rs");
        fs::write(&file, "fn main() {}").unwrap();

        let hash = compute_content_hash(&file).unwrap();
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn detect_stale_and_deleted() {
        let dir = TempDir::new().unwrap();
        let file_a = dir.path().join("a.rs");
        fs::write(&file_a, "original").unwrap();
        let hash_a = compute_content_hash(&file_a).unwrap();

        let mut m = InferenceManifest::default();
        m.upsert_source_entry(SourceFileEntry::new("a.rs", hash_a, vec![], "t"));
        m.upsert_source_entry(SourceFileEntry::new("deleted.rs", "whatever", vec![], "t"));

        let (stale, deleted) = detect_stale_entries(dir.path(), &m);
        assert!(stale.is_empty());
        assert_eq!(deleted, vec!["deleted.rs"]);

        fs::write(&file_a, "modified").unwrap();
        let (stale, deleted) = detect_stale_entries(dir.path(), &m);
        assert_eq!(stale, vec!["a.rs"]);
        assert_eq!(deleted, vec!["deleted.rs"]);
    }

    #[test]
    fn the_source_path_rule() {
        for (given, recorded) in [
            ("a.rs", "a.rs"),
            ("./a.rs", "a.rs"),
            ("src\\a.rs", "src/a.rs"),
            ("src//a.rs", "src/a.rs"),
            ("src/./a.rs", "src/a.rs"),
            ("src/", "src"),
        ] {
            assert_eq!(source_path(given), Ok(recorded.to_string()), "{given}");
        }
        for outside in [
            "/x", "\\x", "C:\\x", "c:/x", "../x", "a/../b", "", ".", "./",
        ] {
            assert_eq!(source_path(outside), Err(OutsideRoot), "{outside:?}");
        }
        // A source root may be the project root itself.
        assert_eq!(source_root("."), Ok(".".to_string()));
        assert_eq!(source_root("./src"), Ok("src".to_string()));
        for outside in ["", "/x", "../x"] {
            assert_eq!(source_root(outside), Err(OutsideRoot), "{outside:?}");
        }
    }

    #[test]
    fn an_entry_recorded_with_another_spelling_is_the_same_path() {
        let mut m = InferenceManifest::default();
        m.upsert_source_entry(entry("./src/lib.rs", &["a"]));
        assert!(m.indexes("src/lib.rs"));
        assert!(m.indexes("./src/lib.rs"));
        assert!(!m.indexes("src/main.rs"));
        // Marking it again replaces the entry, whatever the old spelling.
        m.upsert_source_entry(entry("src/lib.rs", &["a", "b"]));
        assert_eq!(m.source_index.len(), 1);
        assert_eq!(m.source_index[0].path, "src/lib.rs");
    }

    #[test]
    fn empty_source_index_not_serialized() {
        let m = InferenceManifest::default();
        let json = serde_json::to_string(&m).unwrap();
        assert!(!json.contains("source_index"));
        assert!(!json.contains("sessions"));
    }
}
