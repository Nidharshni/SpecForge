//! The component-model runtime (wasmtime): the production adapter of
//! [`WasmRuntime`]. Every extension call (the declaration, commands, passes,
//! collectors, custom rules, scanners, migration hooks) reaches a guest
//! through that trait, here or in the in-process test adapter
//! (`specforge_wasm::testing::InProcessRuntime`).
//!
//! The guest contract is the `specforge:bridge/bridge` world: one `call`
//! export dispatching the JSON wire protocol by export name. Guests are
//! `wasm32-wasip2` components (rustc emits the component encoding directly
//! for that target).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use specforge_wasm::runtime::{WasmCallResult, WasmRuntime, WasmTrapInfo};
use specforge_wasm::sandbox::Limits;
use wasmtime::component::{Component, Linker, Resource};
use wasmtime::{Cache, CacheConfig, Config, Engine, Store};
use wasmtime_wasi::p2::DynPollable;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "wit",
    world: "bridge",
});

pub mod builtins;
#[cfg(feature = "testing")]
pub mod committed;

/// Per-instance state: the WASI context (no capability) and the memory
/// ceiling the limiter enforces. Builtins are pure-compute, but wasip2
/// targets import `wasi:io/poll` from std, so the linker always provides it.
struct HostState {
    table: wasmtime_wasi::ResourceTable,
    wasi: WasiCtx,
    memory: MemoryCeiling,
}

/// Refuses any growth that takes the instance's memories and tables, all
/// counted together, past `bytes`, trapping the call; records the refusal
/// so the call's trap is named `memory_limit_exceeded` whatever way
/// wasmtime wraps the error (ADR 0037).
struct MemoryCeiling {
    bytes: usize,
    /// What every memory and table of the instance holds now.
    used: usize,
    refused: Option<usize>,
}

/// What one table element is counted as: a function reference is a
/// pointer.
const TABLE_ELEMENT_BYTES: usize = size_of::<usize>();

impl MemoryCeiling {
    fn of(limits: Limits) -> Self {
        MemoryCeiling {
            bytes: (limits.memory_mb as usize) << 20,
            used: 0,
            refused: None,
        }
    }

    /// Admits one memory or table going from `current` to `desired` bytes
    /// within the instance's total, or traps.
    fn grow(&mut self, current: usize, desired: usize) -> wasmtime::Result<()> {
        let total = self.used.saturating_sub(current).saturating_add(desired);
        if total > self.bytes {
            self.refused = Some(total);
            wasmtime::bail!(
                "memory limit exceeded: {total} bytes in all asked, {} MiB allowed",
                self.bytes >> 20
            );
        }
        self.used = total;
        Ok(())
    }
}

impl wasmtime::ResourceLimiter for MemoryCeiling {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if maximum.is_some_and(|max| desired > max) {
            return Ok(false);
        }
        self.grow(current, desired)?;
        Ok(true)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if maximum.is_some_and(|max| desired > max) {
            return Ok(false);
        }
        self.grow(
            current.saturating_mul(TABLE_ELEMENT_BYTES),
            desired.saturating_mul(TABLE_ELEMENT_BYTES),
        )?;
        Ok(true)
    }
}

/// The capabilities a guest gets: none. No preopened directory,
/// environment, arguments or stdin (closed); stdout and stderr discarded;
/// TCP, UDP and name lookup refused (every socket address is denied by
/// default as well). Clocks and randomness are WASI's own (wasip2 std needs
/// them). Spelled out so that a future wasmtime default that allows
/// something cannot grant it silently (ADR 0037).
fn no_capabilities() -> WasiCtx {
    WasiCtx::builder()
        .allow_tcp(false)
        .allow_udp(false)
        .allow_ip_name_lookup(false)
        .build()
}

/// A guest waiting on a timer would block the host thread out of the
/// epoch deadline's reach (it fires only while guest code runs), holding
/// the extension's lock for as long as it asks. So the two calls that make
/// a timer trap instead: a guest has nothing else to wait on, as it holds no
/// stream or socket that can block.
fn refuse_timers(linker: &mut Linker<HostState>) -> wasmtime::Result<()> {
    linker.allow_shadowing(true);
    // shortcut: the version wasmtime-wasi 49 links; a wasmtime upgrade that
    // changes it fails `an_extension_waiting_on_a_timer_does_not_block_the_host`.
    let mut clock = linker.instance("wasi:clocks/monotonic-clock@0.2.12")?;
    for subscribe in ["subscribe-duration", "subscribe-instant"] {
        clock.func_wrap(
            subscribe,
            |_, (_,): (u64,)| -> wasmtime::Result<(Resource<DynPollable>,)> {
                wasmtime::bail!("the sandbox has no timer to wait on (ADR 0037)")
            },
        )?;
    }
    linker.allow_shadowing(false);
    Ok(())
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}
struct PluginInstance {
    store: Store<HostState>,
    bindings: Bridge,
    /// What each call into this plugin is held to. The wall-clock budget
    /// is enforced with wasmtime epoch interruption; `set_epoch_deadline`
    /// is refreshed from it before every `call`.
    limits: Limits,
    /// What the instance was made from, to make a fresh one after a trap:
    /// a component instance that trapped cannot be entered again.
    component: Component,
    fuel: u64,
}

/// Deterministic instruction budget, given whole to every call (refilled
/// before each), shared by every surface.
pub const DEFAULT_FUEL_LIMIT: u64 = 30_000 * 20_000_000;

/// Granularity of the epoch ticker: a plugin's `max_execution_ms` deadline is
/// never enforced early, and overshoots by at most two of these intervals
/// (plus scheduling delay). 10 ms keeps deadline precision well under any
/// meaningful budget while the ticker thread costs one wakeup per interval
/// per process.
pub const EPOCH_TICK_MS: u64 = 10;

/// Converts a wall-clock millisecond budget into a number of epoch ticks.
///
/// The ticker runs continuously, so the first tick after a call starts can
/// arrive at any point within one interval — even immediately. Consecutive
/// ticks are at least [`EPOCH_TICK_MS`] apart (`sleep` never wakes early), so
/// `n` ticks guarantee only `(n - 1)` full intervals. A positive budget
/// therefore gets one tick more than it covers: the call is never
/// interrupted before `deadline_ms` has elapsed. Zero stays zero (trap at the
/// first checkpoint).
fn ms_to_ticks(deadline_ms: u64) -> u64 {
    if deadline_ms == 0 {
        0
    } else {
        deadline_ms.div_ceil(EPOCH_TICK_MS) + 1
    }
}

/// Background thread that increments the engine epoch every
/// [`EPOCH_TICK_MS`]. This is wasmtime's documented mechanism for epoch
/// interruption: stores set a deadline in ticks and trap once the engine's
/// epoch (advanced only by this thread) passes it. Stopped and joined when
/// the owning [`ComponentRuntime`] drops.
struct EpochTicker {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl EpochTicker {
    fn spawn(engine: Engine) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            while !stop_flag.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(EPOCH_TICK_MS));
                engine.increment_epoch();
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// A `WasmRuntime` backed by wasmtime 49 Component Model instances.
pub struct ComponentRuntime {
    engine: Engine,
    /// Loaded plugins. The map lock guards membership only; each plugin has
    /// its own lock so calls into DIFFERENT extensions run concurrently while
    /// calls into the SAME extension (its `Store` is single-threaded state)
    /// still serialize (audit C7-10).
    plugins: Mutex<HashMap<String, Arc<Mutex<PluginInstance>>>>,
    fuel: u64,
    /// Drives epoch interruption; must outlive every `Store`.
    _ticker: EpochTicker,
}

impl ComponentRuntime {
    /// Runtime without a compilation cache (unit tests, one-shot tooling).
    pub fn new() -> Self {
        Self::construct(None)
    }

    /// Runtime with wasmtime's on-disk compilation cache enabled.
    ///
    /// The cache MUST be configured before the `Engine` is built — wasmtime
    /// reads the cache setting at construction — so this is a constructor,
    /// not a post-hoc setter (C7-02: the previous `.aot` side cache was a
    /// byte copy that no runtime ever consumed).
    pub fn new_with_compile_cache(dir: PathBuf) -> Self {
        Self::construct(Some(dir))
    }

    /// The runtime a project's extensions load into: with the per-user
    /// compilation cache ([`user_compile_cache_dir`]), or without when it is
    /// switched off. It loads nothing: the environment's extension load
    /// does (`Installed::load`).
    pub fn with_user_cache() -> Self {
        match user_compile_cache_dir() {
            Some(dir) => Self::new_with_compile_cache(dir),
            None => Self::new(),
        }
    }

    fn construct(cache_dir: Option<PathBuf>) -> Self {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.consume_fuel(true);
        // Epoch interruption enforces each plugin's `max_execution_ms`
        // wall-clock budget (audit C7-10): stores get a deadline in ticks
        // and the [`EpochTicker`] thread advances the engine epoch so long
        // loops trap instead of pinning a host thread past their budget.
        config.epoch_interruption(true);
        // Unix signals, not Mach ports, catch guest faults on macOS too.
        // wasmtime's Mach-port handler thread aborts the whole process when
        // its `mach_msg` wait is interrupted (MACH_RCV_INTERRUPTED), which
        // happens whenever a caught signal (e.g. a SIGCHLD handler some
        // library installs) is delivered to that thread. Signal-based traps
        // have no such thread and are what Linux uses already.
        config.macos_use_mach_ports(false);
        if let Some(dir) = &cache_dir
            && let Err(e) = enable_compile_cache(&mut config, dir)
        {
            eprintln!(
                "warning: wasm compile cache disabled ({}: {e})",
                dir.display()
            );
        }
        let engine = Engine::new(&config).expect("engine initializes");
        Self {
            _ticker: EpochTicker::spawn(engine.clone()),
            engine,
            plugins: Mutex::new(HashMap::new()),
            fuel: DEFAULT_FUEL_LIMIT,
        }
    }

    /// Deterministic per-call instruction budget, enforced by the engine.
    pub fn with_fuel_limit(mut self, fuel: u64) -> Self {
        self.fuel = fuel;
        self
    }

    /// Names of the currently loaded extensions, sorted.
    pub fn loaded_names(&self) -> Vec<String> {
        match self.plugins.lock() {
            Ok(plugins) => {
                let mut names: Vec<String> = plugins.keys().cloned().collect();
                names.sort();
                names
            }
            Err(_) => Vec::new(),
        }
    }
    fn instantiate_with_fuel(
        &self,
        name: &str,
        component: Component,
        fuel: u64,
    ) -> Result<(), String> {
        let (store, bindings) = self.fresh_instance(&component, fuel, Limits::CEILING)?;
        let mut plugins = self.plugins.lock().map_err(|e| e.to_string())?;
        plugins.insert(
            name.to_string(),
            Arc::new(Mutex::new(PluginInstance {
                store,
                bindings,
                limits: Limits::CEILING,
                component,
                fuel,
            })),
        );
        Ok(())
    }

    /// A new instance of `component`, with `fuel`, `limits`' memory ceiling
    /// and its wall-clock budget armed.
    fn fresh_instance(
        &self,
        component: &Component,
        fuel: u64,
        limits: Limits,
    ) -> Result<(Store<HostState>, Bridge), String> {
        let mut linker: Linker<HostState> = Linker::new(&self.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .map_err(|e| format!("failed to add WASI to linker: {e}"))?;
        refuse_timers(&mut linker).map_err(|e| format!("failed to refuse WASI timers: {e}"))?;

        let table = wasmtime_wasi::ResourceTable::new();
        let mut store = Store::new(
            &self.engine,
            HostState {
                table,
                wasi: no_capabilities(),
                memory: MemoryCeiling::of(limits),
            },
        );
        store.limiter(|state| &mut state.memory);
        store
            .set_fuel(fuel)
            .map_err(|e| format!("failed to set fuel: {e}"))?;
        // With epoch interruption enabled, stores start with a deadline of
        // zero ticks and would trap immediately — arm the plugin's
        // wall-clock budget before any guest code can run.
        store.set_epoch_deadline(ms_to_ticks(u64::from(limits.execution_ms)));

        let bindings = Bridge::instantiate(&mut store, component, &linker)
            .map_err(|e| format!("failed to instantiate component: {e}"))?;
        Ok((store, bindings))
    }
}

/// What stopped a call: a limit the sandbox holds it to, or another fault.
fn trap_kind(error: &wasmtime::Error, memory_refused: Option<usize>) -> &'static str {
    if memory_refused.is_some() {
        return "memory_limit_exceeded";
    }
    match error.downcast_ref::<wasmtime::Trap>() {
        Some(wasmtime::Trap::Interrupt) => "deadline_exceeded",
        Some(wasmtime::Trap::OutOfFuel) => "fuel_exhausted",
        _ => "call_failed",
    }
}

impl Default for ComponentRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmRuntime for ComponentRuntime {
    /// Compile a component from bytes and register it under `name`,
    /// atomically replacing any existing plugin (hot reload / H1).
    fn load(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let component = Component::from_binary(&self.engine, bytes)
            .map_err(|e| format!("failed to compile component: {e}"))?;
        self.instantiate_with_fuel(name, component, self.fuel)
    }

    fn rename(&self, from: &str, to: &str) -> bool {
        let Ok(mut plugins) = self.plugins.lock() else {
            return false;
        };
        match plugins.remove(from) {
            Some(plugin) => {
                plugins.insert(to.to_string(), plugin);
                true
            }
            None => false,
        }
    }

    fn unload(&self, name: &str) -> bool {
        match self.plugins.lock() {
            Ok(mut plugins) => plugins.remove(name).is_some(),
            Err(_) => false,
        }
    }

    /// Call the bridge `call` export; returns the raw JSON wire bytes.
    fn call_export(&self, name: &str, export: &str, input: &[u8]) -> WasmCallResult {
        // Look up the plugin and release the map lock immediately: holding
        // it across the guest call would serialize every extension behind
        // one mutex (the C7-10 finding). Only the target plugin's own lock
        // is held for the duration of the call.
        let plugin = {
            let plugins = match self.plugins.lock() {
                Ok(p) => p,
                Err(e) => {
                    return WasmCallResult::Trap(WasmTrapInfo {
                        kind: "lock_poisoned".to_string(),
                        message: e.to_string(),
                        export_name: export.to_string(),
                    });
                }
            };
            match plugins.get(name) {
                Some(plugin) => Arc::clone(plugin),
                None => {
                    return WasmCallResult::Trap(WasmTrapInfo {
                        kind: "extension_not_found".to_string(),
                        message: format!("Extension '{name}' not loaded"),
                        export_name: export.to_string(),
                    });
                }
            }
        };
        let mut instance = match plugin.lock() {
            Ok(instance) => instance,
            Err(e) => {
                return WasmCallResult::Trap(WasmTrapInfo {
                    kind: "lock_poisoned".to_string(),
                    message: e.to_string(),
                    export_name: export.to_string(),
                });
            }
        };
        let PluginInstance {
            store,
            bindings,
            limits,
            component,
            fuel,
        } = &mut *instance;
        // Every call gets the whole fuel budget and the whole wall-clock
        // budget: a long session never spends one call's allowance on the
        // next.
        if let Err(e) = store.set_fuel(*fuel) {
            return WasmCallResult::Trap(WasmTrapInfo {
                kind: "call_failed".to_string(),
                message: format!("failed to set fuel for {name}: {e}"),
                export_name: export.to_string(),
            });
        }
        store.set_epoch_deadline(ms_to_ticks(u64::from(limits.execution_ms)));
        match bindings.call_call(&mut *store, name, export, input) {
            Ok(Ok(bytes)) => WasmCallResult::Ok(bytes),
            Ok(Err(message)) => WasmCallResult::Trap(WasmTrapInfo {
                kind: "guest_error".to_string(),
                message,
                export_name: export.to_string(),
            }),
            Err(e) => {
                // What stopped the call is read before the instance is
                // replaced: a limit of the sandbox is named, any other
                // fault keeps wasmtime's message.
                let memory_refused = store.data_mut().memory.refused.take();
                let kind = trap_kind(&e, memory_refused);
                let message = match kind {
                    "memory_limit_exceeded" => format!(
                        "grew past its {} MB memory limit (sandbox_policy.max_memory_mb)",
                        limits.memory_mb
                    ),
                    "fuel_exhausted" => {
                        format!("spent its fuel budget ({fuel} instructions) in one call")
                    }
                    "deadline_exceeded" => format!(
                        "ran past its {} ms execution limit (sandbox_policy.max_execution_ms)",
                        limits.execution_ms
                    ),
                    _ => e.to_string(),
                };
                // An instance that trapped cannot be entered again: the
                // extension's next call gets a fresh one under the same
                // limits, as a guest that panics in one call must not take
                // the extension down for the rest of the process (an MCP
                // session).
                // One that cannot start (its own limits refuse it) is
                // unloaded, so later calls say it is gone instead of failing
                // on the dead instance.
                let message = match self.fresh_instance(component, *fuel, *limits) {
                    Ok((fresh_store, fresh_bindings)) => {
                        *store = fresh_store;
                        *bindings = fresh_bindings;
                        message
                    }
                    Err(why) => {
                        // The plugin's lock goes first: `apply_limits` takes
                        // the map's, then a plugin's.
                        drop(instance);
                        if let Ok(mut plugins) = self.plugins.lock()
                            && plugins.get(name).is_some_and(|p| Arc::ptr_eq(p, &plugin))
                        {
                            plugins.remove(name);
                        }
                        format!("{message}; it could not be restarted ({why}) and is unloaded")
                    }
                };
                WasmCallResult::Trap(WasmTrapInfo {
                    kind: kind.to_string(),
                    message,
                    export_name: export.to_string(),
                })
            }
        }
    }

    fn apply_limits(&self, extension_name: &str, limits: Limits) {
        let Ok(plugins) = self.plugins.lock() else {
            return;
        };
        if let Some(plugin) = plugins.get(extension_name)
            && let Ok(mut plugin) = plugin.lock()
        {
            plugin.limits = limits;
            plugin.store.data_mut().memory.bytes = MemoryCeiling::of(limits).bytes;
        }
    }
}

/// Per-user Wasmtime compilation cache directory.
///
/// `$SPECFORGE_WASMTIME_CACHE` overrides; setting it to `off` disables the
/// cache. Default: `$HOME/.cache/specforge/wasmtime` (no cache when `HOME`
/// is unset). The test drivers read it, so a child running under another `HOME` shares the cache of the test process
/// (a cache of its own would compile every builtin again).
pub fn user_compile_cache_dir() -> Option<PathBuf> {
    match std::env::var_os("SPECFORGE_WASMTIME_CACHE") {
        Some(v) if v == "off" => None,
        Some(v) => Some(PathBuf::from(v)),
        None => std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join(".cache")
                .join("specforge")
                .join("wasmtime")
        }),
    }
}

/// Configure wasmtime's native on-disk compilation cache (true AOT: compiled
/// machine code is cached and deserialized on later runs, keyed by input
/// bytes + engine config). Covers components — `compile_component` goes
/// through wasmtime's `ModuleCacheEntry` like modules do. The soft size cap
/// keeps the governance constraint (500 MB, size-based eviction) true.
fn enable_compile_cache(config: &mut Config, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut cache_config = CacheConfig::new();
    cache_config.with_directory(dir);
    cache_config.with_files_total_size_soft_limit(500 * 1024 * 1024);
    let cache = Cache::new(cache_config).map_err(|e| e.to_string())?;
    config.cache(Some(cache));
    Ok(())
}

#[cfg(test)]
mod deadline_tests {
    use super::{EPOCH_TICK_MS, ms_to_ticks};

    #[test]
    fn ms_to_ticks_never_undercounts_the_budget() {
        assert_eq!(ms_to_ticks(0), 0, "zero budget means trap immediately");
        // One tick for the unknown phase of the first tick, plus the
        // intervals the budget covers (rounded up).
        assert_eq!(ms_to_ticks(1), 2);
        assert_eq!(ms_to_ticks(EPOCH_TICK_MS), 2);
        assert_eq!(ms_to_ticks(EPOCH_TICK_MS + 1), 3, "partial ticks round up");
        assert_eq!(ms_to_ticks(30_000), 3_001);
        // n ticks guarantee (n - 1) full intervals: always at least the budget.
        for ms in [1, 9, 10, 11, 49, 50, 51, 5_000] {
            assert!((ms_to_ticks(ms) - 1) * EPOCH_TICK_MS >= ms, "{ms} ms");
        }
    }
}
