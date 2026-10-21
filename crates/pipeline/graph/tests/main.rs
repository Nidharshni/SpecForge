#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod agreement;
mod build;
mod contracts;
mod copy;
mod cycles;
mod defaults;
mod degree;
mod delta_over;
mod derived;
mod field_form;
mod graph;
mod obligations;
mod reach;
mod reading;
mod resolution;
mod select;
mod spell;
mod stress;
mod titles;
mod tokens;
mod value_forms;
