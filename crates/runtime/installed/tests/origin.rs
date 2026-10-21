// What the extension load records on each entry: where it takes its extension from and whether
// it loaded (ADR 0180).

use specforge_installed::testing::install;
use specforge_installed::{Builtins, Installed, LockSource, Origin};
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

use crate::load::{entries, extension, load_with};

const ENTRIES: [&str; 6] = [
    "@acme/a",
    "@acme/a@1.0.0",
    "@acme/none",
    "tool.wasm",
    "tool.wasm",
    "@acme/b=b.wasm",
];

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    install(dir.path(), &["@acme/a"]);
    std::fs::write(dir.path().join("tool.wasm"), b"\0asm tool").unwrap();
    dir
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "each entry records where the load takes its extension from and whether it loaded"
)]
fn the_load_records_each_entrys_origin_and_whether_it_loaded() {
    let dir = project();
    let runtime = InProcessRuntime::new()
        .with(extension("@acme/a"))
        .binary(b"\0asm tool", extension("@acme/tool"));

    let loaded = load_with(dir.path(), &entries(&ENTRIES), &runtime);

    let registry = Origin::Installed {
        source: LockSource::Registry,
    };
    let file = |path: &str| Origin::File { path: path.into() };
    let recorded: Vec<(Origin, bool, bool)> = loaded
        .enabled
        .iter()
        .map(|e| (e.origin.clone(), e.loaded, e.failure.is_some()))
        .collect();
    assert_eq!(
        recorded,
        [
            (registry.clone(), true, false),
            (registry, false, false),
            (Origin::Unknown, false, true),
            (file("tool.wasm"), true, false),
            (file("tool.wasm"), false, false),
            (file("b.wasm"), false, true),
        ]
    );
    for entry in loaded.enabled.iter().filter(|e| e.loaded) {
        let declared = loaded
            .declarations
            .iter()
            .filter(|d| d.name() == entry.name);
        assert_eq!(declared.count(), 1, "{}", entry.entry);
    }
}

#[test]
fn without_a_runtime_each_entry_records_its_origin() {
    let dir = project();
    let runtime = InProcessRuntime::new()
        .with(extension("@acme/a"))
        .binary(b"\0asm tool", extension("@acme/tool"));
    let entries = entries(&ENTRIES);

    let unloaded = Installed::at(dir.path()).unloaded(&entries, &Builtins::none());

    let loaded = load_with(dir.path(), &entries, &runtime);
    let origins = |l: &specforge_installed::Loaded| -> Vec<Origin> {
        l.enabled.iter().map(|e| e.origin.clone()).collect()
    };
    assert_eq!(origins(&unloaded), origins(&loaded));
    assert!(
        unloaded
            .enabled
            .iter()
            .all(|e| !e.loaded && e.failure.is_none())
    );
    assert!(unloaded.declarations.is_empty());
}
