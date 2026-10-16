//! What the lock and the extension load read, through the disk they are handed
//! (ADR 0114): the lock once, then each
//! module in load order, and nothing for an entry that reads nothing.

use std::path::{Path, PathBuf};

use specforge_installed::{
    Builtins, Installed, LoadProblem, LockFile, LockFileEntry, LockSource, LockState, hex_sha256,
};
use specforge_package::PackageName;
use specforge_registry::{Asked, MemoryDisk};
use specforge_wasm::testing::InProcessRuntime;

use crate::load::entries;

const ROOT: &str = "/p";

fn lock_bytes() -> Vec<u8> {
    let lock = LockFile {
        lockfile_version: 1,
        entries: vec![LockFileEntry {
            name: PackageName::parse("@acme/a").unwrap(),
            version: "1.0.0".to_string(),
            source: LockSource::Registry,
            wasm_hash: hex_sha256(b"module"),
            key_id: None,
            peer_dependencies: Vec::new(),
        }],
    };
    serde_json::to_vec(&lock).unwrap()
}

fn lock_path() -> PathBuf {
    Path::new(ROOT).join("specforge.lock")
}

fn module_path() -> PathBuf {
    Path::new(ROOT).join(".specforge/extensions/@acme/a/extension.wasm")
}

#[test]
fn the_load_reads_the_lock_then_each_module_through_its_disk() {
    let disk = MemoryDisk::new()
        .with_file(lock_path(), &lock_bytes())
        .with_file(module_path(), b"module")
        .with_file("/p/tools/x.wasm", b"x");
    let builtins = Builtins(&[("@specforge/product", b"\0asm")]);
    let installed = Installed::read(Path::new(ROOT), &disk);
    installed.load(
        &entries(&["@specforge/product", "tools/x.wasm", "@acme/a", "@acme/a"]),
        &builtins,
        &InProcessRuntime::new(),
        &disk,
    );
    assert_eq!(
        disk.asked(),
        [
            Asked::Read(lock_path()),
            Asked::Read(module_path()),
            Asked::Read("/p/tools/x.wasm".into()),
        ],
        "the lock once, named before file, nothing for the builtin or the repeat"
    );
}

#[test]
fn a_missing_lock_and_a_missing_file_entry_are_one_read_each() {
    let disk = MemoryDisk::new();
    let installed = Installed::read(Path::new(ROOT), &disk);
    assert_eq!(installed.lock(), &LockState::Absent);
    let loaded = installed.load(
        &entries(&["tools/x.wasm"]),
        &Builtins::none(),
        &InProcessRuntime::new(),
        &disk,
    );
    let failure = loaded.enabled[0].failure.as_ref().unwrap();
    assert!(
        matches!(failure.problem, LoadProblem::FileMissing { .. }),
        "{failure:?}"
    );
    assert_eq!(
        disk.asked(),
        [
            Asked::Read(lock_path()),
            Asked::Read("/p/tools/x.wasm".into())
        ]
    );
}

#[test]
fn an_unreadable_lock_is_e033_and_reads_no_module() {
    let disk = MemoryDisk::new().with_file(lock_path(), b"\xff");
    let installed = Installed::read(Path::new(ROOT), &disk);
    let problem = installed.lock().problem().unwrap();
    assert_eq!(problem.code, "E033");
    assert!(
        problem
            .message
            .contains("stream did not contain valid UTF-8")
            && problem.message.contains("specforge.lock"),
        "{}",
        problem.message
    );
    installed.load(
        &entries(&["@acme/a"]),
        &Builtins::none(),
        &InProcessRuntime::new(),
        &disk,
    );
    assert_eq!(disk.asked(), [Asked::Read(lock_path())]);
}
