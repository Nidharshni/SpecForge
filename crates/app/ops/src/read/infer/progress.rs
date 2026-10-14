//! Inference progress: the source files the enabled analyzers would scan
//! against `specforge-infer.json`'s index.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use specforge_common::shape::Shape;
use specforge_protocol_types::ExtensionDeclaration;

use super::discovery::source_files;
use super::gaps::directory_of;
use super::manifest::{
    InferenceManifest, InferenceSession, InferenceSummary, detect_stale_entries,
};
use crate::OpError;
use crate::view::ProjectView;

/// The unanalyzed files of one directory (`specforge infer-status --gaps`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct UnanalyzedDirectory {
    pub directory: String,
    pub count: usize,
    pub files: Vec<String>,
}

/// How far inference has come: every source file the analyzers would
/// scan, against the manifest's index.
#[derive(Debug, Clone)]
pub struct Progress {
    pub summary: InferenceSummary,
    /// Source files the manifest has not indexed.
    pub unanalyzed: Vec<String>,
    /// Indexed files whose content changed since they were analyzed.
    pub stale: Vec<String>,
    /// Indexed files no longer on disk.
    pub deleted: Vec<String>,
    /// The sessions the manifest records, in order.
    pub sessions: Vec<InferenceSession>,
    /// Whether `specforge-infer.json` exists (false: nothing recorded yet).
    pub recorded: bool,
}

/// Inference progress for the project the view was compiled from: the
/// source files its loaded analyzers would scan at its root, against
/// `specforge-infer.json` there. Without a root: `no_project`.
pub fn progress(view: &ProjectView) -> Result<Progress, OpError> {
    let root = view.project_root()?;
    let manifest = InferenceManifest::read(root)?;
    let recorded = manifest.is_some();
    Ok(progress_under(
        root,
        view.registries().declarations(),
        &manifest.unwrap_or_default(),
        recorded,
    ))
}

fn progress_under(
    root: &Path,
    declarations: &[ExtensionDeclaration],
    manifest: &InferenceManifest,
    recorded: bool,
) -> Progress {
    let files = source_files(root, declarations, manifest);
    let indexed = manifest.indexed_paths();
    let unanalyzed = files
        .iter()
        .filter(|f| !indexed.contains(f.as_str()))
        .cloned()
        .collect();
    let (stale, deleted) = detect_stale_entries(root, manifest);
    Progress {
        summary: manifest.compute_summary(files.len()),
        unanalyzed,
        stale,
        deleted,
        sessions: manifest.sessions.clone(),
        recorded,
    }
}

impl Progress {
    /// No project: nothing discovered, nothing recorded.
    pub fn none() -> Self {
        Progress {
            summary: InferenceManifest::default().compute_summary(0),
            unanalyzed: Vec::new(),
            stale: Vec::new(),
            deleted: Vec::new(),
            sessions: Vec::new(),
            recorded: false,
        }
    }

    /// The unanalyzed files grouped by directory, in directory order.
    pub fn unanalyzed_by_directory(&self) -> Vec<UnanalyzedDirectory> {
        let mut by_dir: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for file in &self.unanalyzed {
            by_dir.entry(directory_of(file)).or_default().push(file);
        }
        by_dir
            .into_iter()
            .map(|(directory, files)| UnanalyzedDirectory {
                directory: directory.to_string(),
                count: files.len(),
                files: files.into_iter().map(str::to_string).collect(),
            })
            .collect()
    }

    /// The document both surfaces answer with:
    /// `{summary, unanalyzed, stale, deleted, sessions}`.
    pub fn document(&self) -> ProgressDocument {
        ProgressDocument {
            summary: ProgressSummary {
                files_total: self.summary.files_total,
                files_analyzed: self.summary.files_analyzed,
                entities_produced: self.summary.entities_produced,
            },
            unanalyzed: self.unanalyzed.clone(),
            stale: self.stale.clone(),
            deleted: self.deleted.clone(),
            sessions: self
                .sessions
                .iter()
                .map(|session| SessionRow {
                    session_id: session.session_id.clone(),
                    agent: session.agent.clone(),
                    status: session.status.name().to_string(),
                    started_at: session.started_at.clone(),
                    ended_at: session.ended_at.clone(),
                })
                .collect(),
        }
    }
}

/// What inference progress answers with, on both surfaces
/// (`McpInferProgressResult`, without its `message`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct ProgressDocument {
    pub summary: ProgressSummary,
    pub unanalyzed: Vec<String>,
    pub stale: Vec<String>,
    pub deleted: Vec<String>,
    pub sessions: Vec<SessionRow>,
}

/// File and entity totals.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct ProgressSummary {
    pub files_total: usize,
    pub files_analyzed: usize,
    pub entities_produced: usize,
}

/// One inference session.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct SessionRow {
    pub session_id: String,
    pub agent: String,
    pub status: String,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::super::gaps::gaps;
    use super::super::manifest::compute_content_hash;
    use super::*;
    use crate::view::testing::Fixture;
    use serde_json::json;
    use specforge_test_macros::test as specforge_test;

    /// A project with Rust sources under `src/`, one of them indexed,
    /// whose compile loaded a Rust analyzer.
    fn project() -> Fixture {
        let fixture = Fixture::new().declarations(rust_analyzer());
        let dir = &fixture.dir;
        std::fs::create_dir_all(dir.path().join("src/net")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "pub fn alpha() {}\n").unwrap();
        std::fs::write(dir.path().join("src/net/wire.rs"), "pub struct Wire;\n").unwrap();
        let hash = compute_content_hash(&dir.path().join("src/lib.rs")).unwrap();
        std::fs::write(
            dir.path().join("specforge-infer.json"),
            json!({
                "version": 1,
                "source_roots": ["src"],
                "source_index": [
                    {"path": "src/lib.rs", "content_hash": hash,
                     "entities_produced": ["alpha"], "analyzed_at": "2026-10-01T00:00:00Z"},
                    {"path": "src/gone.rs", "content_hash": "00",
                     "entities_produced": [], "analyzed_at": "2026-10-01T00:00:00Z"}
                ]
            })
            .to_string(),
        )
        .unwrap();
        fixture
    }

    /// An extension analyzing Rust files.
    fn rust_analyzer() -> Vec<ExtensionDeclaration> {
        vec![ExtensionDeclaration {
            handshake: specforge_protocol_types::HandshakeResponse {
                name: "@acme/rust".into(),
                version: "1.0.0".into(),
                ..Default::default()
            },
            analyzers: vec![specforge_protocol_types::AnalyzerDescriptor {
                language: "rust".into(),
                file_extensions: vec![".rs".into()],
                scan_export: "scan".into(),
                classify_export: "classify".into(),
                map_export: "map".into(),
                ..Default::default()
            }],
            ..Default::default()
        }]
    }

    #[specforge_test(
        behavior = "provide_mcp_infer_progress_tool",
        verify = "returns summary with unanalyzed files"
    )]
    fn progress_lists_unindexed_and_deleted_files() {
        let dir = project();
        let progress = progress(&dir.view()).unwrap();
        assert_eq!(progress.unanalyzed, ["src/net/wire.rs"]);
        assert_eq!(progress.deleted, ["src/gone.rs"]);
        assert!(progress.stale.is_empty(), "{progress:?}");
        assert_eq!(progress.summary.files_total, 2);
        assert_eq!(progress.summary.entities_produced, 1);
        assert_eq!(
            progress.unanalyzed_by_directory(),
            [UnanalyzedDirectory {
                directory: "src/net".to_string(),
                count: 1,
                files: vec!["src/net/wire.rs".to_string()],
            }]
        );
    }

    #[specforge_test(
        behavior = "provide_mcp_infer_progress_tool",
        verify = "detects stale files by content hash"
    )]
    fn progress_detects_a_changed_file() {
        let dir = project();
        std::fs::write(dir.dir.path().join("src/lib.rs"), "pub fn beta() {}\n").unwrap();
        assert_eq!(progress(&dir.view()).unwrap().stale, ["src/lib.rs"]);
    }

    #[specforge_test(
        behavior = "provide_mcp_infer_progress_tool",
        verify = "graceful handling when specforge-infer.json is missing"
    )]
    fn progress_without_a_manifest_counts_every_file_unanalyzed() {
        let dir = project();
        std::fs::remove_file(dir.dir.path().join("specforge-infer.json")).unwrap();
        let progress = progress(&dir.view()).unwrap();
        assert_eq!(progress.summary.files_analyzed, 0);
        assert!(progress.unanalyzed.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn progress_lists_the_sessions_in_order() {
        let dir = project();
        let none = progress(&dir.view()).unwrap();
        assert!(none.recorded && none.sessions.is_empty());

        std::fs::write(
            dir.dir.path().join("specforge-infer.json"),
            json!({
                "version": 1,
                "source_roots": ["src"],
                "sessions": [
                    {"session_id": "s-1", "agent": "a", "status": "completed",
                     "started_at": "t1", "ended_at": "t2"},
                    {"session_id": "s-2", "agent": "b", "status": "active", "started_at": "t3"}
                ]
            })
            .to_string(),
        )
        .unwrap();
        let progress = progress(&dir.view()).unwrap();
        let ids: Vec<&str> = progress
            .sessions
            .iter()
            .map(|s| s.session_id.as_str())
            .collect();
        assert_eq!(ids, ["s-1", "s-2"]);
        let doc = serde_json::to_value(progress.document()).unwrap();
        assert_eq!(doc["sessions"][0]["ended_at"], "t2");
        assert!(doc["sessions"][1].get("ended_at").is_none(), "{doc}");

        // Nothing recorded yet: no file.
        std::fs::remove_file(dir.dir.path().join("specforge-infer.json")).unwrap();
        let fresh = super::progress(&dir.view()).unwrap();
        assert!(!fresh.recorded && fresh.sessions.is_empty());
    }

    #[specforge_test(
        behavior = "mark_source_file_analyzed",
        verify = "mark records the path root-relative with / separators"
    )]
    fn an_entry_recorded_as_dot_slash_counts_as_analyzed() {
        let dir = project();
        let hash = compute_content_hash(&dir.dir.path().join("src/lib.rs")).unwrap();
        std::fs::write(
            dir.dir.path().join("specforge-infer.json"),
            json!({
                "version": 1,
                "source_roots": ["src"],
                "source_index": [
                    {"path": "./src/lib.rs", "content_hash": hash,
                     "entities_produced": ["alpha"], "analyzed_at": "2026-10-01T00:00:00Z"}
                ]
            })
            .to_string(),
        )
        .unwrap();
        let progress = progress(&dir.view()).unwrap();
        assert_eq!(progress.unanalyzed, ["src/net/wire.rs"]);
        assert!(progress.stale.is_empty(), "{progress:?}");
    }

    #[specforge_test(
        behavior = "load_inference_manifest",
        verify = "load refuses an unreadable or invalid manifest with E071"
    )]
    fn an_invalid_manifest_is_e071() {
        let dir = project();
        std::fs::write(dir.dir.path().join("specforge-infer.json"), "{ nope").unwrap();
        let error = progress(&dir.view()).unwrap_err();
        assert!(error.is(specforge_common::codes::E071), "{error:?}");
        assert_eq!(error.kind, crate::OpErrorKind::SchemaMismatch);
        assert!(
            error
                .message
                .starts_with("failed to parse specforge-infer.json:"),
            "{}",
            error.message
        );
        assert!(error.suggestion.is_some());
    }

    #[test]
    fn an_unreadable_manifest_is_e071_of_its_io_kind() {
        let dir = project();
        let path = dir.dir.path().join("specforge-infer.json");
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let error = progress(&dir.view()).unwrap_err();
        assert!(error.is(specforge_common::codes::E071), "{error:?}");
        assert_eq!(error.kind, crate::OpErrorKind::Internal);
        assert!(
            error
                .message
                .starts_with("failed to read specforge-infer.json:"),
            "{}",
            error.message
        );
    }

    #[test]
    fn progress_over_a_rootless_view_is_no_project_and_none_counts_nothing() {
        let dir = project();
        let rootless = dir.rootless_view();

        assert_eq!(progress(&rootless).unwrap_err().code, "no_project");
        assert_eq!(gaps(&rootless).unwrap_err().code, "no_project");
        let none = Progress::none();
        assert_eq!(none.summary.files_total, 0);
        assert_eq!(none.summary.files_analyzed, 0);
        assert!(none.unanalyzed.is_empty() && none.stale.is_empty() && none.deleted.is_empty());

        // Rooted, the same view counts the files at its root.
        assert_eq!(progress(&dir.view()).unwrap().summary.files_total, 2);
    }
}
