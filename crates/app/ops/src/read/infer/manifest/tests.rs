use super::*;
use specforge_test_macros::test as specforge_test;
use std::fs;
use tempfile::TempDir;

/// The manifest's file in `dir`, held by a containment bounded to `dir`.
fn held(dir: &Path) -> Held {
    Containment::within(dir)
        .resolve(Path::new(MANIFEST_FILENAME))
        .unwrap()
}

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
        message.starts_with("failed to parse specforge-infer.json: unknown variant `Completed`"),
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

    let writes = m.write(&held(dir.path())).unwrap();
    assert_eq!(writes.named(Some(dir.path())), ["specforge-infer.json"]);
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
    m.write(&held(dir.path())).unwrap();

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
    m.write(&held(dir.path())).unwrap();

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
    let error = InferenceManifest::default()
        .write(&held(dir.path()))
        .unwrap_err();
    assert_eq!(error.code, MANIFEST_WRITE_FAILED);
    assert!(!dir.path().join("specforge-infer.json.tmp").exists());
}

#[cfg(unix)]
#[test]
fn a_manifest_that_leaves_the_root_is_unusable_and_not_read() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("proj");
    fs::create_dir(&root).unwrap();
    fs::write(
        dir.path().join("outside.json"),
        r#"{"version":1,"source_roots":[]}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(
        dir.path().join("outside.json"),
        root.join(MANIFEST_FILENAME),
    )
    .unwrap();

    let problem = InferenceManifest::read(&root).unwrap_err();
    assert!(
        problem.message().contains("outside"),
        "{}",
        problem.message()
    );
    let error = OpError::from(problem);
    assert_eq!(error.kind, OpErrorKind::InvalidInput);
    assert_eq!(error.code, codes::E071.to_string());
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
