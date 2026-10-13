//! What a test that serves an extension in process writes so that its
//! project loads the extension through the production path: an installed
//! module and the lock entry that pins it.

use std::path::Path;

use specforge_package::PackageName;

use crate::Installed;
use crate::layout::lock_path;
use crate::lock::{LockFile, LockFileEntry, LockState, write_lock_file};
use crate::module::hex_sha256;

/// The placeholder module installed for `name`: the bytes an in-process
/// runtime loads under that name ([`specforge_wasm::testing`] serves
/// whatever name it is given, whatever the bytes).
pub fn placeholder_module(name: &str) -> Vec<u8> {
    format!("\0asm in-process {name}").into_bytes()
}

/// Install each of `names` at `root` as an in-process runtime serves it: a
/// placeholder module and a lock entry pinning it (`source: registry`,
/// version `0.0.0-test`), merged into any lock already there. A builtin
/// needs no install: pass the other names.
///
/// # Panics
/// A name that is no package name, or a module or lock that cannot be
/// written: a test's fixture is wrong.
pub fn install(root: &Path, names: &[&str]) {
    for name in names {
        install_module(root, name, &placeholder_module(name));
    }
}

/// Install `bytes` as the module of `name` at `root`, with the lock entry
/// that pins it (`source: registry`, version `0.0.0-test`), merged into any
/// lock already there: an extension whose binary a test chose.
///
/// # Panics
/// As [`install`].
pub fn install_module(root: &Path, name: &str, bytes: &[u8]) {
    let installed = Installed::unread(root);
    let mut lock = match LockState::at(root) {
        LockState::Read(lock) => lock,
        LockState::Absent | LockState::Unreadable(_) => LockFile::default(),
    };
    let package = PackageName::parse(name)
        .unwrap_or_else(|why| panic!("'{name}' cannot be installed in a test: {why}"));
    let module = installed.module_path(&package);
    std::fs::create_dir_all(module.parent().expect("a module has a directory"))
        .expect("the extensions directory can be created");
    std::fs::write(&module, bytes).expect("the module can be written");
    lock.entries.retain(|entry| entry.name != package);
    lock.entries.push(LockFileEntry {
        name: package,
        version: "0.0.0-test".to_string(),
        source: crate::lock::LockSource::Registry,
        wasm_hash: hex_sha256(bytes),
        key_id: None,
        peer_dependencies: Vec::new(),
    });
    write_lock_file(&lock, &lock_path(root)).expect("the lock can be written");
}

/// [`install`] each named entry of `entries` (a project's `extensions`)
/// that is not a builtin and not a `.wasm` file: what loading the project
/// with an in-process runtime serving them requires.
pub fn install_enabled(root: &Path, entries: &[String], builtins: &crate::Builtins) {
    let names: Vec<&str> = entries
        .iter()
        .filter_map(
            |entry| match specforge_common::ExtensionEntry::parse(entry) {
                specforge_common::ExtensionEntry::Named(name) if !builtins.contains(name) => {
                    Some(name)
                }
                _ => None,
            },
        )
        .collect();
    install(root, &names);
}

/// [`install_enabled`] the entries of the `specforge.json` at `root`, as
/// it is on disk now: call it after the config is written. A config the
/// compile could not use as written is a broken test: it panics, naming
/// the problems.
pub fn install_configured(root: &Path, builtins: &crate::Builtins) {
    let read = specforge_common::read_project_config(root);
    assert!(
        read.problems.is_empty(),
        "the test's specforge.json: {:?}",
        read.problems
    );
    install_enabled(root, &read.config.extensions, builtins);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installing_places_a_module_and_pins_it() {
        let dir = tempfile::tempdir().unwrap();

        install(dir.path(), &["@acme/a", "@acme/b"]);
        install(dir.path(), &["@acme/a"]);

        let installed = Installed::at(dir.path());
        let names: Vec<&str> = installed
            .lock()
            .entries()
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["@acme/b", "@acme/a"], "merged, not replaced");
        let package = PackageName::parse("@acme/a").unwrap();
        let bytes = std::fs::read(installed.module_path(&package)).unwrap();
        assert_eq!(bytes, placeholder_module("@acme/a"));
        assert!(
            installed.health().is_empty(),
            "the module is the one the lock pins"
        );
    }
}
