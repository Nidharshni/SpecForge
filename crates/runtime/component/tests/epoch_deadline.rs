//! Wall-clock enforcement (audit C7-10): a plugin's `max_execution_ms` is
//! enforced via wasmtime epoch interruption (background ticker thread +
//! per-call `Store::set_epoch_deadline`), and plugin locking is per-extension
//! so calls into different extensions run concurrently.
//!
//! The guest is a hand-written component whose `call` export spins forever
//! (canonical-ABI signature matching `specforge:bridge`), so an unbounded
//! call would hang the host thread unless the deadline traps it.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use specforge_component::ComponentRuntime;
use specforge_test_macros::test as specforge_test;
use specforge_wasm::Limits;
use specforge_wasm::runtime::{WasmCallResult, WasmRuntime};

/// A component exporting the `specforge:bridge` `call` func whose body is an
/// infinite loop. The trailing constants satisfy the flattened return type
/// (discriminant + ptr + len) for canonical lifting.
const SPIN_COMPONENT_WAT: &str = r#"
(component
  (type $call_sig
    (func (param "name" string) (param "export-name" string) (param "input" (list u8))
      (result (result (list u8) (error string)))))
  (core module $core
    (memory (export "memory") 1)
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
      (loop $spin (br $spin))
      (i32.const 0)))
  (core instance $inst (instantiate $core))
  (func $lifted (type $call_sig)
    (canon lift
      (core func $inst "call")
      (memory (core memory $inst "memory"))
      (realloc (core func $inst "realloc"))
      string-encoding=utf8))
  (export "call" (func $lifted)))
"#;

fn spin_component_bytes() -> Vec<u8> {
    wat::parse_str(SPIN_COMPONENT_WAT).expect("spin component wat parses")
}

fn greet_wasm_bytes() -> Vec<u8> {
    let path = specforge_component::committed::GREET.path();
    std::fs::read(path).expect("vendored greet component blob")
}

fn trap_kind(result: &WasmCallResult) -> String {
    match result {
        WasmCallResult::Trap(trap) => trap.kind.clone(),
        WasmCallResult::Ok(_) => panic!("expected a trap, got Ok"),
    }
}

#[specforge_test(
    behavior = "enforce_wasm_sandbox",
    verify = "the execution deadline never interrupts a call before its budget"
)]
fn execution_deadline_traps_long_running_export() {
    let runtime = ComponentRuntime::new();
    runtime
        .load("@spin", &spin_component_bytes())
        .expect("spin component instantiates");
    runtime.apply_limits(
        "@spin",
        Limits {
            execution_ms: 50,
            ..Limits::CEILING
        },
    );

    let start = Instant::now();
    let result = runtime.call_export("@spin", "__handshake", b"");
    let elapsed = start.elapsed();

    // A 50 ms budget is 5 intervals plus one tick for the ticker's unknown
    // phase: this can only trap if the background ticker thread is actually
    // advancing the engine epoch, and never before the budget has elapsed.
    assert_eq!(trap_kind(&result), "deadline_exceeded");
    assert!(
        elapsed >= Duration::from_millis(50),
        "trapped before the deadline elapsed: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "deadline enforcement took far too long: {elapsed:?}"
    );
}

#[test]
fn zero_deadline_traps_without_ticking() {
    let runtime = ComponentRuntime::new();
    runtime
        .load("@spin", &spin_component_bytes())
        .expect("spin component instantiates");
    runtime.apply_limits(
        "@spin",
        Limits {
            execution_ms: 0,
            ..Limits::CEILING
        },
    );

    // 0 ms -> 0 ticks: the deadline is already met, so the trap fires at the
    // guest's first epoch checkpoint without waiting for the ticker.
    let start = Instant::now();
    let result = runtime.call_export("@spin", "__handshake", b"");
    assert_eq!(trap_kind(&result), "deadline_exceeded");
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "zero deadline should trap immediately, took {:?}",
        start.elapsed()
    );
}

#[test]
fn deadline_is_scoped_to_one_extension() {
    let runtime = ComponentRuntime::new();
    runtime
        .load("@spin", &spin_component_bytes())
        .expect("spin component instantiates");
    runtime
        .load("@sdk/greet", &greet_wasm_bytes())
        .expect("greet component instantiates");
    runtime.apply_limits(
        "@spin",
        Limits {
            execution_ms: 0,
            ..Limits::CEILING
        },
    );

    // The zeroed deadline binds only @spin; @sdk/greet keeps its default
    // budget and still answers.
    assert_eq!(
        trap_kind(&runtime.call_export("@spin", "__handshake", b"")),
        "deadline_exceeded"
    );
    match runtime.call_export("@sdk/greet", "__handshake", b"") {
        WasmCallResult::Ok(_) => {}
        other => panic!("greet should be unaffected by @spin's deadline: {other:?}"),
    }
}

#[test]
fn different_extensions_run_concurrently() {
    let runtime = Arc::new(ComponentRuntime::new());
    runtime
        .load("@spin", &spin_component_bytes())
        .expect("spin component instantiates");
    runtime
        .load("@sdk/greet", &greet_wasm_bytes())
        .expect("greet component instantiates");
    // Long enough that the slow call is definitely still executing while the
    // fast extension is served.
    runtime.apply_limits(
        "@spin",
        Limits {
            execution_ms: 800,
            ..Limits::CEILING
        },
    );

    let slow = {
        let runtime = Arc::clone(&runtime);
        std::thread::spawn(move || runtime.call_export("@spin", "__handshake", b""))
    };
    // Let the slow call get past the map lookup and into the guest loop
    // before the fast call arrives.
    std::thread::sleep(Duration::from_millis(100));

    let fast_start = Instant::now();
    let fast = {
        let runtime = Arc::clone(&runtime);
        std::thread::spawn(move || runtime.call_export("@sdk/greet", "__handshake", b""))
    };
    let fast_result = fast.join().unwrap();
    let fast_elapsed = fast_start.elapsed();

    // Under the old single global plugins mutex this call could only start
    // after @spin's call returned (~800 ms). Per-extension locking serves it
    // while the guest is still spinning.
    assert!(
        matches!(fast_result, WasmCallResult::Ok(_)),
        "fast extension call must succeed: {fast_result:?}"
    );
    assert!(
        fast_elapsed < Duration::from_millis(400),
        "fast extension was blocked behind the slow one: {fast_elapsed:?}"
    );
    assert!(
        !slow.is_finished(),
        "slow call should still be running while the fast one completes"
    );

    let slow_result = slow.join().unwrap();
    assert_eq!(trap_kind(&slow_result), "deadline_exceeded");
}

#[test]
fn same_extension_calls_still_serialize() {
    let runtime = Arc::new(ComponentRuntime::new());
    runtime
        .load("@sdk/greet", &greet_wasm_bytes())
        .expect("greet component instantiates");

    // Concurrent calls into the SAME extension share its Store; the
    // per-extension lock must serialize them without corruption.
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let runtime = Arc::clone(&runtime);
            std::thread::spawn(move || runtime.call_export("@sdk/greet", "__handshake", b""))
        })
        .collect();
    for handle in handles {
        match handle.join().unwrap() {
            WasmCallResult::Ok(_) => {}
            other => panic!("concurrent same-extension call failed: {other:?}"),
        }
    }
}
