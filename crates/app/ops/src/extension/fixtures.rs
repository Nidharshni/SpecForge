//! What the extension operations' tests share: the greet extension, the lock entries and projects they
//! work on.

use specforge_installed::{
    Installed, LockFile, LockFileEntry, LockSource, hex_sha256, lock_path, write_lock_file,
};
use specforge_package::PackageName;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::PeerDependency;
use std::path::{Path, PathBuf};

/// Where the module of extension `name` is installed under `root`.
pub(super) fn installed(root: &Path, name: &str) -> PathBuf {
    Installed::unread(root).module_path(&PackageName::parse(name).unwrap())
}

/// The bytes the in-process runtime serves as `@sdk/greet` 0.1.0.
pub(super) fn greet() -> Vec<u8> {
    crate::testing::GREET.to_vec()
}

/// What `wasm` declares, as `publish` would upload it.
pub(super) fn declaration_of(wasm: &[u8]) -> ExtensionDeclaration {
    crate::extension::declare(&crate::testing::candidates(), wasm)
        .expect("a publishable binary")
        .0
}

pub(super) fn entry(
    name: &str,
    version: &str,
    source: &str,
    peers: &[(&str, &str)],
) -> LockFileEntry {
    let peers: Vec<PeerDependency> = peers
        .iter()
        .map(|(name, range)| PeerDependency {
            name: name.to_string(),
            version: range.to_string(),
            optional: false,
        })
        .collect();
    LockFileEntry {
        name: PackageName::parse(name).unwrap(),
        version: version.to_string(),
        source: LockSource::parse(source),
        wasm_hash: hex_sha256(&module_of(name, version, &peers)),
        key_id: None,
        peer_dependencies: peers,
    }
}

/// The module a lock entry for `name` pins in these tests: bytes that spell the declaration the entry
/// records, which the in-process runtime serves under any name.
pub(super) fn module_of(name: &str, version: &str, peers: &[PeerDependency]) -> Vec<u8> {
    specforge_wasm::testing::spelled_module(name, version, peers)
}

/// [`project`], with a `specforge.json` enabling every locked name: the extensions the project
/// loads (ADR 0041: the peer set is what the entries load).
pub(super) fn enabled_project(entries: Vec<LockFileEntry>) -> tempfile::TempDir {
    let names: Vec<String> = entries.iter().map(|e| e.name.to_string()).collect();
    let dir = project(entries);
    let config = serde_json::json!({"name": "p", "version": "0.1.0", "extensions": names});
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    dir
}

/// A project locking `entries`, each installed with the module its entry pins ([`module_of`]).
pub(super) fn project(entries: Vec<LockFileEntry>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for e in &entries {
        let path = installed(dir.path(), e.name.as_str());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let module = module_of(e.name.as_str(), &e.version, &e.peer_dependencies);
        std::fs::write(path, module).unwrap();
    }
    let lock = LockFile {
        lockfile_version: 1,
        entries,
    };
    write_lock_file(&lock, &lock_path(dir.path())).unwrap();
    dir
}
