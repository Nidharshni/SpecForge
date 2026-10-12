// A change to what is installed is all or nothing: temp dirs, no runtime.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use specforge_common::{Diagnostic, codes};
use specforge_installed::{Change, Installed, Module, Pin};
use specforge_package::PackageName;
use tempfile::TempDir;

type Snapshot = BTreeMap<PathBuf, Vec<u8>>;

/// Every file under `root` with its bytes.
fn snapshot(root: &Path) -> Snapshot {
    let mut files = Snapshot::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.map(|entry| entry.unwrap().path()) {
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    files
}

fn name(text: &str) -> PackageName {
    PackageName::parse(text).unwrap()
}

fn pin(text: &str) -> Pin {
    Pin {
        name: name(text),
        version: "1.0.0".to_string(),
        source: specforge_installed::LockSource::parse("registry"),
        key_id: None,
        peers: Vec::new(),
    }
}

/// A project with `specforge.json` and `@acme/a` installed holding `bytes`.
fn project_with_a(bytes: &[u8]) -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("specforge.json"), "{\"extensions\":[]}").unwrap();
    let installed = Installed::at(dir.path());
    let mut change = installed.change().unwrap();
    change.install(Module::new(bytes.to_vec()), pin("@acme/a"));
    change.commit().unwrap();
    dir
}

/// Make the lock write fail: it goes through a sibling file, and a
/// directory in its place cannot be written.
fn unwritable_lock(root: &Path) {
    std::fs::create_dir_all(root.join("specforge.lock.tmp")).unwrap();
}

/// Where `text` is installed under `root`.
fn module_of(root: &Path, text: &str) -> PathBuf {
    Installed::at(root).module_path(&name(text))
}

#[specforge_test_macros::test(
    behavior = "install_wasm_extension",
    verify = "places binary atomically via temp dir"
)]
fn a_committed_install_places_the_module_and_locks_it() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let installed = Installed::at(root);
    let mut change = installed.change().unwrap();
    change.install(Module::new(b"one".to_vec()), pin("@acme/a"));

    let committed = change.commit().unwrap();

    let module = module_of(root, "@acme/a");
    assert_eq!(std::fs::read(&module).unwrap(), b"one");
    assert_eq!(committed.changed, [module, root.join("specforge.lock")]);
    let installed = Installed::at(root);
    let entry = &installed.lock().entries()[0];
    assert_eq!(entry.name, name("@acme/a"));
    assert_eq!(entry.wasm_hash, specforge_installed::hex_sha256(b"one"));
    assert!(installed.health().is_empty());
    assert!(
        !root.join(".specforge/extensions/.staging").exists(),
        "nothing is left in the staging directory"
    );
}

#[specforge_test_macros::test(
    behavior = "extension_operation_atomicity",
    verify = "failed install rolls back to previous state"
)]
fn a_failed_install_puts_back_the_binary_the_lock_and_the_config() {
    let config = "{\"extensions\":[]}";
    let fresh = || {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("specforge.json"), config).unwrap();
        dir
    };
    fn attempt(installed: &Installed) -> Change<'_> {
        let mut change = installed.change().unwrap();
        change.install(Module::new(b"new".to_vec()), pin("@acme/a"));
        change
    }

    // A placement that fails: `.specforge` is a file, not a directory.
    let dir = fresh();
    std::fs::write(dir.path().join(".specforge"), "in the way").unwrap();
    let before = snapshot(dir.path());
    let failed = attempt(&Installed::at(dir.path())).commit().unwrap_err();
    assert_eq!(failed.error.code, "E032", "{:?}", failed.error);
    assert!(failed.left.is_empty());
    assert_eq!(snapshot(dir.path()), before);

    // A lock that cannot be written, after the module was placed.
    let dir = fresh();
    unwritable_lock(dir.path());
    let before = snapshot(dir.path());
    let failed = attempt(&Installed::at(dir.path())).commit().unwrap_err();
    assert_eq!(failed.error.code, "E033", "{:?}", failed.error);
    assert!(
        failed.error.message.contains("specforge.lock.tmp"),
        "the message names the file it could not write: {:?}",
        failed.error
    );
    assert!(failed.left.is_empty());
    assert_eq!(snapshot(dir.path()), before);

    // A config edit that fails after it wrote.
    let dir = fresh();
    let config_file = dir.path().join("specforge.json");
    let before = snapshot(dir.path());
    let failed = attempt(&Installed::at(dir.path()))
        .commit_with(Some(&config_file), || -> Result<(), Diagnostic> {
            std::fs::write(&config_file, "half written").unwrap();
            Err(Diagnostic::new(codes::E069, "the config cannot be edited"))
        })
        .unwrap_err();
    assert_eq!(failed.error.code, "E069");
    assert!(failed.left.is_empty());
    assert_eq!(snapshot(dir.path()), before);
}

#[specforge_test_macros::test(
    behavior = "extension_operation_atomicity",
    verify = "interrupted upgrade preserves original extension"
)]
fn a_failed_upgrade_keeps_the_original_module() {
    let dir = project_with_a(b"original");
    let root = dir.path();
    unwritable_lock(root);
    let before = snapshot(root);
    let installed = Installed::at(root);
    let mut change = installed.change().unwrap();
    change.install(Module::new(b"newer".to_vec()), pin("@acme/a"));

    let failed = change.commit().unwrap_err();

    assert_eq!(failed.error.code, "E033");
    assert_eq!(
        std::fs::read(module_of(root, "@acme/a")).unwrap(),
        b"original"
    );
    assert_eq!(snapshot(root), before);
}

#[specforge_test_macros::test(
    behavior = "uninstall_wasm_extension",
    verify = "rolls back on failure"
)]
fn a_failed_uninstall_puts_back_the_directory_the_lock_and_the_config() {
    let dir = project_with_a(b"original");
    let root = dir.path();
    let config_file = root.join("specforge.json");
    let install_config = "{\"extensions\":[\"@acme/a\"]}";
    std::fs::write(&config_file, install_config).unwrap();
    let before = snapshot(root);

    // The lock cannot be written: the directory comes back.
    unwritable_lock(root);
    let installed = Installed::at(root);
    let mut change = installed.change().unwrap();
    change.uninstall(&name("@acme/a"));
    let failed = change.commit().unwrap_err();
    assert_eq!(failed.error.code, "E033");
    std::fs::remove_dir(root.join("specforge.lock.tmp")).unwrap();
    assert_eq!(snapshot(root), before);

    // The config edit fails after it wrote: the lock and the directory
    // come back too.
    let installed = Installed::at(root);
    let mut change = installed.change().unwrap();
    change.uninstall(&name("@acme/a"));
    let failed = change
        .commit_with(Some(&config_file), || -> Result<(), Diagnostic> {
            std::fs::write(&config_file, "{\"extensions\":[]}").unwrap();
            Err(Diagnostic::new(codes::E069, "the config cannot be edited"))
        })
        .unwrap_err();
    assert_eq!(failed.error.code, "E069");
    assert!(failed.left.is_empty());
    assert_eq!(snapshot(root), before);
}

#[specforge_test_macros::test(
    behavior = "install_wasm_extension",
    verify = "an install over an unreadable specforge.lock is refused before anything is written"
)]
fn a_change_over_an_unreadable_lock_is_refused() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    std::fs::write(root.join("specforge.lock"), "not a lock {{{").unwrap();
    let before = snapshot(root);

    let refused = Installed::at(root).change().err().expect("a refusal");

    assert_eq!(refused.code, "E033");
    assert!(refused.message.contains("corrupt lock file"), "{refused:?}");
    assert!(refused.suggestion.is_some());
    assert_eq!(snapshot(root), before, "nothing was written");
}

#[cfg(unix)]
#[test]
fn a_rollback_that_cannot_restore_a_file_names_it() {
    use std::os::unix::fs::PermissionsExt;

    let dir = project_with_a(b"original");
    let root = dir.path();
    let config_file = root.join("specforge.json");
    let module = module_of(root, "@acme/a");
    let scope = module.parent().unwrap().parent().unwrap().to_path_buf();
    let mode = |m| std::fs::Permissions::from_mode(m);
    // Running as a user permissions don't bind (root): nothing to test.
    std::fs::set_permissions(&scope, mode(0o555)).unwrap();
    let binds = std::fs::write(scope.join("probe"), b"").is_err();
    std::fs::set_permissions(&scope, mode(0o755)).unwrap();
    if !binds {
        return;
    }
    let installed = Installed::at(root);
    let mut change = installed.change().unwrap();
    change.install(Module::new(b"newer".to_vec()), pin("@acme/a"));

    let failed = change
        .commit_with(Some(&config_file), || -> Result<(), Diagnostic> {
            // The scope directory can no longer be changed: the rollback
            // cannot take the new module away nor bring the old one back.
            std::fs::set_permissions(&scope, mode(0o555)).unwrap();
            Err(Diagnostic::new(codes::E069, "the config cannot be edited"))
        })
        .unwrap_err();
    std::fs::set_permissions(&scope, mode(0o755)).unwrap();

    assert_eq!(failed.left, [module.parent().unwrap().to_path_buf()]);
    assert_eq!(failed.error.code, "E069");
}

#[test]
fn a_failing_last_step_puts_back_the_binaries_and_the_lock() {
    let dir = project_with_a(b"original");
    let root = dir.path();
    let before = snapshot(root);
    let installed = Installed::at(root);
    let mut change = installed.change().unwrap();
    change.install(Module::new(b"newer".to_vec()), pin("@acme/a"));
    change.install(Module::new(b"other".to_vec()), pin("@acme/b"));

    let failed = change
        .commit_with(None, || -> Result<(), Diagnostic> {
            Err(Diagnostic::new(codes::R_TRUST_006, "x"))
        })
        .unwrap_err();

    assert_eq!(failed.error.code, "R-TRUST-006");
    assert!(failed.left.is_empty());
    assert_eq!(snapshot(root), before);
}

#[test]
fn a_config_the_last_step_leaves_as_it_was_is_not_changed() {
    let dir = project_with_a(b"original");
    let root = dir.path();
    let config_file = root.join("specforge.json");
    let module = module_of(root, "@acme/a");
    let installed = Installed::at(root);
    let mut change = installed.change().unwrap();
    change.install(Module::new(b"newer".to_vec()), pin("@acme/a"));

    let committed = change
        .commit_with(Some(&config_file), || -> Result<(), Diagnostic> {
            let same = std::fs::read(&config_file).unwrap();
            std::fs::write(&config_file, same).unwrap();
            Ok(())
        })
        .unwrap();

    assert_eq!(committed.changed, [module, root.join("specforge.lock")]);

    let installed = Installed::at(root);
    let committed = installed
        .change()
        .unwrap()
        .commit_with(Some(&config_file), || -> Result<(), Diagnostic> {
            std::fs::write(&config_file, "{\"extensions\":[\"@acme/a\"]}").unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(committed.changed, [config_file]);
}

#[test]
fn committed_lists_the_files_it_changed() {
    let dir = project_with_a(b"original");
    let root = dir.path();
    let module = module_of(root, "@acme/a");
    let lock = root.join("specforge.lock");

    // The same module again: nothing changed.
    let installed = Installed::at(root);
    let mut change = installed.change().unwrap();
    change.install(Module::new(b"original".to_vec()), pin("@acme/a"));
    assert_eq!(change.commit().unwrap().changed, Vec::<PathBuf>::new());

    // A changed module changes the module and the lock.
    let installed = Installed::at(root);
    let mut change = installed.change().unwrap();
    change.install(Module::new(b"newer".to_vec()), pin("@acme/a"));
    assert_eq!(
        change.commit().unwrap().changed,
        [module.clone(), lock.clone()]
    );

    // An uninstall lists each file the directory held.
    let other = module.parent().unwrap().join("notes.txt");
    std::fs::write(&other, "kept by the user").unwrap();
    let installed = Installed::at(root);
    let mut change = installed.change().unwrap();
    change.uninstall(&name("@acme/a"));
    let committed = change.commit().unwrap();
    let mut expected = vec![module.clone(), other, lock];
    expected.sort();
    assert_eq!(committed.changed, expected);
    assert!(!module.parent().unwrap().exists());
    assert!(
        !module.parent().unwrap().parent().unwrap().exists(),
        "the scope directory it emptied goes too"
    );

    // A name the lock does not hold stages nothing.
    let installed = Installed::at(root);
    let mut change = installed.change().unwrap();
    change.uninstall(&name("@acme/a"));
    assert_eq!(change.commit().unwrap().changed, Vec::<PathBuf>::new());
}
