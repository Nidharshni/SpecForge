#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod contract;
mod credentials;
mod home;
mod registry_client;
mod registry_config;
mod registry_ops;
mod retry;
mod user_files;
