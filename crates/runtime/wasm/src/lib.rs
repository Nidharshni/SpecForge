#![allow(clippy::result_large_err)]

pub mod calls;
pub mod protocol;
pub mod runtime;
pub mod sandbox;
#[cfg(feature = "testing")]
pub mod testing;

pub use calls::{
    CallError, CallFailure, Encoded, ExtensionCalls, Handshake, Operation, pass_diagnostics,
};
pub use runtime::{WasmCallResult, WasmRuntime, WasmTrapInfo};
pub use sandbox::{Limits, Sandbox};
