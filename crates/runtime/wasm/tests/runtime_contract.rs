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
