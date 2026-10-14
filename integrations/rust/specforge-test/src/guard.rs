use crate::atexit;
use crate::registry::{self, TestRecordEntry};
use specforge_protocol_types::TestStatus;
use std::time::Instant;

pub struct TestGuard {
    entity_kind: &'static str,
    entity_id: &'static str,
    module_path: &'static str,
    test_name: &'static str,
    file: &'static str,
    verify: Option<&'static str>,
    /// `#[should_panic]` test: a panic is the success path (C11-04).
    expect_panic: bool,
    started: Instant,
}

/// Fail a test that runs twice in one test binary. `#[specforge_test]`
/// registers the test itself; a `#[test]` written *above* it is invisible
/// to the macro (the compiler expands it first) and would register the
/// function a second time, doubling every recorded result.
pub fn assert_registered_once(
    module_path: &'static str,
    test_name: &'static str,
    entity_id: &'static str,
    verify: Option<&'static str>,
) {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    type Key = (
        &'static str,
        &'static str,
        &'static str,
        Option<&'static str>,
    );
    static SEEN: OnceLock<Mutex<HashSet<Key>>> = OnceLock::new();
    let first = SEEN
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert((module_path, test_name, entity_id, verify));
    assert!(
        first,
        "{module_path}::{test_name} is registered twice: remove its #[test] \
         (#[specforge_test] registers the test itself)"
    );
}

impl TestGuard {
    pub fn new(
        entity_kind: &'static str,
        entity_id: &'static str,
        module_path: &'static str,
        test_name: &'static str,
        file: &'static str,
        _line: u32,
        verify: Option<&'static str>,
    ) -> Self {
        Self::with_expectations(
            entity_kind,
            entity_id,
            module_path,
            test_name,
            file,
            _line,
            verify,
            false,
        )
    }

    /// Macro entry point carrying the `#[should_panic]` expectation
    /// (C11-04). An `#[ignore]`d test runs only when libtest is asked to
    /// (`--ignored`), and is then recorded like any other.
    #[expect(
        clippy::too_many_arguments,
        reason = "the macro's whole expansion: entity, verify, module path, file, line and the three expectations"
    )]
    pub fn with_expectations(
        entity_kind: &'static str,
        entity_id: &'static str,
        module_path: &'static str,
        test_name: &'static str,
        file: &'static str,
        _line: u32,
        verify: Option<&'static str>,
        expect_panic: bool,
    ) -> Self {
        atexit::ensure_registered();
        Self {
            entity_kind,
            entity_id,
            module_path,
            test_name,
            file,
            verify,
            expect_panic,
            started: Instant::now(),
        }
    }
}

impl Drop for TestGuard {
    fn drop(&mut self) {
        let status = if self.expect_panic {
            // C11-04: a panic under #[should_panic] is the success path —
            // recording Fail made every should_panic test count against
            // coverage.
            if std::thread::panicking() {
                TestStatus::Passed
            } else {
                // should_panic tests that never panicked are harness
                // failures; the harness itself catches that case, but the
                // guard still ran, so record Fail.
                TestStatus::Failed
            }
        } else if std::thread::panicking() {
            TestStatus::Failed
        } else {
            TestStatus::Passed
        };

        let entry = TestRecordEntry {
            entity_kind: self.entity_kind.to_string(),
            entity_id: self.entity_id.to_string(),
            test_name: self.test_name.to_string(),
            module_path: Some(self.module_path.to_string()),
            file: self.file.to_string(),
            verify: self.verify.map(|s| s.to_string()),
            duration_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            status,
        };
        registry::record(entry);
    }
}
