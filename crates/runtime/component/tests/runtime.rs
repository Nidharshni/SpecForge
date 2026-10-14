//! ComponentRuntime end-to-end: load a real wasip2 component, call through
//! the bridge world, exercise reload/unload semantics (hardening-plan W2 +
//! H1 parity).

use specforge_component::ComponentRuntime;
use specforge_wasm::runtime::WasmCallResult;
use specforge_wasm::runtime::WasmRuntime;

fn component_path() -> std::path::PathBuf {
    specforge_component::committed::BRIDGE.path()
}

fn loaded() -> ComponentRuntime {
    let runtime = ComponentRuntime::new();
    let bytes = std::fs::read(component_path()).expect("fixture component blob");
    runtime
        .load("@test/component", &bytes)
        .expect("fixture component instantiates");
    runtime
}

#[test]
fn bridge_call_handshake_roundtrip() {
    let runtime = loaded();
    let result = runtime.call_export("@test/component", "__handshake", b"");
    match result {
        WasmCallResult::Ok(bytes) => {
            let text = String::from_utf8(bytes).unwrap();
            assert!(text.contains("\"name\":\"@test/component\""), "got: {text}");
        }
        WasmCallResult::Trap(t) => panic!("handshake trapped: {t:?}"),
    }
}

#[test]
fn echo_receives_input_bytes() {
    let runtime = loaded();
    let payload = b"payload-bytes-123";
    let result = runtime.call_export("@test/component", "__echo", payload);
    match result {
        WasmCallResult::Ok(bytes) => assert_eq!(bytes, payload),
        WasmCallResult::Trap(t) => panic!("echo trapped: {t:?}"),
    }
}

#[test]
fn guest_error_surfaces_as_trap() {
    let runtime = loaded();
    let result = runtime.call_export("@test/component", "__does_not_exist", b"");
    match result {
        WasmCallResult::Trap(t) => {
            assert_eq!(t.kind, "guest_error");
            assert!(t.message.contains("unknown export"));
        }
        WasmCallResult::Ok(_) => panic!("unknown export must trap"),
    }
}

#[test]
fn unknown_extension_traps() {
    let runtime = loaded();
    let result = runtime.call_export("@no/such", "__handshake", b"");
    assert!(matches!(result, WasmCallResult::Trap(_)));
}

#[test]
fn reload_swaps_and_unload_drops() {
    let runtime = loaded();
    assert!(
        runtime
            .loaded_names()
            .contains(&"@test/component".to_string())
    );

    let bytes = std::fs::read(component_path()).unwrap();
    runtime
        .load("@test/component", &bytes)
        .expect("reload succeeds");
    let result = runtime.call_export("@test/component", "__handshake", b"");
    assert!(matches!(result, WasmCallResult::Ok(_)));

    assert!(runtime.unload("@test/component"));
    assert!(!runtime.unload("@test/component"));
    let result = runtime.call_export("@test/component", "__handshake", b"");
    match result {
        WasmCallResult::Trap(t) => assert_eq!(t.kind, "extension_not_found"),
        WasmCallResult::Ok(_) => panic!("unloaded component must not answer calls"),
    }
}

/// The component runtime keeps the contract every adapter of the port keeps
/// for the modules it holds: load bytes by name, rename, unload.
#[test]
fn the_component_runtime_keeps_the_module_contract() {
    // An extension that declares its own name (the test component echoes
    // the name it is called as).
    let greet = specforge_component::committed::GREET.path();
    let bytes = std::fs::read(greet).expect("the greet fixture is vendored");

    specforge_wasm::testing::assert_module_contract(&ComponentRuntime::new(), &bytes, "@sdk/greet");
}
