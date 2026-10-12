// The extension load over the runtime port, with the in-process adapter and a
// temp dir: every branch of the load policy, with no wasmtime.

use std::path::Path;
use std::sync::Mutex;

use specforge_common::Diagnostic;
use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_installed::testing::{install, placeholder_module};
use specforge_installed::{
    Builtins, Installed, LoadProblem, LoadSubject, LockState, hex_sha256, write_lock_file,
};
use specforge_package::PackageName;
use specforge_wasm::testing::InProcessRuntime;
use specforge_wasm::{WasmCallResult, WasmRuntime};
use tempfile::TempDir;

fn extension(name: &'static str) -> impl Fn() -> ContributionsBuilder + Send + Sync + 'static {
    move || ContributionsBuilder::new(ExtensionMeta::new(name, "1.0.0"))
}

/// A runtime serving `@acme/a` and `@acme/b` under their names.
fn runtime() -> InProcessRuntime {
    InProcessRuntime::new()
        .with(extension("@acme/a"))
        .with(extension("@acme/b"))
}

fn entries(entries: &[&str]) -> Vec<String> {
    entries.iter().map(|e| e.to_string()).collect()
}

fn load_with(
    root: &Path,
    entries: &[String],
    runtime: &dyn WasmRuntime,
) -> specforge_installed::Loaded {
    Installed::at(root).load(entries, &Builtins::none(), runtime)
}

fn package(name: &str) -> PackageName {
    PackageName::parse(name).unwrap()
}

/// What the load of `root` reported, in `load_diagnostics`' order.
fn reported(root: &Path, loaded: &specforge_installed::Loaded) -> Vec<Diagnostic> {
    Installed::at(root)
        .load_diagnostics(&loaded.enabled)
        .into_iter()
        .map(|(_, diagnostic)| diagnostic.clone())
        .collect()
}

fn codes(root: &Path, loaded: &specforge_installed::Loaded) -> Vec<String> {
    reported(root, loaded).into_iter().map(|d| d.code).collect()
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "an installed extension loads only when its binary is the one its specforge.lock entry pins"
)]
fn an_installed_extension_loads_from_its_pinned_module() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), &["@acme/a"]);

    let loaded = load_with(dir.path(), &entries(&["@acme/a"]), &runtime());

    assert_eq!(
        codes(dir.path(), &loaded),
        Vec::<String>::new(),
        "{:?}",
        reported(dir.path(), &loaded)
    );
    assert_eq!(loaded.declarations.len(), 1);
    assert_eq!(loaded.declarations[0].name(), "@acme/a");
    assert!(loaded.enabled[0].failure.is_none());
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "an installed extension loads only when its binary is the one its specforge.lock entry pins"
)]
fn a_changed_module_is_refused_and_not_loaded() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), &["@acme/a"]);
    let module = Installed::at(dir.path()).module_path(&package("@acme/a"));
    std::fs::write(&module, b"\0asm swapped after install").unwrap();
    let runtime = runtime();

    let loaded = load_with(dir.path(), &entries(&["@acme/a"]), &runtime);

    assert_eq!(
        codes(dir.path(), &loaded),
        ["E070"],
        "{:?}",
        reported(dir.path(), &loaded)
    );
    assert!(loaded.declarations.is_empty());
    let failure = loaded.enabled[0].failure.as_ref().unwrap();
    assert_eq!(
        failure.problem,
        LoadProblem::Changed {
            locked: hex_sha256(&placeholder_module("@acme/a")),
            actual: hex_sha256(b"\0asm swapped after install"),
        }
    );
    assert!(failure.problem.is_module_health());
    assert!(
        failure.diagnostic.message.contains("integrity mismatch"),
        "{:?}",
        failure.diagnostic
    );
    assert!(
        failure
            .diagnostic
            .suggestion
            .as_deref()
            .unwrap()
            .contains("specforge add @acme/a@0.0.0-test"),
        "the remedy is the command that reinstalls it as its lock entry records it"
    );
}

#[specforge_test_macros::test(
    behavior = "install_wasm_extension",
    verify = "an extension is installed under the extensions directory of its project, by its package name"
)]
fn an_entry_that_names_no_package_is_e072_and_not_loaded() {
    let dir = TempDir::new().unwrap();
    // A lock cannot name it; even an unreadable lock is not asked about it.
    std::fs::write(dir.path().join("specforge.lock"), "not a lock {{{").unwrap();
    let runtime = Recording {
        inner: runtime(),
        loaded: Mutex::new(Vec::new()),
    };

    let loaded = load_with(
        dir.path(),
        &entries(&["../../../outside1", "@acme/..", "Bad Name"]),
        &runtime,
    );

    assert_eq!(
        codes(dir.path(), &loaded),
        ["E072", "E072", "E072"],
        "{:?}",
        reported(dir.path(), &loaded)
    );
    for (diagnostic, entry) in
        reported(dir.path(), &loaded)
            .iter()
            .zip(["../../../outside1", "@acme/..", "Bad Name"])
    {
        assert!(diagnostic.message.contains(entry), "{diagnostic:?}");
    }
    assert!(loaded.declarations.is_empty());
    assert!(
        runtime.loaded.lock().unwrap().is_empty(),
        "nothing was loaded for a name that is no package"
    );
    assert!(matches!(
        loaded.enabled[0].failure.as_ref().unwrap().problem,
        LoadProblem::NotAPackageName { .. }
    ));
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "an enabled extension with no installed binary produces E028 naming the command that installs it"
)]
fn an_entry_without_a_lock_entry_is_e028_naming_the_command_that_installs_it() {
    let dir = TempDir::new().unwrap();

    let loaded = load_with(dir.path(), &entries(&["@acme/a"]), &runtime());

    assert_eq!(codes(dir.path(), &loaded), ["E028"]);
    let diagnostic = &reported(dir.path(), &loaded)[0];
    assert!(diagnostic.message.contains("no specforge.lock entry"));
    assert!(
        diagnostic
            .suggestion
            .as_deref()
            .unwrap()
            .contains("specforge add @acme/a")
    );
    assert_eq!(
        loaded.enabled[0].failure.as_ref().unwrap().problem,
        LoadProblem::NotInstalled
    );
}

#[test]
fn a_locked_extension_without_its_module_is_e028_naming_where_it_should_be() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), &["@acme/a"]);
    let module = Installed::at(dir.path()).module_path(&package("@acme/a"));
    std::fs::remove_file(&module).unwrap();

    let loaded = load_with(dir.path(), &entries(&["@acme/a"]), &runtime());

    assert_eq!(codes(dir.path(), &loaded), ["E028"]);
    assert_eq!(
        loaded.enabled[0].failure.as_ref().unwrap().problem,
        LoadProblem::ModuleMissing { path: module }
    );
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "an unreadable specforge.lock is reported once (E033) and each installed extension it leaves unloaded is E028 naming it"
)]
fn an_unreadable_lock_is_reported_once_before_the_first_entry_it_left_unloaded() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), &["@acme/a", "@acme/b"]);
    std::fs::write(dir.path().join("specforge.lock"), "not a lock {{{").unwrap();

    let loaded = load_with(dir.path(), &entries(&["@acme/a", "@acme/b"]), &runtime());

    let installed = Installed::at(dir.path());
    let reported = installed.load_diagnostics(&loaded.enabled);
    let parts: Vec<_> = reported
        .iter()
        .map(|(s, d)| (*s, d.code.as_str()))
        .collect();
    assert_eq!(
        parts,
        [
            (LoadSubject::Lock, "E033"),
            (LoadSubject::Entry(0), "E028"),
            (LoadSubject::Entry(1), "E028"),
        ]
    );
    assert!(reported[1].1.message.contains("@acme/a"));
    assert!(reported[2].1.message.contains("@acme/b"));
    assert!(
        reported[1]
            .1
            .message
            .contains("specforge.lock can't be read")
    );
    assert!(loaded.declarations.is_empty());
}

#[test]
fn a_project_that_enables_nothing_installed_does_not_report_its_lock() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("specforge.lock"), "not a lock {{{").unwrap();
    let builtins = Builtins(&[("@acme/a", &b"\0asm builtin a"[..])]);

    let loaded = Installed::at(dir.path()).load(&entries(&["@acme/a"]), &builtins, &runtime());

    assert_eq!(
        codes(dir.path(), &loaded),
        Vec::<String>::new(),
        "{:?}",
        reported(dir.path(), &loaded)
    );
    assert_eq!(loaded.declarations.len(), 1);
}

/// A runtime that records the bytes it is asked to load, over the in-process
/// adapter.
struct Recording {
    inner: InProcessRuntime,
    loaded: Mutex<Vec<(String, Vec<u8>)>>,
}

impl WasmRuntime for Recording {
    fn load(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.loaded
            .lock()
            .unwrap()
            .push((name.to_string(), bytes.to_vec()));
        self.inner.load(name, bytes)
    }
    fn rename(&self, from: &str, to: &str) -> bool {
        self.inner.rename(from, to)
    }
    fn unload(&self, name: &str) -> bool {
        self.inner.unload(name)
    }
    fn call_export(&self, extension_name: &str, export_name: &str, input: &[u8]) -> WasmCallResult {
        self.inner.call_export(extension_name, export_name, input)
    }
    fn apply_limits(&self, extension_name: &str, limits: specforge_wasm::Limits) {
        self.inner.apply_limits(extension_name, limits)
    }
}

#[specforge_test_macros::test(
    behavior = "load_wasm_module",
    verify = "the hash is checked over the bytes the runtime compiles"
)]
fn the_bytes_compiled_are_the_bytes_hashed() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), &["@acme/a"]);
    let runtime = Recording {
        inner: runtime(),
        loaded: Mutex::new(Vec::new()),
    };

    let loaded = load_with(dir.path(), &entries(&["@acme/a"]), &runtime);

    assert!(
        reported(dir.path(), &loaded).is_empty(),
        "{:?}",
        reported(dir.path(), &loaded)
    );
    let seen = runtime.loaded.lock().unwrap();
    assert_eq!(seen.len(), 1);
    let module = Installed::at(dir.path()).module_path(&package("@acme/a"));
    assert_eq!(seen[0].1, std::fs::read(module).unwrap());
    let LockState::Read(lock) = Installed::at(dir.path()).lock().clone() else {
        panic!("a lock was written");
    };
    assert_eq!(lock.entries[0].wasm_hash, hex_sha256(&seen[0].1));
}

/// `bytes` written as the `.wasm` file `name` under `dir`.
fn file(dir: &Path, name: &str, bytes: &[u8]) {
    std::fs::write(dir.join(name), bytes).unwrap();
}

#[test]
fn a_file_entry_loads_under_the_name_it_declares() {
    let dir = TempDir::new().unwrap();
    file(dir.path(), "tool.wasm", b"\0asm tool");
    let runtime = InProcessRuntime::new().binary(b"\0asm tool", extension("@acme/tool"));

    let loaded = load_with(dir.path(), &entries(&["tool.wasm"]), &runtime);

    assert!(
        reported(dir.path(), &loaded).is_empty(),
        "{:?}",
        reported(dir.path(), &loaded)
    );
    assert_eq!(loaded.enabled[0].name, "@acme/tool");
    assert_eq!(loaded.enabled[0].file.as_deref(), Some("tool.wasm"));
    assert_eq!(loaded.declarations[0].name(), "@acme/tool");
    // It answers under the name it declares, not under the entry.
    assert!(matches!(
        runtime.call_export("@acme/tool", "__handshake", b"{}"),
        WasmCallResult::Ok(_)
    ));
    assert!(matches!(
        runtime.call_export("tool.wasm", "__handshake", b"{}"),
        WasmCallResult::Trap(_)
    ));
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "a .wasm file entry that is missing, is not a component, names another extension or repeats a loaded one produces E028 naming the entry"
)]
fn a_file_entry_that_cannot_load_is_e028_naming_it() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), &["@acme/a"]);
    file(dir.path(), "plain.wasm", b"not a component");
    file(dir.path(), "tool.wasm", b"\0asm tool");
    file(dir.path(), "again.wasm", b"\0asm again");
    let runtime = runtime()
        .binary(b"\0asm tool", extension("@acme/tool"))
        .binary(b"\0asm again", extension("@acme/a"));
    let all = entries(&[
        "missing.wasm",
        "plain.wasm",
        "@acme/other=tool.wasm",
        "again.wasm",
        "@acme/a",
    ]);

    let loaded = load_with(dir.path(), &all, &runtime);

    assert_eq!(
        codes(dir.path(), &loaded),
        ["E028"; 4],
        "{:?}",
        reported(dir.path(), &loaded)
    );
    let problems: Vec<&LoadProblem> = loaded
        .enabled
        .iter()
        .filter_map(|e| e.failure.as_ref().map(|f| &f.problem))
        .collect();
    assert!(matches!(problems[0], LoadProblem::FileMissing { .. }));
    assert!(matches!(problems[1], LoadProblem::NotAComponent { .. }));
    assert!(matches!(
        problems[2],
        LoadProblem::NotTheNameWritten { declared, written }
            if declared == "@acme/tool" && written == "@acme/other"
    ));
    assert!(matches!(
        problems[3],
        LoadProblem::AlreadyLoaded { declared } if declared == "@acme/a"
    ));
    for (diagnostic, entry) in reported(dir.path(), &loaded).iter().zip([
        "missing.wasm",
        "plain.wasm",
        "@acme/other=tool.wasm",
        "again.wasm",
    ]) {
        assert!(
            diagnostic
                .message
                .contains(&format!("extension entry '{entry}'")),
            "{diagnostic:?}"
        );
    }
    // What was refused is not loaded, and the named entry is.
    assert_eq!(loaded.declarations.len(), 1);
    assert!(!runtime.unload("tool.wasm"));
    assert!(!runtime.unload("again.wasm"));
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "two extensions loaded and registries populated without collision"
)]
fn declarations_come_in_entry_order_once_per_extension() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), &["@acme/a", "@acme/b"]);
    file(dir.path(), "tool.wasm", b"\0asm tool");
    let runtime = runtime().binary(b"\0asm tool", extension("@acme/tool"));
    // `@acme/b` first, the file entry between, and `@acme/a` twice (the
    // second spelled as older `specforge add`s wrote it).
    let all = entries(&["@acme/b", "tool.wasm", "@acme/a", "@acme/a@1.0.0"]);

    let loaded = load_with(dir.path(), &all, &runtime);

    let names: Vec<&str> = loaded.declarations.iter().map(|d| d.name()).collect();
    assert_eq!(names, ["@acme/b", "@acme/tool", "@acme/a"]);
    assert!(
        reported(dir.path(), &loaded).is_empty(),
        "{:?}",
        reported(dir.path(), &loaded)
    );
    assert_eq!(loaded.enabled.len(), 4);
    assert!(loaded.enabled.iter().all(|e| e.failure.is_none()));
}

#[specforge_test_macros::test(
    behavior = "load_wasm_module",
    verify = "legacy lockfile entry without hash loads with W149"
)]
fn an_unpinned_entry_loads_with_w149() {
    let dir = TempDir::new().unwrap();
    install(dir.path(), &["@acme/a"]);
    let mut lock = match Installed::at(dir.path()).lock().clone() {
        LockState::Read(lock) => lock,
        other => panic!("{other:?}"),
    };
    lock.entries[0].wasm_hash = String::new();
    write_lock_file(&lock, &dir.path().join("specforge.lock")).unwrap();

    let loaded = load_with(dir.path(), &entries(&["@acme/a"]), &runtime());

    assert_eq!(
        loaded.declarations.len(),
        1,
        "{:?}",
        reported(dir.path(), &loaded)
    );
    assert_eq!(codes(dir.path(), &loaded), ["W149"]);
    assert!(reported(dir.path(), &loaded)[0].message.contains("@acme/a"));
    assert!(
        reported(dir.path(), &loaded)[0]
            .suggestion
            .as_deref()
            .unwrap()
            .contains("specforge add @acme/a@0.0.0-test")
    );
    assert!(loaded.enabled[0].failure.is_none(), "it loaded");
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "an installed extension loads only when its binary is the one its specforge.lock entry pins"
)]
fn a_module_declaring_another_extension_is_e070() {
    let dir = TempDir::new().unwrap();
    // The lock names @acme/a; the module installed there is @acme/b's.
    let b_binary = b"\0asm the binary of b";
    specforge_installed::testing::install_module(dir.path(), "@acme/a", b_binary);
    let runtime = InProcessRuntime::new().binary(b_binary, extension("@acme/b"));

    let loaded = load_with(dir.path(), &entries(&["@acme/a"]), &runtime);

    assert_eq!(
        codes(dir.path(), &loaded),
        ["E070"],
        "{:?}",
        reported(dir.path(), &loaded)
    );
    assert_eq!(
        loaded.enabled[0].failure.as_ref().unwrap().problem,
        LoadProblem::NotItsLockEntry {
            declared: "@acme/b".to_string()
        }
    );
    assert!(loaded.declarations.is_empty());
    assert!(
        reported(dir.path(), &loaded)[0]
            .message
            .contains("declares '@acme/b', not the extension its lock entry names"),
        "{:?}",
        reported(dir.path(), &loaded)[0]
    );
    // What was refused is not left loaded.
    assert!(!runtime.unload("@acme/a"));
}
