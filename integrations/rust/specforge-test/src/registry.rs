use crossbeam_queue::SegQueue;
use serde::Serialize;
use specforge_protocol_types::TestStatus;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TestRecordEntry {
    pub entity_kind: String,
    pub entity_id: String,
    pub test_name: String,
    /// `module_path!()` of the test (`my_crate::cart::tests`): with
    /// `test_name` it names the test as libtest prints it, so a collector
    /// can tell attribute-linked tests from plain ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,
    /// Wall-clock duration in milliseconds (C11-04).
    pub duration_ms: u64,
    /// What the run said of the test: the protocol's [`TestStatus`]
    /// (ADR 0069). The guard records `Passed` or `Failed`; an `#[ignore]`d
    /// test records nothing.
    pub status: TestStatus,
}

static REGISTRY: SegQueue<TestRecordEntry> = SegQueue::new();

pub fn record(entry: TestRecordEntry) {
    REGISTRY.push(entry);
}

pub fn drain() -> Vec<TestRecordEntry> {
    let mut entries = Vec::new();
    while let Some(entry) = REGISTRY.pop() {
        entries.push(entry);
    }
    entries
}
