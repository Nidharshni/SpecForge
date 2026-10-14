//! Wasmtime on-disk compile cache wiring (C7-02 closeout).
//!
//! Proves the cache is configured at engine construction and actually
//! populates on first compile — the previous `.aot` side cache was a byte
//! copy no runtime ever consumed.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_component::ComponentRuntime;
use specforge_wasm::runtime::WasmRuntime;

fn component_path() -> std::path::PathBuf {
    specforge_component::committed::BRIDGE.path()
}

#[test]
fn compile_cache_dir_is_populated_on_first_load() {
    let dir = tempfile::TempDir::new().unwrap();
    let cache_dir = dir.path().join("cache");

    let runtime = ComponentRuntime::new_with_compile_cache(cache_dir.clone());
    let bytes = std::fs::read(component_path()).expect("fixture component bytes");
    runtime
        .load("@test/component", &bytes)
        .expect("fixture component compiles");
    let entries: Vec<_> = std::fs::read_dir(&cache_dir)
        .expect("cache dir exists")
        .collect();
    assert!(
        !entries.is_empty(),
        "compile cache must be populated by the first component compile"
    );
}

#[test]
fn second_runtime_reuses_cache_and_still_loads() {
    let dir = tempfile::TempDir::new().unwrap();
    let cache_dir = dir.path().join("cache");

    let first = ComponentRuntime::new_with_compile_cache(cache_dir.clone());
    let bytes = std::fs::read(component_path()).expect("fixture component bytes");
    first
        .load("@test/component", &bytes)
        .expect("first compile");
    // A fresh engine over the same cache dir exercises the deserialize path.
    let second = ComponentRuntime::new_with_compile_cache(cache_dir);
    second
        .load("@test/component", &bytes)
        .expect("cache-hit load");
    let result = second.call_export("@test/component", "__handshake", b"");
    match result {
        specforge_wasm::runtime::WasmCallResult::Ok(bytes) => {
            let text = String::from_utf8(bytes).unwrap();
            assert!(text.contains("\"name\":\"@test/component\""), "got: {text}");
        }
        specforge_wasm::runtime::WasmCallResult::Trap(t) => panic!("handshake trapped: {t:?}"),
    }
}

#[test]
fn unwritable_cache_dir_degrades_to_no_cache_with_warning() {
    // A path under a *file* cannot be created — the runtime must still work.
    let file = tempfile::TempDir::new().unwrap();
    let blocker = file.path().join("blocker");
    std::fs::write(&blocker, b"not a dir").unwrap();

    let runtime = ComponentRuntime::new_with_compile_cache(blocker.join("cache"));
    let bytes = std::fs::read(component_path()).expect("fixture component bytes");
    runtime
        .load("@test/component", &bytes)
        .expect("component compiles even when cache setup fails");
}
