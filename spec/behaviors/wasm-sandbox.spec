// Wasm sandbox enforcement and configuration, compile cache, session runtime reuse

use "invariants/wasm"
use "ports/outbound"
use "types/config"
use "types/errors"
use "types/wasm"

behavior enforce_wasm_sandbox "Enforce Wasm Sandbox" {
  features   [wasm_extension_runtime]
  invariants [wasm_sandbox_integrity, extension_isolation]
  category   command
  types      [SandboxPolicy, ExtensionError, WasmTrapInfo]
  ports      [WasmRuntime]
  requires {
    sandbox_policy_configured "the extension's limits have been computed from its handshake via configure_sandbox_policy"
    wasm_runtime_available    "WasmRuntime port is available for enforcement"
  }
  ensures {
    no_capability_granted   "a component is granted no capability: no preopened directory, environment, arguments or stdin, no socket and no name lookup, whatever its declaration asks for"
    memory_limit_enforced   "the instance's linear memory cannot grow past the extension's memory limit; a growth past it traps the call (memory_limit_exceeded)"
    execution_time_enforced "each call is held to the extension's wall-clock limit by epoch interruption (deadline_exceeded) and to the host's fuel budget, given whole to every call (fuel_exhausted)"
    deadline_never_early    "a call is never interrupted before its max_execution_ms budget has elapsed"
    violations_trapped      "a call that crosses a limit traps and fails with E028 naming the limit's kind; the extension's next call gets a fresh instance under the same limits"
  }
  contract   """
    The component runtime MUST grant a component no capability: its WASI
    context preopens no directory and passes no environment, arguments or
    stdin; stdout and stderr are discarded; TCP, UDP and name lookup are
    refused. Clocks and randomness are WASI's own. This holds for every
    export of the instance, whatever the extension's declaration asks for
    (ADR 0037). The runtime MUST hold every call to the extension's limits
    (configure_sandbox_policy): its linear memory cannot grow past
    max_memory_mb, a growth past it trapping the call
    (memory_limit_exceeded); its wall-clock time is bounded by epoch
    interruption (max_execution_ms, checked by a background ticker every
    10 ms; deadline_exceeded) and its instructions by fuel metering, the
    host's whole fuel budget given to every call (fuel_exhausted). The
    wall-clock deadline MUST never interrupt a call before its budget has
    elapsed, whatever the ticker's phase when the call starts, and SHOULD
    overshoot it by no more than two ticks plus scheduling delay. A call
    that crosses a limit fails with E028, its message naming the limit;
    the extension's next call gets a fresh instance under the same limits.
  """
  produces   []
  verify unit "memory limit enforced via linear memory cap"
  verify unit "every call gets the whole fuel budget, and a call that spends it traps as fuel_exhausted"
  verify unit "the execution deadline never interrupts a call before its budget"
  verify unit "an export reaches no directory: the root and a directory it is told about can be neither listed, read nor written"
  verify unit "an export reaches no network: it can neither connect to a listening port nor resolve a name"
  verify unit "a call that crosses a limit traps with the limit's kind, and the next call gets a fresh instance under the same limits"
  verify unit "an extension waiting on a timer is stopped instead of blocking the host"
  verify unit "an extension that cannot be restarted after a trap is unloaded, and its next call says why"
  verify contract "Enforce Wasm Sandbox: Wasm sandbox enforcement holds — sandbox_policy_configured, wasm_runtime_available, no_capability_granted, memory_limit_enforced, execution_time_enforced, deadline_never_early, violations_trapped"
}

behavior compile_wasm_component_with_cache "Compile Wasm Component With Cache" {
  features   [wasm_performance_optimization]
  invariants [wasm_compile_cache_integrity]
  category   command
  types      [ExtensionDeclaration]
  ports      [WasmRuntime, FileSystem]
  requires {
    component_binary_available "component .wasm binary exists and is readable"
    cache_dir_resolved         "compile cache directory resolved: SPECFORGE_WASMTIME_CACHE if set, else $HOME/.cache/specforge/wasmtime; 'off' disables"
  }
  ensures {
    engine_configured_at_construction "the compile cache is configured when the runtime engine is built, before any component compiles"
    first_compile_populates_cache     "first compile of a binary writes its compiled artifact to the cache directory"
    cache_hit_skips_compilation       "a later engine over the same cache directory deserializes the artifact instead of recompiling"
    cache_failure_degrades            "an unwritable or corrupted cache degrades to uncached compilation with a warning, never a load failure"
  }
  contract   """
    The runtime engine (wasmtime) MUST be constructed with its native
    on-disk compilation cache when SPECFORGE_WASMTIME_CACHE selects a
    directory (default: $HOME/.cache/specforge/wasmtime; the value 'off'
    disables the cache). Compiled machine code MUST be cached and
    deserialized on later loads, keyed by the engine configuration and
    component bytes. Cache corruption or an unusable cache directory MUST
    degrade to uncached compilation with a warning. Installed-binary
    integrity is a separate concern enforced by the lockfile hash pin
    (E070) at load time.
  """
  verify unit "first compile populates the compile cache directory"
  verify unit "second engine over the same cache dir loads via cache and executes"
  verify unit "unwritable cache dir degrades to uncached compile with warning"
  verify unit "tampered installed binary refused via E070 lockfile pin"
  verify contract "Compile Wasm Component With Cache: wasm compile cache holds — component_binary_available, cache_dir_resolved, engine_configured_at_construction, first_compile_populates_cache, cache_hit_skips_compilation, cache_failure_degrades"
}

behavior reuse_session_runtime "Reuse Session Runtime" {
  features   [wasm_performance_optimization]
  invariants [extension_isolation]
  category   command
  types      [WasmTrapInfo]
  ports      [WasmRuntime]
  requires {
    session_context        "the process is a CLI run, an LSP session, or an MCP server session"
    wasm_runtime_available "the session's ComponentRuntime is available to all compilation stages"
  }
  ensures {
    single_engine_per_session     "one runtime engine is constructed per run/session and shared by every stage"
    plugin_instances_reused       "loaded component instances are reused across repeated calls without re-instantiation"
    instance_replaced_atomically  "reloading an extension atomically replaces its loaded instance"
    instances_dropped_on_shutdown "all instances are dropped when the runtime is dropped at session end"
  }
  contract   """
    Each process MUST construct a single ComponentRuntime and share it
    across compilation stages (CLI pipeline, LSP state, MCP server).
    Loaded component instances live in the runtime and MUST be reused for
    subsequent calls. Hot reload MUST atomically replace an extension's
    loaded instance. No cross-process warm pool is promised: a new
    process pays component compilation once per binary (mitigated by the
    on-disk compile cache) and instances end with the session.
  """
  verify unit "same runtime instance serves repeated calls without re-instantiation"
  verify unit "hot reload atomically replaces a loaded component"
  verify unit "runtime dropped at session end releases all instances"
  verify contract "Reuse Session Runtime: session runtime reuse holds — session_context, wasm_runtime_available, single_engine_per_session, plugin_instances_reused, instance_replaced_atomically, instances_dropped_on_shutdown"
}

// -- Error Recovery -----

// -- Compile Cache -----
// The compile cache is owned by the runtime engine (wasmtime): entries are
// keyed by bytes + engine config and validated by the engine itself, so
// there is no host-side invalidation behavior. See
// compile_wasm_component_with_cache and the wasm_compile_cache_integrity
// invariant.

behavior configure_sandbox_policy "Configure Sandbox Policy" {
  features   [wasm_extension_runtime]
  invariants [wasm_sandbox_integrity]
  category   command
  types      [SandboxPolicy, ExtensionDeclaration]
  ports      [WasmRuntime]
  requires {
    handshake_read "the extension's handshake answered, with its sandbox_policy as sent (absent or null when it declares none)"
  }
  ensures {
    limits_held_to_ceiling      "each declared limit (max_execution_ms, max_memory_mb) is applied as declared, held to the host's ceiling of 30000 ms and 512 MB; an undeclared limit is the ceiling"
    above_ceiling_warned        "a declared limit above the ceiling is W153, naming the limit and the ceiling it is held to"
    capabilities_never_granted  "a sandbox_policy key other than the two limits whose value asks for something, and a surface's sandbox override, is W153: the host grants no capability"
    limits_applied_at_handshake "the limits hold every call after the handshake is read, on every path that loads an extension's declaration; a handshake call alone applies none"
  }
  contract   """
    When the host loads an extension's declaration it MUST compute the
    extension's limits from the handshake's sandbox_policy and apply them
    to the extension's later calls (the WasmRuntime port's apply_limits;
    the component runtime enforces them, enforce_wasm_sandbox). The policy
    declares limits only (ADR 0037): max_execution_ms, a call's wall-clock
    budget, and max_memory_mb, the instance's linear memory. A declared
    limit is applied as declared, never above the host's ceiling (30000 ms,
    512 MB), which is also the limit of an extension that declares none:
    an extension may tighten its sandbox, never widen it. A declared limit
    above the ceiling MUST be W153. The host grants a component no
    capability, so a sandbox_policy key other than the two limits whose
    value asks for something (true, a non-zero number, a non-empty string,
    list or object: network_access, file_system_access, allowed_domains,
    allowed_paths and allowed_output_extensions from a guest built before
    ADR 0037, or a misspelled limit) and a surface's sandbox override MUST
    be W153 and grant nothing; a key whose value asks for nothing is not
    reported. The warnings are load warnings: check, the LSP and MCP
    report them with the environment's diagnostics, and specforge publish
    and specforge extension validate show them to the extension's author.
    Each warning names the extension by the name its handshake declares,
    whatever key it was loaded under (a candidate's, a .wasm file entry's).
    No project-level override exists: the ceiling bounds every extension.
  """
  produces   []
  verify unit "an extension declaring no sandbox policy runs under the host's ceiling"
  verify unit "a declared limit below the ceiling is applied as declared"
  verify unit "a declared limit above the ceiling is held to it, with W153"
  verify unit "a sandbox_policy key that asks for a capability is W153"
  verify unit "a sandbox_policy key that asks for nothing is not reported"
  verify unit "a surface's sandbox override is W153"
  verify unit "a load's warnings name the extension by the name its handshake declares"
  verify contract "Configure Sandbox Policy: sandbox policy configuration holds — handshake_read, limits_held_to_ceiling, above_ceiling_warned, capabilities_never_granted, limits_applied_at_handshake"
}
