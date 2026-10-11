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
    crate::publish::declare(&crate::testing::candidates(), wasm)
        .expect("a publishable binary")
        .0
}

pub(super) fn entry(
    name: &str,
    version: &str,
    source: &str,
    peers: &[(&str, &str)],
) -> LockFileEntry {
    LockFileEntry {
        name: PackageName::parse(name).unwrap(),
        version: version.to_string(),
        source: LockSource::parse(source),
        wasm_hash: hex_sha256(b"old"),
        key_id: None,
        peer_dependencies: peers
            .iter()
            .map(|(name, range)| PeerDependency {
                name: name.to_string(),
                version: range.to_string(),
                optional: false,
            })
            .collect(),
    }
}

/// A project locking `entries`, each installed with the bytes `old`.
pub(super) fn project(entries: Vec<LockFileEntry>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for e in &entries {
        let path = installed(dir.path(), e.name.as_str());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"old").unwrap();
    }
    let lock = LockFile {
        lockfile_version: 1,
        entries,
    };
    write_lock_file(&lock, &lock_path(dir.path())).unwrap();
    dir
}
