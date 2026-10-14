// The lock file and doctor checks.

use specforge_installed::{
    Health, Installed, LockFile, LockFileEntry, LockState, read_lock_file, write_lock_file,
};
use specforge_package::PackageName;
use std::path::Path;
use tempfile::TempDir;

// ============================================================
// B:write_lock_file + B:read_lock_file
// ============================================================

// B:write_lock_file, B:read_lock_file — verify integration "roundtrip write+read produces identical LockFile"
#[test]
fn lock_file_roundtrip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("specforge.lock");

    let lock = LockFile {
        lockfile_version: 1,
        entries: vec![
            LockFileEntry {
                name: specforge_package::PackageName::parse("@specforge/software").unwrap(),
                version: "1.0.0".to_string(),
                source: specforge_installed::LockSource::parse("registry"),
                wasm_hash: "abc123".to_string(),
                key_id: None,
                peer_dependencies: Vec::new(),
            },
            LockFileEntry {
                name: specforge_package::PackageName::parse("@specforge/governance").unwrap(),
                version: "2.0.0".to_string(),
                source: specforge_installed::LockSource::parse("local:./ext"),
                wasm_hash: "def456".to_string(),
                key_id: None,
                peer_dependencies: Vec::new(),
            },
        ],
    };

    write_lock_file(&lock, &path).unwrap();
    let read_back = read_lock_file(&path).unwrap();
    assert_eq!(lock, read_back);
}

// B:read_lock_file — verify integration "corrupt file produces E033"
#[test]
fn lock_file_corrupt_e033() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("specforge.lock");
    std::fs::write(&path, "not json {{{").unwrap();

    let err = read_lock_file(&path).unwrap_err();
    assert_eq!(err.code, "E033");
    assert!(err.message.contains("corrupt"));
}

// B:read_lock_file — verify integration "missing file produces E033"
#[test]
fn lock_file_missing_e033() {
    let err = read_lock_file(Path::new("/nonexistent/specforge.lock")).unwrap_err();
    assert_eq!(err.code, "E033");
}

// ============================================================
// B:run_doctor_check
// ============================================================

fn entry(name: &str, hash: &str) -> LockFileEntry {
    LockFileEntry {
        name: specforge_package::PackageName::parse(name).unwrap(),
        version: "1.0.0".to_string(),
        source: specforge_installed::LockSource::parse("registry"),
        wasm_hash: hash.to_string(),
        key_id: None,
        peer_dependencies: Vec::new(),
    }
}

fn installed(dir: &TempDir, entry: LockFileEntry) -> Installed {
    Installed::with_lock(
        dir.path(),
        LockState::Read(LockFile {
            lockfile_version: 1,
            entries: vec![entry],
        }),
    )
}

fn put_module(installed: &Installed, name: &str, bytes: &[u8]) {
    let path = installed.module_path(&PackageName::parse(name).unwrap());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

// B:run_doctor_check — verify integration "missing binary detected"
#[test]
fn doctor_missing_binary() {
    let dir = TempDir::new().unwrap();
    let installed = installed(&dir, entry("missing-ext", "abc"));

    assert!(
        installed
            .health()
            .iter()
            .any(|r| matches!(r, Health::MissingModule { name } if name == "missing-ext"))
    );
}

// B:run_doctor_check — verify integration "stale hash detected"
#[test]
fn doctor_stale_hash() {
    let dir = TempDir::new().unwrap();
    let installed = installed(&dir, entry("my-ext", "expected_hash"));
    put_module(&installed, "my-ext", b"content");

    assert!(
        installed
            .health()
            .iter()
            .any(|r| matches!(r, Health::Changed { .. }))
    );
}

// B:run_doctor_check — verify integration "all healthy returns empty"
#[test]
fn doctor_all_healthy() {
    let dir = TempDir::new().unwrap();
    let installed = installed(
        &dir,
        entry("good-ext", &specforge_installed::hex_sha256(b"wasm")),
    );
    put_module(&installed, "good-ext", b"wasm");

    let results = installed.health();
    assert!(results.is_empty(), "expected healthy, got: {:?}", results);
}
