//! The host side of the extension protocol: the `WasmRuntime` port every extension call goes through, the typed
//! calls (`ExtensionCalls`) whose every failure is one E028 (ADR 0013), the sandbox limits the host holds an
//! extension to (ADR 0037), and, under `testing`, the in-process runtime. Its production adapter is
//! `specforge-component`.

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
