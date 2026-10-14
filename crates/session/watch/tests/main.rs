#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod coalesce;
mod contracts;
mod debounce;
mod session_watch;
mod watcher;
