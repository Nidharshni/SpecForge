//! The limits the component runtime holds an extension to (ADR 0037).
//!
//! The guests are hand-written `specforge:bridge` components (like
//! `epoch_deadline.rs`'s spin component), built by [`bridge_component`]:
//! each call spins a counted loop, grows linear memory, and answers a
//! fixed string.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod probe_support;

use std::time::{Duration, Instant};

use probe_support::{ALL, Bait, PROBE, granted_nothing, probe_runtime};
use serde_json::json;
use specforge_component::ComponentRuntime;
use specforge_protocol_types::{CommandInput, RawGraph};
use specforge_test_macros::test as specforge_test;
use specforge_wasm::runtime::{WasmCallResult, WasmRuntime};
use specforge_wasm::{ExtensionCalls, Limits};

/// The handshake of `@repro/grow` declaring a memory limit of `mb` MB: the
/// answer of every export of the growing guest.
fn declares_memory_limit(mb: u32) -> String {
    format!(
        r#"{{"protocol_version":"1.0.0","name":"@repro/grow","version":"0.1.0","contribution_flags":{{}},"peer_dependencies":[],"sandbox_policy":{{"max_memory_mb":{mb}}}}}"#
    )
}

/// The handshake of `@repro/grow` declaring no sandbox policy.
const DECLARES_NOTHING: &str = r#"{"protocol_version":"1.0.0","name":"@repro/grow","version":"0.1.0","contribution_flags":{},"peer_dependencies":[]}"#;

/// A runtime serving `@repro/grow`, a guest growing by `pages` per call,
/// whose handshake has been read and its limits applied.
fn grower(pages: u32, handshake: &str) -> ComponentRuntime {
    let runtime = ComponentRuntime::new();
    runtime
        .load("@repro/grow", &bridge_component(pages, 0, false, handshake))
        .expect("the growing guest instantiates");
    let handshake = ExtensionCalls::new(&runtime)
        .handshake("@repro/grow")
        .expect("the handshake answers");
    // Reading the handshake applies nothing: the loader does, so does this.
    runtime.apply_limits("@repro/grow", handshake.sandbox.limits);
    runtime
}

/// A command input over an empty graph, for a call through
/// `ExtensionCalls::run_command`.
fn empty_command_input() -> CommandInput<RawGraph> {
    CommandInput {
        graph: RawGraph::new(r#"{"nodes":[],"edges":[]}"#.to_string()).unwrap(),
        ..CommandInput::default()
    }
}

fn trap_message(result: &WasmCallResult) -> &str {
    match result {
        WasmCallResult::Trap(trap) => &trap.message,
        WasmCallResult::Ok(_) => panic!("expected a trap, got Ok"),
    }
}

/// A `specforge:bridge` component whose every export (its `call` func, the
/// canonical-ABI signature `(name, export-name, input) -> result<list<u8>,
/// string>`):
///
/// - runs a counting loop of `spin` iterations;
/// - grows its linear memory by `pages` 64 KiB pages, writing every byte of
///   them when `fill` is set (so the growth costs real memory; without it
///   the pages stay virtual and a test is cheap);
/// - answers `Ok(answer)`.
fn bridge_component(pages: u32, spin: u32, fill: bool, answer: &str) -> Vec<u8> {
    let fill_pages = if fill {
        "(memory.fill (i32.mul (local.get $old) (i32.const 65536)) (i32.const 1)
          (i32.mul (i32.const PAGES) (i32.const 65536)))"
    } else {
        ""
    };
    let body = r#"
      (local.set $left (i32.const SPIN))
      (block $done
        (loop $again
          (br_if $done (i32.eqz (local.get $left)))
          (local.set $left (i32.sub (local.get $left) (i32.const 1)))
          (br $again)))
      (if (i32.ne (i32.const PAGES) (i32.const 0))
        (then
          (local.set $old (memory.grow (i32.const PAGES)))
          FILL))"#
        .replace("FILL", fill_pages)
        .replace("PAGES", &pages.to_string())
        .replace("SPIN", &spin.to_string());
    custom_bridge("", "", &body, answer)
}

/// A `specforge:bridge` component whose `call` runs `body` (wat
/// instructions, with the locals `$left` and `$old` free to use), then
/// answers `Ok(answer)`. `outside` is component-level wat placed before the
/// core module (imports, other core modules and instances). `host` is wat
/// placed first in the core module: its imports, all from a module `"host"`
/// that `outside` defines as the core instance `$host`, then any other
/// declaration.
///
/// The result is lowered through a return area at offset 0: the `ok`
/// discriminant (0), the answer's pointer (16) and its length. The answer
/// itself sits at offset 16 and the bump allocator (`realloc`, for the
/// arguments the host lowers) starts at 8192.
fn custom_bridge(outside: &str, host: &str, body: &str, answer: &str) -> Vec<u8> {
    let escaped: String = answer.bytes().map(|byte| format!("\\{byte:02x}")).collect();
    let with = if outside.contains("(core instance $host") {
        r#"(with "host" (instance $host))"#
    } else {
        ""
    };
    let wat = r#"
(component
  (type $call_sig
    (func (param "name" string) (param "export-name" string) (param "input" (list u8))
      (result (result (list u8) (error string)))))
  OUTSIDE
  (core module $core
    IMPORTS
    (memory (export "memory") 1)
    (data (i32.const 16) "ANSWER")
    (global $heap (mut i32) (i32.const 8192))
    (func $realloc (export "realloc")
      (param $old_ptr i32) (param $old_size i32) (param $align i32) (param $new_size i32)
      (result i32)
      (local $ret i32)
      (local.set $ret
        (i32.and
          (i32.add (i32.add (global.get $heap) (local.get $align)) (i32.const 15))
          (i32.const -16)))
      (global.set $heap (i32.add (local.get $ret) (local.get $new_size)))
      (local.get $ret))
    (func (export "call") (param i32 i32 i32 i32 i32 i32) (result i32)
      (local $left i32)
      (local $old i32)
      BODY
      (i32.store (i32.const 0) (i32.const 0))
      (i32.store (i32.const 4) (i32.const 16))
      (i32.store (i32.const 8) (i32.const ANSWER_LEN))
      (i32.const 0)))
  (core instance $inst (instantiate $core WITH))
  (func $lifted (type $call_sig)
    (canon lift
      (core func $inst "call")
      (memory (core memory $inst "memory"))
      (realloc (core func $inst "realloc"))
      string-encoding=utf8))
  (export "call" (func $lifted)))
"#
    .replace("OUTSIDE", outside)
    .replace("IMPORTS", host)
    .replace("WITH", with)
    .replace("BODY", body)
    .replace("ANSWER_LEN", &answer.len().to_string())
    .replace("ANSWER", &escaped);
    wat::parse_str(wat).expect("the bridge component's wat parses")
}

fn answered(result: &WasmCallResult) -> bool {
    matches!(result, WasmCallResult::Ok(_))
}

fn trap_kind(result: &WasmCallResult) -> &str {
    match result {
        WasmCallResult::Trap(trap) => &trap.kind,
        WasmCallResult::Ok(_) => panic!("expected a trap, got Ok"),
    }
}

#[specforge_test(
    behavior = "enforce_wasm_sandbox",
    verify = "memory limit enforced via linear memory cap"
)]
#[specforge_test(
    constraint = "wasm_memory_limit",
    verify = "an extension growing past its memory limit traps"
)]
#[specforge_test(
    invariant = "wasm_sandbox_integrity",
    verify = "sandbox violation traps the extension and emits a diagnostic"
)]
#[specforge_test(
    failure_mode = "wasm_memory_exhaustion",
    verify = "Wasm Memory Exhaustion failure mode is handled"
)]
fn an_extension_growing_past_its_memory_limit_traps() {
    // The handshake runs under the ceiling (its limits are not known yet);
    // the 32 MB it declares then binds every later growth.
    let runtime = grower(1024, &declares_memory_limit(32));
    for call in 1..=2 {
        let result = runtime.call_export("@repro/grow", "anything", b"{}");
        assert_eq!(
            trap_kind(&result),
            "memory_limit_exceeded",
            "call {call}: {result:?}"
        );
        assert!(
            trap_message(&result).contains("32 MB"),
            "the trap names the limit: {}",
            trap_message(&result)
        );
    }

    let error = ExtensionCalls::new(&runtime)
        .run_command("@repro/grow", "cmd__grow", &empty_command_input())
        .expect_err("the command's growth traps");
    assert_eq!(error.diagnostic().code, "E028");
    assert!(
        error.to_string().contains("memory_limit_exceeded"),
        "{error}"
    );
}

#[specforge_test(
    constraint = "wasm_memory_limit",
    verify = "an extension declaring no memory limit is held to the 512 MB ceiling"
)]
fn an_extension_declaring_no_memory_limit_is_held_to_the_ceiling() {
    // 128 MiB per call: the handshake, call 1 and call 2 reach 128, 256 and
    // 384 MiB; call 3 would pass 512 MiB.
    let runtime = grower(2048, DECLARES_NOTHING);
    for call in 1..=2 {
        let result = runtime.call_export("@repro/grow", "anything", b"{}");
        assert!(answered(&result), "call {call} answers: {result:?}");
    }
    let third = runtime.call_export("@repro/grow", "anything", b"{}");
    assert_eq!(trap_kind(&third), "memory_limit_exceeded", "{third:?}");
    assert!(trap_message(&third).contains("512 MB"), "{third:?}");
}

#[specforge_test(
    behavior = "enforce_wasm_sandbox",
    verify = "every call gets the whole fuel budget, and a call that spends it traps as fuel_exhausted"
)]
fn every_call_gets_the_whole_fuel_budget() {
    // Each call spends about a third of the budget, so a budget shared by
    // the instance's calls would run out on the third.
    let runtime = ComponentRuntime::new().with_fuel_limit(1_000_000);
    runtime
        .load("@repro/spin", &bridge_component(0, 50_000, false, "{}"))
        .expect("the spinning guest instantiates");
    for call in 1..=10 {
        let result = runtime.call_export("@repro/spin", "anything", b"{}");
        assert!(answered(&result), "call {call} answers: {result:?}");
    }

    // One call that spends more than the whole budget traps, naming it.
    let runtime = ComponentRuntime::new().with_fuel_limit(1_000_000);
    runtime
        .load("@repro/spin", &bridge_component(0, 400_000, false, "{}"))
        .expect("the spinning guest instantiates");
    let result = runtime.call_export("@repro/spin", "anything", b"{}");
    assert_eq!(trap_kind(&result), "fuel_exhausted", "{result:?}");
    assert!(
        trap_message(&result).contains("1000000"),
        "the trap names the budget: {}",
        trap_message(&result)
    );
}

#[specforge_test(
    behavior = "enforce_wasm_sandbox",
    verify = "a call that crosses a limit traps with the limit's kind, and the next call gets a fresh instance under the same limits"
)]
fn a_limit_trap_names_its_limit() {
    let runtime = ComponentRuntime::new();
    runtime
        .load("@repro/spin", &bridge_component(0, u32::MAX, false, "{}"))
        .expect("the spinning guest instantiates");
    runtime.apply_limits(
        "@repro/spin",
        Limits {
            execution_ms: 50,
            ..Limits::CEILING
        },
    );
    for call in 1..=2 {
        let result = runtime.call_export("@repro/spin", "anything", b"{}");
        assert_eq!(
            trap_kind(&result),
            "deadline_exceeded",
            "call {call}: {result:?}"
        );
        assert!(
            trap_message(&result).contains("50 ms"),
            "the trap names the limit: {}",
            trap_message(&result)
        );
    }
}

/// The sandbox contract end to end: nothing is granted, and the three
/// limits trap.
#[specforge_test(
    behavior = "enforce_wasm_sandbox",
    verify = "Enforce Wasm Sandbox: Wasm sandbox enforcement holds — sandbox_policy_configured, wasm_runtime_available, no_capability_granted, memory_limit_enforced, execution_time_enforced, deadline_never_early, violations_trapped"
)]
fn the_sandbox_contract_holds() {
    // no_capability_granted: the probe is told about a directory and a port.
    let probe = probe_runtime();
    let bait = Bait::new();
    let report = ExtensionCalls::new(&probe)
        .call_mcp_tool(
            PROBE,
            "mcp__probe_tool",
            &json!({"dir": bait.dir(), "port": bait.port()}),
        )
        .expect("the probe answers");
    granted_nothing(&report, ALL);
    bait.untouched();

    // memory_limit_enforced, violations_trapped
    let runtime = grower(1024, &declares_memory_limit(32));
    let result = runtime.call_export("@repro/grow", "anything", b"{}");
    assert_eq!(trap_kind(&result), "memory_limit_exceeded", "{result:?}");

    // execution_time_enforced: the instruction budget
    let runtime = ComponentRuntime::new().with_fuel_limit(1_000_000);
    runtime
        .load("@repro/spin", &bridge_component(0, 400_000, false, "{}"))
        .unwrap();
    let result = runtime.call_export("@repro/spin", "anything", b"{}");
    assert_eq!(trap_kind(&result), "fuel_exhausted", "{result:?}");

    // execution_time_enforced, deadline_never_early: the wall clock
    let runtime = ComponentRuntime::new();
    runtime
        .load("@repro/spin", &bridge_component(0, u32::MAX, false, "{}"))
        .unwrap();
    runtime.apply_limits(
        "@repro/spin",
        Limits {
            execution_ms: 50,
            ..Limits::CEILING
        },
    );
    let start = Instant::now();
    let result = runtime.call_export("@repro/spin", "anything", b"{}");
    let elapsed = start.elapsed();
    assert_eq!(trap_kind(&result), "deadline_exceeded", "{result:?}");
    assert!(
        elapsed >= Duration::from_millis(50),
        "trapped before the budget elapsed: {elapsed:?}"
    );
}

/// A runtime serving `@repro/limits`, a custom bridge guest held to a
/// memory limit of 32 MB. Its handshake is not called, so `body` runs only
/// in the calls a test makes.
fn custom_guest(outside: &str, host: &str, body: &str) -> Result<ComponentRuntime, String> {
    guest_held_to(32, outside, host, body)
}

/// [`custom_guest`] held to a memory limit of `mb` MB.
fn guest_held_to(
    mb: u32,
    outside: &str,
    host: &str,
    body: &str,
) -> Result<ComponentRuntime, String> {
    let runtime = ComponentRuntime::new();
    runtime.load("@repro/limits", &custom_bridge(outside, host, body, "{}"))?;
    runtime.apply_limits(
        "@repro/limits",
        Limits {
            memory_mb: mb,
            ..Limits::CEILING
        },
    );
    Ok(runtime)
}

/// A core module exporting `grow`, growing its own memory, instantiated as
/// `$other`, and the `$host` instance handing that `grow` to the bridge's
/// core module. `memory` is the other module's memory declaration.
fn second_memory(memory: &str) -> String {
    r#"
  (core module $other_module
    MEMORY
    (func (export "grow") (param i32) (result i32) (memory.grow (local.get 0))))
  (core instance $other (instantiate $other_module))
  (core instance $host (export "grow" (func $other "grow")))"#
        .replace("MEMORY", memory)
}

#[specforge_test(
    constraint = "wasm_memory_limit",
    verify = "the memory limit holds for every memory of an extension together"
)]
fn the_memory_limit_counts_every_memory_together() {
    // Two memories of 24 MiB each: neither passes 32 MB alone, together
    // they do.
    let runtime = custom_guest(
        &second_memory("(memory 1)"),
        r#"(import "host" "grow" (func $grow_other (param i32) (result i32)))"#,
        "(drop (memory.grow (i32.const 384))) (drop (call $grow_other (i32.const 384)))",
    )
    .expect("the two-memory guest instantiates");
    let result = runtime.call_export("@repro/limits", "anything", b"{}");
    assert_eq!(trap_kind(&result), "memory_limit_exceeded", "{result:?}");
}

#[specforge_test(
    constraint = "wasm_memory_limit",
    verify = "an extension cannot grow a shared memory past its memory limit"
)]
fn a_shared_memory_cannot_pass_the_memory_limit() {
    // 64 MiB of shared memory under a 32 MB limit: refused at load (the
    // engine does not enable shared memory), or trapped when it grows.
    let loaded = custom_guest(
        &second_memory("(memory 1 65536 shared)"),
        r#"(import "host" "grow" (func $grow_other (param i32) (result i32)))"#,
        "(drop (call $grow_other (i32.const 1024)))",
    );
    if let Ok(runtime) = loaded {
        let result = runtime.call_export("@repro/limits", "anything", b"{}");
        assert_eq!(trap_kind(&result), "memory_limit_exceeded", "{result:?}");
    }
}

#[specforge_test(
    constraint = "wasm_memory_limit",
    verify = "an extension cannot grow a table without bound"
)]
fn a_table_cannot_grow_without_bound() {
    // A table declaring no maximum, grown by ten million entries at once.
    let runtime = custom_guest(
        "",
        "(table $table 1 funcref)",
        "(drop (table.grow $table (ref.null func) (i32.const 10000000)))",
    )
    .expect("the table guest instantiates");
    let result = runtime.call_export("@repro/limits", "anything", b"{}");
    assert_eq!(trap_kind(&result), "memory_limit_exceeded", "{result:?}");
}

#[specforge_test(
    behavior = "enforce_wasm_sandbox",
    verify = "an extension waiting on a timer is stopped instead of blocking the host"
)]
fn an_extension_waiting_on_a_timer_does_not_block_the_host() {
    // The guest asks WASI for a 5 s timer and blocks on it. Epoch
    // interruption only fires while guest code runs, so the wait itself
    // must be refused.
    let clock = r#"
  (import "wasi:io/poll@0.2.0" (instance $poll
    (export "pollable" (type (sub resource)))
    (export "[method]pollable.block" (func (param "self" (borrow 0))))))
  (alias export $poll "pollable" (type $pollable))
  (import "wasi:clocks/monotonic-clock@0.2.0" (instance $clock
    (alias outer 1 $pollable (type))
    (export "pollable" (type (eq 0)))
    (export "subscribe-duration" (func (param "when" u64) (result (own 1))))))
  (core func $block (canon lower (func $poll "[method]pollable.block")))
  (core func $subscribe (canon lower (func $clock "subscribe-duration")))
  (core instance $host
    (export "block" (func $block))
    (export "subscribe" (func $subscribe)))"#;
    let runtime = custom_guest(
        clock,
        r#"(import "host" "block" (func $block (param i32)))
    (import "host" "subscribe" (func $subscribe (param i64) (result i32)))"#,
        "(call $block (call $subscribe (i64.const 5000000000)))",
    )
    .expect("the sleeping guest instantiates");
    runtime.apply_limits(
        "@repro/limits",
        Limits {
            execution_ms: 100,
            ..Limits::CEILING
        },
    );
    let start = Instant::now();
    let result = runtime.call_export("@repro/limits", "anything", b"{}");
    let elapsed = start.elapsed();
    assert!(!answered(&result), "the wait was not refused: {result:?}");
    assert!(
        elapsed < Duration::from_secs(2),
        "the call blocked the host for {elapsed:?}"
    );
}

#[specforge_test(
    behavior = "enforce_wasm_sandbox",
    verify = "an extension that cannot be restarted after a trap is unloaded, and its next call says why"
)]
fn an_extension_that_cannot_restart_is_unloaded_with_its_reason() {
    // 18 pages (over 1 MiB) at instantiation, which the 512 MB ceiling
    // admits; under its 1 MB limit, the first growth traps and no fresh
    // instance can start.
    let runtime = guest_held_to(
        1,
        &second_memory("(memory 17)"),
        r#"(import "host" "grow" (func $grow_other (param i32) (result i32)))"#,
        "(drop (memory.grow (i32.const 1)))",
    )
    .expect("the guest instantiates under the ceiling");
    let first = runtime.call_export("@repro/limits", "anything", b"{}");
    assert_eq!(trap_kind(&first), "memory_limit_exceeded", "{first:?}");
    assert!(
        trap_message(&first).contains("could not be restarted"),
        "{first:?}"
    );
    let next = runtime.call_export("@repro/limits", "anything", b"{}");
    assert_eq!(trap_kind(&next), "extension_not_found", "{next:?}");
}
