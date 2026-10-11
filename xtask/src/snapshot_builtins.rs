//! Pins the wire declaration of every builtin and of the SDK greet fixture:
//! the exact bytes each blob answers to `__handshake` and to `__describe`
//! for every category the protocol defines, written to
//! `crates/specforge-component/tests/declarations/<dir>/{handshake,describe_<category>}.json`.
//!
//! The files are generated, never edited: they are the golden wire fixtures
//! `crates/specforge-component/tests/declarations.rs` compares every blob
//! against, byte for byte. Re-run after a change that is meant to change a
//! declaration (re-vendor first), and review the diff:
//!
//!   cargo run -p xtask --bin snapshot-builtins            write every snapshot
//!   cargo run -p xtask --bin snapshot-builtins -- --check fail if any differs (no write)

use specforge_component::ComponentRuntime;
use specforge_component::builtins::BUILTIN_EXTENSIONS;
use specforge_protocol_types::{DeclaredCategory, PROTOCOL_VERSION};
use specforge_wasm::runtime::{WasmCallResult, WasmRuntime};
use std::path::Path;

/// The SDK fixture pinned beside the builtins, under the directory `greet`.
const GREET: (&str, &str) = ("@sdk/greet", "fixtures/greet-extension/greet.wasm");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = match args.as_slice() {
        [] => false,
        [flag] if flag == "--check" => true,
        other => {
            eprintln!("usage: snapshot-builtins [--check]   (got {other:?})");
            std::process::exit(2);
        }
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace root");
    let out = root.join("crates/specforge-component/tests/declarations");

    let runtime = ComponentRuntime::new();
    let mut extensions: Vec<(String, String)> = Vec::new();
    for (name, bytes) in BUILTIN_EXTENSIONS {
        if let Err(e) = runtime.load(name, bytes) {
            eprintln!("[fail]   {name}: {e}");
            std::process::exit(1);
        }
        let dir = name.rsplit('/').next().unwrap_or(name).to_string();
        extensions.push((name.to_string(), dir));
    }
    let greet = std::fs::read(root.join(GREET.1))
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", GREET.1));
    if let Err(e) = runtime.load(GREET.0, &greet) {
        eprintln!("[fail]   {}: {e}", GREET.0);
        std::process::exit(1);
    }
    extensions.push((GREET.0.to_string(), "greet".to_string()));

    let mut failed = false;
    for (name, dir) in &extensions {
        let dir_path = out.join(dir);
        let answers = match wire_answers(&runtime, name) {
            Ok(answers) => answers,
            Err(e) => {
                eprintln!("[fail]   {dir}: {e}");
                failed = true;
                continue;
            }
        };
        let stale: Vec<&str> = answers
            .iter()
            .filter(|(file, bytes)| std::fs::read(dir_path.join(file)).ok().as_ref() != Some(bytes))
            .map(|(file, _)| file.as_str())
            .collect();
        if check {
            if stale.is_empty() {
                println!("[fresh]  {dir}: {} files match", answers.len());
            } else {
                eprintln!("[stale]  {dir}: {}", stale.join(", "));
                failed = true;
            }
            continue;
        }
        std::fs::create_dir_all(&dir_path)
            .unwrap_or_else(|e| panic!("cannot create {}: {e}", dir_path.display()));
        for (file, bytes) in &answers {
            let path = dir_path.join(file);
            std::fs::write(&path, bytes)
                .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
        }
        println!(
            "[ok]     {dir}: {} files written ({} changed)",
            answers.len(),
            stale.len()
        );
    }
    if failed {
        if check {
            eprintln!(
                "declaration snapshots are stale: cargo run -p xtask --bin snapshot-builtins"
            );
        }
        std::process::exit(1);
    }
}

/// Every wire answer of `extension`, as (file name, exact bytes): its
/// handshake, then its describe answer for each category the protocol defines.
fn wire_answers(
    runtime: &ComponentRuntime,
    extension: &str,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let request = serde_json::json!({
        "host_version": PROTOCOL_VERSION,
    });
    let mut answers = vec![(
        "handshake.json".to_string(),
        answer(runtime, extension, "__handshake", &request)?,
    )];
    for category in DeclaredCategory::ALL {
        let category = category.name();
        let request = serde_json::json!({ "category": category });
        answers.push((
            format!("describe_{category}.json"),
            answer(runtime, extension, "__describe", &request)?,
        ));
    }
    Ok(answers)
}

fn answer(
    runtime: &ComponentRuntime,
    extension: &str,
    export: &str,
    request: &serde_json::Value,
) -> Result<Vec<u8>, String> {
    let input = serde_json::to_vec(request).expect("requests serialize");
    match runtime.call_export(extension, export, &input) {
        WasmCallResult::Ok(bytes) => Ok(bytes),
        WasmCallResult::Trap(trap) => Err(format!(
            "{export} {request}: {}: {}",
            trap.kind, trap.message
        )),
    }
}
