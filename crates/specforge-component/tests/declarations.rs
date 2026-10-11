//! The wire declaration of every builtin and of the SDK greet fixture,
//! pinned byte for byte: each blob's `__handshake` answer and its
//! `__describe` answer for every category the protocol supports equal the
//! generated files under `tests/declarations/<dir>/`.
//!
//! A change that is meant to change a declaration re-vendors the blobs,
//! then regenerates the files (`cargo run -p xtask --bin snapshot-builtins`)
//! and shows the diff for review. Byte identity also guards against a
//! dependency enabling `serde_json/preserve_order` in a guest, which would
//! reorder every key.

use std::path::{Path, PathBuf};

use specforge_component::ComponentRuntime;
use specforge_component::builtins::BUILTIN_EXTENSIONS;
use specforge_protocol_types::{DECLARED_CATEGORIES, PROTOCOL_VERSION};
use specforge_wasm::runtime::{WasmCallResult, WasmRuntime};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives in <root>/crates/")
        .to_path_buf()
}

fn answer(runtime: &ComponentRuntime, extension: &str, export: &str, request: &[u8]) -> Vec<u8> {
    match runtime.call_export(extension, export, request) {
        WasmCallResult::Ok(bytes) => bytes,
        WasmCallResult::Trap(trap) => panic!(
            "{extension} {export} {}: {}: {}",
            String::from_utf8_lossy(request),
            trap.kind,
            trap.message
        ),
    }
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "every builtin's handshake and describe answers match their pinned snapshot byte for byte"
)]
fn builtin_declarations_match_their_snapshots() {
    let root = repo_root();
    let runtime = ComponentRuntime::new();
    let mut extensions: Vec<(String, String)> = Vec::new();
    for (name, bytes) in BUILTIN_EXTENSIONS {
        runtime.load(name, bytes).expect("builtin loads");
        let dir = name.rsplit('/').next().unwrap_or(name).to_string();
        extensions.push((name.to_string(), dir));
    }
    let greet = std::fs::read(root.join("fixtures/greet-extension/greet.wasm"))
        .expect("vendored greet component blob");
    runtime.load("@sdk/greet", &greet).expect("greet loads");
    extensions.push(("@sdk/greet".to_string(), "greet".to_string()));

    let handshake_request = serde_json::to_vec(&serde_json::json!({
        "host_version": PROTOCOL_VERSION,
    }))
    .unwrap();
    let snapshots = root.join("crates/specforge-component/tests/declarations");
    let mut compared = 0;
    let mut stale = Vec::new();
    for (name, dir) in &extensions {
        let mut answers = vec![(
            "handshake.json".to_string(),
            answer(&runtime, name, "__handshake", &handshake_request),
        )];
        for category in DECLARED_CATEGORIES {
            let request = serde_json::to_vec(&serde_json::json!({ "category": category })).unwrap();
            answers.push((
                format!("describe_{category}.json"),
                answer(&runtime, name, "__describe", &request),
            ));
        }
        for (file, bytes) in answers {
            let path = snapshots.join(dir).join(&file);
            let pinned = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("missing snapshot {}: {e}", path.display()));
            if pinned != bytes {
                stale.push(format!("{dir}/{file}"));
            }
            compared += 1;
        }
    }
    assert!(
        stale.is_empty(),
        "these wire answers differ from their pinned snapshots: {stale:?}. \
         If the change is intended, regenerate them with \
         `cargo run -p xtask --bin snapshot-builtins` and review the diff"
    );
    assert_eq!(compared, 10 * (1 + DECLARED_CATEGORIES.len()));
}
