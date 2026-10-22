//! What a test that serves an extension in process writes so that its
//! project loads the extension through the production path: an installed
//! module and the lock entry that pins it.

use std::path::Path;

use specforge_package::PackageName;
use specforge_protocol_types::PeerDependency;
use specforge_registry::PlainDisk;
use specforge_wasm::WasmRuntime;
use specforge_wasm::testing::{InProcessRuntime, spelled_module};

use crate::layout::lock_path;
use crate::lock::{LockFile, LockFileEntry, LockState, write_lock_file};
use crate::module::hex_sha256;
use crate::{Installed, declaration_of};

/// The placeholder module installed for `name`: the bytes an in-process
/// runtime loads under that name ([`specforge_wasm::testing`] serves
/// whatever name it is given, whatever the bytes).
pub fn placeholder_module(name: &str) -> Vec<u8> {
    format!("\0asm in-process {name}").into_bytes()
}

/// The version and peers `runtime` declares for a module of `bytes` loaded as `name`. The in-process
/// runtime says so without a call; another runtime is asked, as an install asks it.
fn declared(
    runtime: &dyn WasmRuntime,
    name: &str,
    bytes: &[u8],
) -> Option<(String, Vec<PeerDependency>)> {
    match InProcessRuntime::of(runtime) {
        Some(runtime) => runtime.declared_by(name, bytes),
        None => declaration_of(bytes, runtime).ok().map(|loaded| {
            (
                loaded.declaration.version().to_string(),
                loaded.declaration.peers().to_vec(),
            )
        }),
    }
}

/// Install each of `names` at `root` as `runtime` serves it: a module that spells what the runtime
/// serves under that name and a lock entry pinning it (`source: registry`) and recording that
/// version and those peers, as an install records them, merged into any lock already there. A name
/// the runtime serves nothing under gets a placeholder and version `0.0.0-test`: it fails to load
/// before its entry is compared. A builtin needs no install: pass the other names.
///
/// # Panics
/// A name that is no package name, or a module or lock that cannot be
/// written: a test's fixture is wrong.
pub fn install(root: &Path, runtime: &dyn WasmRuntime, names: &[&str]) {
    for name in names {
        let placeholder = placeholder_module(name);
        let bytes = match declared(runtime, name, &placeholder) {
            Some((version, peers)) => spelled_module(name, &version, &peers),
            None => placeholder,
        };
        install_module(root, runtime, name, &bytes);
    }
}

/// Install `bytes` as the module of `name` at `root`, with the lock entry that pins it
/// (`source: registry`) and records the version and peers `runtime` serves for those bytes, merged
/// into any lock already there: an extension whose binary a test chose.
///
/// # Panics
/// As [`install`].
pub fn install_module(root: &Path, runtime: &dyn WasmRuntime, name: &str, bytes: &[u8]) {
    let installed = Installed::unread(root);
    let mut lock = match LockState::read(root, &PlainDisk) {
        LockState::Read(lock) => lock,
        LockState::Absent | LockState::Unreadable(_) => LockFile::default(),
    };
    let package = PackageName::parse(name)
        .unwrap_or_else(|why| panic!("'{name}' cannot be installed in a test: {why}"));
    let module = installed.module_path(&package);
    std::fs::create_dir_all(module.parent().expect("a module has a directory"))
        .expect("the extensions directory can be created");
    std::fs::write(&module, bytes).expect("the module can be written");
    let (version, peer_dependencies) =
        declared(runtime, name, bytes).unwrap_or_else(|| ("0.0.0-test".to_string(), Vec::new()));
    lock.entries.retain(|entry| entry.name != package);
    lock.entries.push(LockFileEntry {
        name: package,
        version,
        source: crate::lock::LockSource::Registry,
        wasm_hash: hex_sha256(bytes),
        key_id: None,
        peer_dependencies,
    });
    write_lock_file(&lock, &lock_path(root)).expect("the lock can be written");
}

/// Make each lock entry at `root` record the version and peers `runtime` serves for the module installed
/// under it, as an install records them: for a project whose runtime is only known once it is compiled.
/// An entry `runtime` serves nothing for, or without a module, is left as it is.
///
/// # Panics
/// A lock that cannot be written.
pub fn record_declarations(root: &Path, runtime: &dyn WasmRuntime) {
    let LockState::Read(mut lock) = LockState::read(root, &PlainDisk) else {
        return;
    };
    let installed = Installed::unread(root);
    let before = lock.clone();
    for entry in &mut lock.entries {
        let Ok(bytes) = std::fs::read(installed.module_path(&entry.name)) else {
            continue;
        };
        if let Some((version, peers)) = declared(runtime, entry.name.as_str(), &bytes) {
            entry.version = version;
            entry.peer_dependencies = peers;
        }
    }
    if lock != before {
        write_lock_file(&lock, &lock_path(root)).expect("the lock can be written");
    }
}

/// `<root>/specforge.lock` locking each `(name, version, source)`, written as
/// `specforge lock` writes it (the source as `LockSource::parse` reads it, the
/// hash a placeholder): what a project assembled in memory says it installed.
///
/// # Panics
/// A name that is no package name, or a lock that cannot be written.
pub fn write_lock(root: &Path, entries: &[(&str, &str, &str)]) -> LockFile {
    let lock = LockFile {
        lockfile_version: 1,
        entries: entries
            .iter()
            .map(|(name, version, source)| LockFileEntry {
                name: PackageName::parse(name)
                    .unwrap_or_else(|why| panic!("'{name}' cannot be locked in a test: {why}")),
                version: version.to_string(),
                source: crate::lock::LockSource::parse(source),
                wasm_hash: format!("hash-{name}"),
                key_id: None,
                peer_dependencies: Vec::new(),
            })
            .collect(),
    };
    write_lock_file(&lock, &lock_path(root)).expect("the lock can be written");
    lock
}

/// [`install`] (as `runtime` serves them) each named entry of `entries` (a project's `extensions`)
/// that is not a builtin and not a `.wasm` file: what loading the project
/// with an in-process runtime serving them requires.
pub fn install_enabled(
    root: &Path,
    entries: &[String],
    builtins: &crate::Builtins,
    runtime: &dyn WasmRuntime,
) {
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
    install(root, runtime, &names);
}

/// [`install_enabled`] the entries of the `specforge.json` at `root`, as
/// it is on disk now: call it after the config is written. A config the
/// compile could not use as written is a broken test: it panics, naming
/// the problems.
pub fn install_configured(root: &Path, builtins: &crate::Builtins, runtime: &dyn WasmRuntime) {
    let read = specforge_common::read_project_config(root);
    assert!(
        read.problems.is_empty(),
        "the test's specforge.json: {:?}",
        read.problems
    );
    install_enabled(root, &read.config.extensions, builtins, runtime);
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};

    #[test]
    fn installing_places_a_module_and_pins_it() {
        let dir = tempfile::tempdir().unwrap();

        let runtime = InProcessRuntime::new()
            .with(|| ContributionsBuilder::new(ExtensionMeta::new("@acme/a", "1.2.3")))
            .with(|| ContributionsBuilder::new(ExtensionMeta::new("@acme/b", "1.0.0")));
        install(dir.path(), &runtime, &["@acme/a", "@acme/b"]);
        install(dir.path(), &runtime, &["@acme/a"]);

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
        assert_eq!(bytes, spelled_module("@acme/a", "1.2.3", &[]));
        assert_eq!(
            installed.lock().entries()[1].version,
            "1.2.3",
            "the entry records the version the runtime serves"
        );
        assert!(
            installed.health().is_empty(),
            "the module is the one the lock pins"
        );
    }
}
