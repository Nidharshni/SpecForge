// A .wasm file entry's declared name is a package name, and its component never loads over
// another entry's (ADR 0223).

use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_installed::LoadProblem;
use specforge_wasm::testing::InProcessRuntime;
use specforge_wasm::{WasmCallResult, WasmRuntime};
use tempfile::TempDir;

use crate::load::{entries, extension, file, load_with, reported};

/// What `name` answers to its handshake: the JSON, or None when nothing is loaded under it.
fn handshake(runtime: &InProcessRuntime, name: &str) -> Option<serde_json::Value> {
    match runtime.call_export(name, "__handshake", b"{}") {
        WasmCallResult::Ok(answer) => Some(serde_json::from_slice(&answer).unwrap()),
        _ => None,
    }
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "a .wasm file entry whose component declares no package name is E072 naming the entry, and nothing loads"
)]
#[ignore = "flipped by T4"]
fn a_file_entry_declaring_no_package_name_is_e072_naming_the_entry() {
    for (entry, path, declared) in [
        ("x.wasm", "x.wasm", "@acme/Tool"),
        ("c.wasm", "c.wasm", "__candidate"),
        ("Bad=b.wasm", "b.wasm", "Bad"),
        ("g=y.wasm", "y.wasm", "@acme/Tool"),
    ] {
        let dir = TempDir::new().unwrap();
        file(dir.path(), path, b"\0asm entry");
        let runtime = InProcessRuntime::new().binary(b"\0asm entry", extension(declared));

        let loaded = load_with(dir.path(), &entries(&[entry]), &runtime);

        let refused = &loaded.enabled[0];
        assert!(!refused.loaded, "{entry}: {refused:?}");
        assert!(
            matches!(
                refused.failure.as_ref().map(|f| &f.problem),
                Some(LoadProblem::NotAPackageName { .. })
            ),
            "{entry}: {refused:?}"
        );
        let reports = reported(dir.path(), &loaded);
        assert_eq!(reports.len(), 1, "{entry}: {reports:?}");
        assert_eq!(reports[0].code, "E072");
        assert!(
            reports[0].message.starts_with(&format!(
                "extension entry '{entry}' in specforge.json: '{declared}' is not a package name"
            )),
            "{:?}",
            reports[0]
        );
        assert!(loaded.declarations.is_empty());
        // Nothing stays loaded under any key the load may have used.
        assert!(!runtime.unload(declared));
        assert!(!runtime.unload(&format!("file:{entry}")));
        assert!(!runtime.unload(entry));
    }
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "a .wasm file entry loads under a key no extension is admitted under, so it never replaces what another entry loaded"
)]
#[ignore = "flipped by T2"]
fn a_file_entrys_load_never_replaces_another_entrys_extension() {
    for order in [["first.wasm", "second.wasm"], ["second.wasm", "first.wasm"]] {
        let dir = TempDir::new().unwrap();
        file(dir.path(), "second.wasm", b"\0asm second");
        file(dir.path(), "first.wasm", b"\0asm first");
        let runtime = InProcessRuntime::new()
            .binary(b"\0asm second", extension("first.wasm"))
            .binary(b"\0asm first", extension("@acme/first"));

        let loaded = load_with(dir.path(), &entries(&order), &runtime);

        assert!(loaded.enabled.iter().all(|e| e.loaded), "{order:?}");
        assert_eq!(loaded.declarations.len(), 2, "{order:?}");
        for name in ["first.wasm", "@acme/first"] {
            let answer = handshake(&runtime, name)
                .unwrap_or_else(|| panic!("{order:?}: nothing is loaded as {name}"));
            assert_eq!(answer["name"], name, "{order:?}");
        }
    }
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "a .wasm file entry declaring a name another entry already loads is refused, whatever its own text"
)]
#[ignore = "flipped by T3"]
fn a_file_entry_declaring_its_own_text_is_refused_when_another_entry_loads_that_name() {
    let dir = TempDir::new().unwrap();
    file(dir.path(), "x.wasm", b"\0asm x");
    file(dir.path(), "first.wasm", b"\0asm f");
    let runtime = InProcessRuntime::new()
        .binary(b"\0asm x", extension("first.wasm"))
        .binary(b"\0asm f", || {
            ContributionsBuilder::new(ExtensionMeta::new("first.wasm", "2.0.0"))
        });

    let loaded = load_with(dir.path(), &entries(&["x.wasm", "first.wasm"]), &runtime);

    assert!(loaded.enabled[0].loaded);
    assert!(matches!(
        loaded.enabled[1].failure.as_ref().map(|f| &f.problem),
        Some(LoadProblem::AlreadyLoaded { declared }) if declared == "first.wasm"
    ));
    let reports = reported(dir.path(), &loaded);
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0].code, "E028");
    assert!(reports[0].message.contains("extension entry 'first.wasm'"));
    assert!(
        reports[0]
            .message
            .contains("already loaded by another entry")
    );
    assert_eq!(loaded.declarations.len(), 1);
    assert_eq!(
        handshake(&runtime, "first.wasm").unwrap()["version"],
        "1.0.0"
    );
}
