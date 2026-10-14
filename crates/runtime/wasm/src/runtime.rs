use crate::sandbox::Limits;

/// Result of calling a Wasm export function.
#[derive(Debug, Clone)]
pub enum WasmCallResult {
    Ok(Vec<u8>),
    Trap(WasmTrapInfo),
}

/// Details of a Wasm trap.
#[derive(Debug, Clone)]
pub struct WasmTrapInfo {
    pub kind: String,
    pub message: String,
    pub export_name: String,
}

/// Testable abstraction over a Wasm runtime (wasmtime component engine).
///
/// Compilation caching is a host-engine concern (wasmtime's native on-disk
/// cache, configured at `ComponentRuntime` construction) — it is not part of
/// the load contract.
pub trait WasmRuntime: Send + Sync {
    /// Compile the component `bytes` and register it under `name`,
    /// replacing what was loaded under it. The bytes are the ones the
    /// caller checked: the runtime reads nothing else.
    fn load(&self, name: &str, bytes: &[u8]) -> Result<(), String>;

    /// Register what is loaded as `from` under `to` instead, without
    /// compiling it again. False when `from` is not loaded.
    fn rename(&self, from: &str, to: &str) -> bool;

    /// Drop what is loaded as `name`. False when nothing was.
    fn unload(&self, name: &str) -> bool;

    /// Call an export function on a loaded module.
    fn call_export(&self, extension_name: &str, export_name: &str, input: &[u8]) -> WasmCallResult;

    /// Hold `extension_name`'s later calls to `limits` (its handshake's
    /// sandbox, ADR 0037): the component runtime enforces them, the
    /// in-process runtime records them (its guest runs in the host process
    /// and is held to nothing). An extension that is not loaded is ignored.
    fn apply_limits(&self, extension_name: &str, limits: Limits);
}
