use specforge_protocol_types::TestStatus;
use specforge_test::registry;
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn macro_injects_guard_that_records_to_registry() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    registry::drain();

    annotated_passing_test();

    let entries = registry::drain();
    assert_eq!(
        entries.len(),
        1,
        "expected 1 entry from macro-annotated function"
    );
    assert_eq!(entries[0].entity_kind, "behavior");
    assert_eq!(entries[0].entity_id, "create_user");
    assert_eq!(entries[0].status, TestStatus::Passed);
}

#[test]
fn macro_supports_invariant_kind() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    registry::drain();

    annotated_invariant_test();

    let entries = registry::drain();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].entity_kind, "invariant");
    assert_eq!(entries[0].entity_id, "unique_ids");
}

#[test]
fn macro_accepts_custom_entity_kind() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    registry::drain();

    annotated_custom_kind_test();

    let entries = registry::drain();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].entity_kind, "constraint");
    assert_eq!(entries[0].entity_id, "naming_convention");
    assert_eq!(entries[0].status, TestStatus::Passed);
}

#[test]
fn multiple_attributes_produce_multiple_guards() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    registry::drain();

    annotated_multi_entity();

    let entries = registry::drain();
    assert_eq!(
        entries.len(),
        2,
        "expected 2 entries from double-annotated function"
    );
    let kinds: Vec<&str> = entries.iter().map(|e| e.entity_kind.as_str()).collect();
    assert!(kinds.contains(&"behavior"));
    assert!(kinds.contains(&"invariant"));
}

#[test]
fn panicking_annotated_function_records_fail() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    registry::drain();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        annotated_panicking();
    }));

    assert!(result.is_err());
    let entries = registry::drain();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].status, TestStatus::Failed);
}

#[test]
fn a_test_registered_twice_fails_its_second_run() {
    use specforge_test::__private::assert_registered_once;
    assert_registered_once("m", "twice", "e", Some("v"));
    let second = std::panic::catch_unwind(|| assert_registered_once("m", "twice", "e", Some("v")));
    let message = second.expect_err("the second registration must fail");
    let message = message
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(message.contains("remove its #[test]"), "{message}");
    // A different obligation of the same test is a separate registration.
    assert_registered_once("m", "twice", "e", Some("other"));
}

// Helper functions with the macro attribute, called directly: `__guard_only`
// records the guard without registering them as tests.
#[specforge_test_macros::test(behavior = "create_user", __guard_only)]
fn annotated_passing_test() {}

#[specforge_test_macros::test(invariant = "unique_ids", __guard_only)]
fn annotated_invariant_test() {}

#[specforge_test_macros::test(constraint = "naming_convention", __guard_only)]
fn annotated_custom_kind_test() {}

#[specforge_test_macros::test(behavior = "create_user", __guard_only)]
#[specforge_test_macros::test(invariant = "unique_ids", __guard_only)]
fn annotated_multi_entity() {}

#[specforge_test_macros::test(behavior = "delete_user", __guard_only)]
fn annotated_panicking() {
    panic!("simulated failure");
}
