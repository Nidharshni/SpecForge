use specforge_protocol_types::TestStatus;
use specforge_test::guard::TestGuard;
use specforge_test::registry;
use std::panic;
use std::sync::Mutex;

// Guard tests must run serially because they share the global registry.
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn passing_test_records_pass() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    registry::drain();

    {
        let _guard = TestGuard::new(
            "behavior",
            "create_user",
            "mod",
            "test_fn",
            "test.rs",
            1,
            None,
        );
    }

    let entries = registry::drain();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].entity_id, "create_user");
    assert_eq!(entries[0].entity_kind, "behavior");
    assert_eq!(entries[0].status, TestStatus::Passed);
}

#[test]
fn panicking_test_records_fail() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    registry::drain();

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let _guard = TestGuard::new(
            "behavior",
            "delete_user",
            "mod",
            "test_fn",
            "test.rs",
            1,
            None,
        );
        panic!("simulated test failure");
    }));

    assert!(result.is_err());
    let entries = registry::drain();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].entity_id, "delete_user");
    assert_eq!(entries[0].status, TestStatus::Failed);
}

#[test]
fn multiple_guards_record_multiple_entries() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    registry::drain();

    {
        let _g1 = TestGuard::new(
            "behavior",
            "create_user",
            "mod",
            "test_fn",
            "test.rs",
            1,
            None,
        );
        let _g2 = TestGuard::new(
            "invariant",
            "unique_ids",
            "mod",
            "test_fn",
            "test.rs",
            1,
            None,
        );
    }

    let entries = registry::drain();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].entity_id, "unique_ids"); // dropped in reverse order
    assert_eq!(entries[1].entity_id, "create_user");
}

#[test]
fn drain_clears_registry() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    registry::drain();

    {
        let _guard = TestGuard::new("behavior", "foo", "mod", "test_fn", "test.rs", 1, None);
    }

    let first = registry::drain();
    assert_eq!(first.len(), 1);

    let second = registry::drain();
    assert_eq!(second.len(), 0);
}

// --- C11-04: should_panic inversion ---

#[test]
fn should_panic_guard_records_pass_on_panic() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    registry::drain();

    let result = std::panic::catch_unwind(|| {
        let _guard = TestGuard::with_expectations(
            "behavior",
            "panics_by_contract",
            "mod",
            "should_panic_test",
            "test.rs",
            1,
            None,
            true,
        );
        panic!("this panic is the success path");
    });
    assert!(
        result.is_err(),
        "the panic must still propagate to the harness"
    );
    let after = registry::drain();
    assert_eq!(after.len(), 1, "exactly this guard recorded: {after:?}");
    assert_eq!(
        after[0].status,
        TestStatus::Passed,
        "#[should_panic] panic must record Pass, not Fail (C11-04)"
    );
}
