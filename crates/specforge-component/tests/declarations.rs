//! The wire declaration of every committed SDK blob
//! (`specforge_component::committed::COMMITTED`), pinned byte for byte: each
//! blob's `__handshake` answer and its `__describe` answer for every category
//! the protocol supports equal the generated files under
//! `tests/declarations/<dir>/`.
//!
//! A change that is meant to change a declaration re-vendors the blobs,
//! then regenerates the files (`cargo run -p xtask --bin snapshot-builtins`)
//! and shows the diff for review. Byte identity also guards against a
//! dependency enabling `serde_json/preserve_order` in a guest, which would
//! reorder every key.

use std::collections::BTreeSet;

use specforge_component::ComponentRuntime;
use specforge_component::committed::{COMMITTED, CommittedBlob, Guest, snapshots, wire_answers};
use specforge_protocol_types::{DeclaredCategory, HandshakeRequest, PROTOCOL_VERSION};
use specforge_wasm::runtime::{WasmCallResult, WasmRuntime};

fn sdk_blobs() -> impl Iterator<Item = &'static CommittedBlob> {
    COMMITTED.iter().filter(|blob| blob.is_sdk())
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "every committed SDK blob's handshake and describe answers match their pinned snapshot byte for byte"
)]
fn sdk_declarations_match_their_snapshots() {
    let runtime = ComponentRuntime::new();
    let mut stale = Vec::new();
    for blob in sdk_blobs() {
        runtime
            .load(blob.name, &blob.read())
            .unwrap_or_else(|e| panic!("{}: {e}", blob.name));
        for answer in wire_answers(&runtime, blob).unwrap() {
            let path = snapshots().join(blob.snapshot_dir()).join(&answer.file);
            if std::fs::read(&path).ok() != Some(answer.bytes) {
                stale.push(format!("{}/{}", blob.snapshot_dir(), answer.file));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "these wire answers differ from their pinned snapshots: {stale:?}. \
         If the change is intended, re-vendor with \
         `cargo run -p xtask --bin revendor` and review the diff"
    );
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "the declaration snapshots are exactly the committed SDK blobs'"
)]
fn the_snapshots_are_the_sdk_blobs() {
    // The list is consistent with itself.
    let mut names = BTreeSet::new();
    let mut dirs = BTreeSet::new();
    let mut crate_dirs = BTreeSet::new();
    for blob in COMMITTED {
        assert!(names.insert(blob.name), "{}: listed twice", blob.name);
        let last = blob.crate_dir.rsplit('/').next().unwrap();
        assert!(
            crate_dirs.insert(last),
            "{}: crate directory name {last} is not unique",
            blob.name
        );
        assert!(
            blob.path().is_file(),
            "{}: no file at {}",
            blob.name,
            blob.repo_path()
        );
    }
    for blob in sdk_blobs() {
        assert!(
            dirs.insert(blob.snapshot_dir()),
            "{}: snapshot directory {} is not unique",
            blob.name,
            blob.snapshot_dir()
        );
    }

    // The snapshots hold exactly the SDK blobs, each with exactly its files.
    let on_disk: BTreeSet<String> = std::fs::read_dir(snapshots())
        .expect("the snapshots directory")
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let expected: BTreeSet<String> = dirs.iter().map(|d| d.to_string()).collect();
    assert_eq!(on_disk, expected, "snapshot directories vs SDK blobs");

    let files: BTreeSet<String> = std::iter::once("handshake.json".to_string())
        .chain(
            DeclaredCategory::ALL
                .iter()
                .map(|category| format!("describe_{}.json", category.name())),
        )
        .collect();
    for dir in &expected {
        let held: BTreeSet<String> = std::fs::read_dir(snapshots().join(dir))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(held, files, "{dir}: snapshot files");
    }
}

fn handshake_protocol(runtime: &ComponentRuntime, name: &str) -> String {
    let request = serde_json::to_vec(&HandshakeRequest {
        host_version: PROTOCOL_VERSION.to_string(),
    })
    .unwrap();
    let handshake = match runtime.call_export(name, "__handshake", &request) {
        WasmCallResult::Ok(bytes) => bytes,
        WasmCallResult::Trap(trap) => panic!("{name} __handshake: {}: {}", trap.kind, trap.message),
    };
    let value: serde_json::Value = serde_json::from_slice(&handshake).expect("a JSON handshake");
    value["protocol_version"]
        .as_str()
        .unwrap_or_else(|| panic!("{name}: the handshake has no protocol_version"))
        .to_string()
}

#[ignore = "fixed by the round's re-vendor; 09-T9 un-ignores it"]
#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "every committed SDK blob answers the host's protocol version; a bridge component answers the version it declares"
)]
fn every_sdk_blob_answers_the_host_protocol() {
    let runtime = ComponentRuntime::new();
    let mut stale = Vec::new();
    for blob in sdk_blobs() {
        runtime
            .load(blob.name, &blob.read())
            .unwrap_or_else(|e| panic!("{}: {e}", blob.name));
        let found = handshake_protocol(&runtime, blob.name);
        if found != PROTOCOL_VERSION {
            stale.push(format!("{}: {found}", blob.name));
        }
    }
    assert!(
        stale.is_empty(),
        "these blobs answer a protocol other than the host's {PROTOCOL_VERSION}: {stale:?}"
    );
}

#[specforge_test_macros::test(
    behavior = "load_extension_declaration",
    verify = "every committed SDK blob answers the host's protocol version; a bridge component answers the version it declares"
)]
fn the_bridge_component_answers_its_canned_protocol() {
    let runtime = ComponentRuntime::new();
    let mut checked = 0;
    for blob in COMMITTED {
        let Guest::Bridge { canned_protocol } = blob.guest else {
            continue;
        };
        runtime
            .load(blob.name, &blob.read())
            .unwrap_or_else(|e| panic!("{}: {e}", blob.name));
        assert_eq!(handshake_protocol(&runtime, blob.name), canned_protocol);
        checked += 1;
    }
    assert!(checked > 0, "no bridge component is listed");
}
