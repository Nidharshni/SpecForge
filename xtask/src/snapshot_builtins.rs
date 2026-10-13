//! Pins the wire declaration of every committed SDK blob: the exact bytes each
//! answers to `__handshake` and to `__describe` for every category the protocol
//! defines, written to
//! `crates/specforge-component/tests/declarations/<dir>/{handshake,describe_<category>}.json`.
//!
//! The files are generated, never edited: they are the golden wire fixtures
//! `crates/specforge-component/tests/declarations.rs` compares every blob
//! against, byte for byte. The list of blobs is
//! `specforge_component::committed::COMMITTED`. Re-run after a change that is
//! meant to change a declaration (re-vendor first), and review the diff:
//!
//!   cargo run -p xtask --bin snapshot-builtins            write every snapshot
//!   cargo run -p xtask --bin snapshot-builtins -- --check fail if any differs (no write)

use specforge_component::ComponentRuntime;
use specforge_component::committed::{COMMITTED, snapshots, wire_answers};
use specforge_wasm::runtime::WasmRuntime;

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
    let out = snapshots();
    let runtime = ComponentRuntime::new();
    let mut failed = false;
    for blob in COMMITTED.iter().filter(|blob| blob.is_sdk()) {
        let dir = blob.snapshot_dir();
        if let Err(e) = runtime.load(blob.name, &blob.read()) {
            eprintln!("[fail]   {}: {e}", blob.name);
            failed = true;
            continue;
        }
        let dir_path = out.join(dir);
        let answers = match wire_answers(&runtime, blob) {
            Ok(answers) => answers,
            Err(e) => {
                eprintln!("[fail]   {dir}: {e}");
                failed = true;
                continue;
            }
        };
        let stale: Vec<&str> = answers
            .iter()
            .filter(|answer| {
                std::fs::read(dir_path.join(&answer.file)).ok().as_ref() != Some(&answer.bytes)
            })
            .map(|answer| answer.file.as_str())
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
        for answer in &answers {
            let path = dir_path.join(&answer.file);
            std::fs::write(&path, &answer.bytes)
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
            eprintln!("declaration snapshots are stale: cargo run -p xtask --bin revendor");
        }
        std::process::exit(1);
    }
}
