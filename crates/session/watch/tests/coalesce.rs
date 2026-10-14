//! The debounce rule on synthetic instants: nothing here sleeps or reads a
//! clock.

use std::time::{Duration, Instant};

use specforge_test_macros::test as spec;
use specforge_watch::Coalescer;

const WINDOW: Duration = Duration::from_millis(50);

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[spec(
    behavior = "debounce_file_changes",
    verify = "rapid successive changes coalesced into single batch"
)]
fn changes_less_than_a_window_apart_are_one_batch() {
    let t0 = Instant::now();
    let mut batch = Coalescer::new(WINDOW);
    batch.push("a", t0);
    batch.push("b", t0 + ms(30));
    batch.push("c", t0 + ms(60));
    // 49 ms after the last change: still quiet for less than the window.
    assert_eq!(batch.take_due(t0 + ms(109)), None);
    assert_eq!(batch.take_due(t0 + ms(110)), Some(vec!["a", "b", "c"]));
    assert_eq!(batch.take_due(t0 + ms(500)), None, "a batch is taken once");
}

#[spec(
    behavior = "debounce_file_changes",
    verify = "each change restarts the quiet window"
)]
fn each_change_restarts_the_window() {
    let t0 = Instant::now();
    let mut batch = Coalescer::new(WINDOW);
    batch.push("a", t0);
    assert_eq!(batch.due(), Some(t0 + WINDOW));
    batch.push("b", t0 + ms(40));
    assert_eq!(batch.due(), Some(t0 + ms(90)));
    batch.push("c", t0 + ms(80));
    assert_eq!(batch.due(), Some(t0 + ms(130)));
    assert_eq!(batch.take_due(t0 + ms(129)), None);
}

#[spec(
    behavior = "debounce_file_changes",
    verify = "single isolated change triggers after debounce window"
)]
fn a_batch_is_due_one_window_after_its_last_change() {
    let t0 = Instant::now();
    let mut batch = Coalescer::new(WINDOW);
    batch.push("only", t0);
    assert_eq!(batch.take_due(t0), None);
    assert_eq!(batch.take_due(t0 + ms(49)), None);
    assert_eq!(batch.take_due(t0 + WINDOW), Some(vec!["only"]));
}

#[spec(
    behavior = "debounce_file_changes",
    verify = "debounce window prevents redundant recompilation"
)]
fn repeated_changes_are_in_the_batch_once() {
    let t0 = Instant::now();
    let mut batch = Coalescer::new(WINDOW);
    for i in 0..5 {
        batch.push("same", t0 + ms(i));
    }
    assert_eq!(batch.flush(), Some(vec!["same"]));
}

#[spec(
    behavior = "debounce_file_changes",
    verify = "coalesced batch includes union of all changed files"
)]
fn the_batch_is_the_union_sorted() {
    let t0 = Instant::now();
    let mut batch = Coalescer::new(WINDOW);
    batch.push("z", t0);
    batch.push("x", t0 + ms(1));
    batch.push("y", t0 + ms(2));
    batch.push("x", t0 + ms(3));
    assert_eq!(batch.flush(), Some(vec!["x", "y", "z"]));
}

#[spec(behavior = "debounce_file_changes")]
fn nothing_pending_is_never_due() {
    let mut batch = Coalescer::<&str>::new(WINDOW);
    assert_eq!(batch.due(), None);
    assert_eq!(
        batch.take_due(Instant::now() + Duration::from_secs(60)),
        None
    );
    assert_eq!(batch.flush(), None);
}

#[spec(behavior = "debounce_file_changes")]
fn a_flushed_batch_starts_the_next_one_afresh() {
    let t0 = Instant::now();
    let mut batch = Coalescer::new(WINDOW);
    batch.push("a", t0);
    assert_eq!(batch.flush(), Some(vec!["a"]));
    assert_eq!(batch.due(), None);
    batch.push("b", t0 + ms(500));
    assert_eq!(batch.take_due(t0 + ms(550)), Some(vec!["b"]));
}
