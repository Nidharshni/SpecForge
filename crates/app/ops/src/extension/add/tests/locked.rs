//! What specforge.lock pins for a registry install holds every fetch of it (ADR 0201): at the version
//! it pins, only its bytes install.

use super::*;
use crate::config::testing::files_under;
use specforge_installed::{hex_sha256, lock_path, read_lock_file};

/// A project locking `@sdk/greet` 0.1.0 as `sha256`, enabled, its module `old` on disk.
fn locked_greet(sha256: &str) -> tempfile::TempDir {
    add_project(vec![LockFileEntry {
        wasm_hash: sha256.to_string(),
        ..entry("@sdk/greet", "0.1.0", "registry", &[])
    }])
}

fn serving_greet() -> MemoryRegistry {
    MemoryRegistry::new().serving(Published::new(greet(), declaration_of(&greet())))
}

fn add_greet_at(
    dir: &Path,
    registry: &MemoryRegistry,
    dry_run: bool,
) -> Result<Added, Refusal<AddField>> {
    let root = specforge_common::ProjectRoot::of(dir).unwrap();
    add(
        &several(&root, vec!["@sdk/greet@0.1.0".into()], dry_run),
        registry,
        &runtime(),
    )
}

#[specforge_test(
    behavior = "check_registry_reply",
    verify = "add refuses the version specforge.lock pins served as other bytes, writing nothing, and its dry run says so"
)]
fn the_locked_version_served_as_other_bytes_is_refused() {
    // A clone has no module; a module changed on disk is not the pinned one either.
    for tampered in [None, Some(&b"tampered"[..])] {
        for dry_run in [true, false] {
            let dir = locked_greet(&hex_sha256(b"old"));
            let module = installed(dir.path(), "@sdk/greet");
            match tampered {
                None => std::fs::remove_file(&module).unwrap(),
                Some(bytes) => std::fs::write(&module, bytes).unwrap(),
            }
            let before = files_under(dir.path());
            let registry = serving_greet();

            let refusal = add_greet_at(dir.path(), &registry, dry_run).unwrap_err();

            assert_eq!(refusal.error.code, "R014", "{refusal:?}");
            assert_eq!(
                refusal.error.message,
                format!(
                    "registry package @sdk/greet@0.1.0 is not the binary specforge.lock pins: \
                     the lock records sha256 {}, the registry serves {}",
                    hex_sha256(b"old"),
                    hex_sha256(&greet())
                )
            );
            assert_eq!(files_under(dir.path()), before);
            assert!(registry.pins().is_empty());
        }
    }
}

#[specforge_test(
    behavior = "check_registry_reply",
    verify = "add of the version specforge.lock pins, served as its bytes, installs them and leaves the lock as it was"
)]
fn the_locked_version_served_as_its_bytes_installs_them_and_keeps_the_lock() {
    let dir = locked_greet(&hex_sha256(&greet()));
    let lock = std::fs::read(lock_path(dir.path())).unwrap();
    let module = installed(dir.path(), "@sdk/greet");
    for damage in [None, Some(&b"tampered"[..])] {
        match damage {
            None => std::fs::remove_file(&module).unwrap(),
            Some(bytes) => std::fs::write(&module, bytes).unwrap(),
        }

        add_greet_at(dir.path(), &serving_greet(), false).unwrap();

        assert_eq!(std::fs::read(&module).unwrap(), greet());
        assert_eq!(std::fs::read(lock_path(dir.path())).unwrap(), lock);
    }
}

#[specforge_test(
    behavior = "install_wasm_extension",
    verify = "an add of a registry install whose lock entry pins no hash fetches it and pins its bytes"
)]
#[ignore = "flipped by T4"]
fn an_entry_that_pins_no_hash_is_pinned_by_installing_it_again() {
    let dir = locked_greet("");
    let registry = serving_greet();

    add_greet_at(dir.path(), &registry, false).unwrap();

    assert!(!registry.asked().is_empty());
    let lock = read_lock_file(&lock_path(dir.path())).unwrap();
    assert_eq!(lock.entries[0].wasm_hash, hex_sha256(&greet()));
}
