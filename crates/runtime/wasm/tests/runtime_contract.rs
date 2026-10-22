//! The in-process adapter of the `WasmRuntime` port keeps the contract the
//! component runtime keeps (`specforge_wasm::testing::assert_runtime_contract`,
//! run over the component runtime in `specforge-component`'s tests).

use specforge_extension_sdk::prelude::*;
use specforge_wasm::WasmRuntime;
use specforge_wasm::testing::{InProcessRuntime, assert_module_contract, assert_runtime_contract};

/// An extension whose `crash` command panics.
fn crashing() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@contract/crash", "1.0.0"));
    c.command("crash", |cmd| {
        cmd.title("Crash")
            .description("Panics")
            .handler(|_| panic!("the command crashed"));
    });
    c
}

#[specforge_test_macros::test(
    behavior = "call_extension_exports",
    verify = "both runtimes report an unknown extension, an unrouted export, a guest error and a guest panic as traps"
)]
#[specforge_test_macros::test(port = "WasmRuntime", verify = "WasmRuntime contract is satisfied")]
fn the_in_process_runtime_keeps_the_runtime_contract() {
    let runtime = InProcessRuntime::new().with(crashing);
    assert_runtime_contract(&runtime, "@contract/crash", "cmd__crash");
}

#[test]
fn the_in_process_runtime_keeps_the_module_contract() {
    let bytes = b"\0asm in-process contract";
    let runtime = InProcessRuntime::new().binary(bytes, crashing);

    assert_module_contract(&runtime, bytes, "@contract/crash");
}

#[test]
fn a_binary_is_served_under_the_name_it_is_loaded_as() {
    let runtime = InProcessRuntime::new()
        .binary(b"one", crashing)
        .with(|| ContributionsBuilder::new(ExtensionMeta::new("@contract/served", "1.0.0")));

    // Bytes it was not given are no component it serves, under any name.
    assert!(runtime.load("@acme/other", b"two").is_err());
    // The bytes it was given are served under the name they are loaded as,
    // not the one they declare.
    runtime.load("@acme/installed", b"one").unwrap();
    assert!(runtime.unload("@acme/installed"));
    // A name it serves loads as it is, whatever the bytes.
    runtime.load("@contract/served", b"anything").unwrap();
    assert!(runtime.unload("@contract/served"));
    assert!(!runtime.unload("@contract/served"));
}

#[test]
fn a_module_whose_bytes_spell_a_declaration_loads_under_any_name() {
    use specforge_extension_sdk::PeerDependency;
    use specforge_wasm::protocol::load_declaration;
    use specforge_wasm::testing::spelled_module;

    let peers = vec![PeerDependency {
        name: "@acme/b".to_string(),
        version: "^1.0".to_string(),
        optional: false,
    }];
    let bytes = spelled_module("@acme/a", "2.3.4", &peers);
    let runtime = InProcessRuntime::new();

    assert_eq!(
        runtime.declared_by("@acme/x", &bytes),
        Some(("2.3.4".to_string(), peers.clone()))
    );
    runtime.load("@acme/x", &bytes).unwrap();
    let loaded = load_declaration(&runtime, "@acme/x").unwrap();
    assert_eq!(loaded.declaration.name(), "@acme/a");
    assert_eq!(loaded.declaration.version(), "2.3.4");
    assert_eq!(loaded.declaration.peers(), peers.as_slice());
    // Bytes that spell nothing are no component, and declare nothing.
    assert!(runtime.load("@acme/y", b"\0asm other").is_err());
    assert_eq!(runtime.declared_by("@acme/y", b"\0asm other"), None);
}
