// The per-entry extension load (plan r12-p08): one step for every kind of entry,
// so a problem it finds has one message, a refused entry leaves nothing loaded,
// and a declaration is read once.

use serde_json::json;
use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_installed::testing::install_module;
use specforge_installed::{Builtins, Installed, LoadProblem};
use specforge_protocol_types::DeclaredCategory;
use specforge_wasm::testing::InProcessRuntime;
use specforge_wasm::{WasmCallResult, WasmRuntime, WasmTrapInfo};
use tempfile::TempDir;

use crate::load::{entries, extension, file, load_with, reported};

/// `runtime` where each of `names` traps when asked to describe its entities:
/// the handshake is answered, the declaration can't be read. A file entry
/// passes both its key and the name it declares, so the pin holds whichever
/// name the describe is asked under.
fn describe_fails(mut runtime: InProcessRuntime, names: &[&str]) -> InProcessRuntime {
    for name in names {
        runtime = runtime.answer_raw_to(
            name,
            "__describe",
            json!({"category": "entities"}),
            WasmCallResult::Trap(WasmTrapInfo {
                kind: "guest_error".into(),
                message: "no describe".into(),
                export_name: "__describe".into(),
            }),
        );
    }
    runtime
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "an entry whose declaration can't be read is E028 saying so and naming what failed"
)]
fn a_file_entry_whose_describe_fails_says_its_declaration_cannot_be_read() {
    let dir = TempDir::new().unwrap();
    file(dir.path(), "m.wasm", b"\0asm m");
    let runtime = describe_fails(
        InProcessRuntime::new().binary(b"\0asm m", extension("@acme/m")),
        &["m.wasm", "@acme/m"],
    );

    let loaded = load_with(dir.path(), &entries(&["m.wasm"]), &runtime);

    let reports = reported(dir.path(), &loaded);
    assert_eq!(reports.len(), 1, "{reports:?}");
    let failure = loaded.enabled[0].failure.as_ref().unwrap();
    assert!(
        matches!(&failure.problem, LoadProblem::NoDeclaration { reason }
            if reason.starts_with("describe 'entities' failed")),
        "{:?}",
        failure.problem
    );
    let message = &reports[0].message;
    assert!(message.contains("extension entry 'm.wasm'"), "{message}");
    assert!(
        message.contains("has no declaration the host can read: describe 'entities' failed"),
        "{message}"
    );
    assert!(!message.contains("handshake"), "{message}");
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "an entry refused at any step of its load leaves nothing loaded"
)]
fn a_named_entry_whose_declaration_cannot_be_read_is_not_left_loaded() {
    let dir = TempDir::new().unwrap();
    install_module(dir.path(), "@acme/a", b"\0asm a");
    let builtins = Builtins(&[("@acme/b", &b"\0asm b"[..])]);
    let runtime = describe_fails(
        InProcessRuntime::new()
            .binary(b"\0asm a", extension("@acme/a"))
            .binary(b"\0asm b", extension("@acme/b")),
        &["@acme/a", "@acme/b"],
    );

    let loaded =
        Installed::at(dir.path()).load(&entries(&["@acme/a", "@acme/b"]), &builtins, &runtime);

    for entry in &loaded.enabled {
        assert!(
            matches!(
                entry.failure.as_ref().map(|f| &f.problem),
                Some(LoadProblem::NoDeclaration { .. })
            ),
            "{entry:?}"
        );
    }
    assert!(!runtime.unload("@acme/a"));
    assert!(!runtime.unload("@acme/b"));
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "an entry's declaration is read once, with one handshake"
)]
fn a_file_entry_is_handshaken_once() {
    let dir = TempDir::new().unwrap();
    file(dir.path(), "tool.wasm", b"\0asm tool");
    let runtime = InProcessRuntime::new().binary(b"\0asm tool", extension("@acme/tool"));

    let loaded = load_with(dir.path(), &entries(&["tool.wasm"]), &runtime);

    assert!(
        loaded.enabled[0].failure.is_none(),
        "{:?}",
        loaded.enabled[0]
    );
    let handshakes = runtime
        .calls()
        .iter()
        .filter(|call| call.export == "__handshake")
        .count();
    assert_eq!(handshakes, 1);
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "a .wasm file entry that cannot be read is E028 saying it cannot be read"
)]
fn a_file_entry_that_cannot_be_read_says_so() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join("d.wasm")).unwrap();

    let loaded = load_with(dir.path(), &entries(&["d.wasm"]), &InProcessRuntime::new());

    let failure = loaded.enabled[0].failure.as_ref().unwrap();
    assert!(
        matches!(failure.problem, LoadProblem::ModuleUnreadable { .. }),
        "{:?}",
        failure.problem
    );
    let message = &failure.diagnostic.message;
    assert!(message.contains("extension entry 'd.wasm'"), "{message}");
    assert!(message.contains("cannot be read"), "{message}");
    assert!(!message.contains("does not exist"), "{message}");
    assert!(
        failure
            .diagnostic
            .suggestion
            .as_deref()
            .unwrap()
            .contains("readable"),
        "{:?}",
        failure.diagnostic
    );
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "a .wasm file entry's load warnings name the entry"
)]
fn a_file_entrys_load_warnings_name_the_entry() {
    let dir = TempDir::new().unwrap();
    file(dir.path(), "typo.wasm", b"\0asm typo");
    let typo = || {
        let mut builder = ContributionsBuilder::new(ExtensionMeta::new("@acme/typo", "0.1.0"));
        builder.raw_category(
            DeclaredCategory::Entities,
            json!([{"name": "memo", "testabel": true}]),
        );
        builder
    };
    let runtime = InProcessRuntime::new().binary(b"\0asm typo", typo);

    let loaded = load_with(dir.path(), &entries(&["typo.wasm"]), &runtime);

    assert!(
        loaded.enabled[0].failure.is_none(),
        "{:?}",
        loaded.enabled[0]
    );
    assert_eq!(loaded.enabled[0].name, "@acme/typo");
    let warnings = &loaded.enabled[0].warnings;
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].code, "W138");
    assert!(
        warnings[0].message.contains("extension 'typo.wasm'"),
        "{:?}",
        warnings[0]
    );
}
