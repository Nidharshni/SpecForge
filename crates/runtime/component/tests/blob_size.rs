//! The committed builtins are built small (`[profile.blob]`, I1): a size budget so a build
//! that drops the profile shows up as a test failure, not as a bigger binary and pack.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_component::builtins::BUILTINS;
use specforge_component::committed::COMMITTED;

const TOTAL_BUDGET: usize = 3_500_000;
const BLOB_BUDGET: usize = 800_000;

#[test]
fn committed_builtins_stay_under_their_size_budget() {
    let sizes: Vec<(&str, usize)> = COMMITTED[..BUILTINS.len()]
        .iter()
        .map(|b| (b.name, b.read().len()))
        .collect();
    let total: usize = sizes.iter().map(|(_, n)| n).sum();
    assert!(total <= TOTAL_BUDGET, "builtins total {total} B: {sizes:?}");
    for (name, n) in &sizes {
        assert!(*n <= BLOB_BUDGET, "{name} is {n} B");
    }
}
