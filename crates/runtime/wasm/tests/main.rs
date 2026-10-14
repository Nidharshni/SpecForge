#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod calls;
mod handshake_conformance;
mod load_declaration;
mod protocol_host;
mod protocol_types;
mod runtime_contract;
mod sdk_vocabulary;
