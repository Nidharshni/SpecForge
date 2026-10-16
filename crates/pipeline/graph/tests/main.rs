#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod build;
mod contracts;
mod copy;
mod derived;
mod field_form;
mod graph;
mod obligations;
mod reading;
mod resolution;
mod stress;
