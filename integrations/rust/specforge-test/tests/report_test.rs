#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_protocol_types::TestStatus;
use specforge_test::registry::TestRecordEntry;
use specforge_test::report;
use std::fs;

fn entry(id: &str, status: TestStatus) -> TestRecordEntry {
    TestRecordEntry {
        entity_kind: "behavior".to_string(),
        entity_id: id.to_string(),
        test_name: format!("test_{id}"),
        module_path: None,
        file: "test.rs".to_string(),
        verify: None,
        duration_ms: 0,
        status,
    }
}

#[test]
fn writes_report_json() {
    let dir = tempfile::tempdir().unwrap();
    let entries = vec![
        entry("create_user", TestStatus::Passed),
        entry("delete_user", TestStatus::Failed),
    ];

    report::write_report(dir.path(), "my_binary", &entries).unwrap();

    let path = dir.path().join("my_binary.json");
    assert!(path.exists());

    let content: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(content["schema_version"], "1.0");
    assert_eq!(content["binary_name"], "my_binary");
    assert_eq!(content["entries"].as_array().unwrap().len(), 2);
    // Entries are sorted by entity_id
    assert_eq!(content["entries"][0]["entity_id"], "create_user");
    assert_eq!(content["entries"][0]["status"], "passed");
    assert_eq!(content["entries"][1]["entity_id"], "delete_user");
    assert_eq!(content["entries"][1]["status"], "failed");
}

#[test]
fn empty_entries_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();

    report::write_report(dir.path(), "empty_binary", &[]).unwrap();

    let path = dir.path().join("empty_binary.json");
    assert!(!path.exists());
}

#[test]
fn report_bytes_are_compact_json() {
    let dir = tempfile::tempdir().unwrap();
    let entries = vec![entry("compact", TestStatus::Passed)];

    report::write_report(dir.path(), "compact_binary", &entries).unwrap();

    let raw = fs::read_to_string(dir.path().join("compact_binary.json")).unwrap();
    // C6-15: compact serialization — a single line, no pretty-print
    // whitespace between keys and values.
    assert!(
        !raw.contains('\n'),
        "report must not be pretty-printed: {raw:?}"
    );
    assert!(!raw.contains(": "), "no pretty spacing: {raw:?}");
    // Still valid JSON with the same schema.
    let content: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(content["schema_version"], "1.0");
    assert_eq!(content["binary_name"], "compact_binary");
    assert_eq!(content["entries"][0]["entity_id"], "compact");
    assert_eq!(content["entries"][0]["status"], "passed");
}

#[test]
fn entries_are_sorted_in_report() {
    let dir = tempfile::tempdir().unwrap();
    // Write entries in reverse order
    let entries = vec![
        entry("zebra", TestStatus::Passed),
        entry("alpha", TestStatus::Passed),
        entry("middle", TestStatus::Failed),
    ];

    report::write_report(dir.path(), "sorted_binary", &entries).unwrap();

    let path = dir.path().join("sorted_binary.json");
    let content: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let ids: Vec<&str> = content["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["entity_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["alpha", "middle", "zebra"]);
}
