//! Fixture for `ignore_test.rs`, which runs this binary with different
//! libtest arguments. Both tests are ignored, so a normal `cargo test`
//! reports them as ignored and records nothing.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_test_macros::test as specforge_test;

#[specforge_test(behavior = "record_test_via_drop_guard")]
#[ignore]
fn ignored_by_default() {
    assert_eq!(1 + 1, 2);
}

#[specforge_test(behavior = "record_test_via_drop_guard")]
#[ignore]
#[should_panic(expected = "the expected panic")]
fn ignored_should_panic() {
    panic!("the expected panic");
}
