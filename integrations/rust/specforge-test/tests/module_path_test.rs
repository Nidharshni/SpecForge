//! A recorded entry names its test the way libtest prints it, so a
//! collector reading libtest's output can tell attribute-linked tests from
//! plain ones. Its own binary: it drains the global registry.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_test::guard::TestGuard;
use specforge_test::registry;
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "record_test_via_drop_guard",
    verify = "the recorded entry carries the test's module path"
)]
fn the_recorded_entry_carries_the_module_path() {
    registry::drain();
    drop(TestGuard::new(
        "behavior",
        "create_user",
        module_path!(),
        "rejects_a_duplicate_email",
        file!(),
        line!(),
        None,
    ));
    let entries = registry::drain();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].module_path.as_deref(), Some("module_path_test"));
    let json = serde_json::to_value(&entries[0]).unwrap();
    assert_eq!(json["module_path"], "module_path_test");
}
