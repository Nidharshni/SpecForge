// A `.wasm` file entry never loads under a builtin's name (ADR 0214).

use specforge_installed::{Builtins, Installed, Origin};
use specforge_registry::PlainDisk;
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

use crate::load::{entries, extension, file, reported};

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "a .wasm file entry that declares a builtin's name is E028 naming the entry, and only the builtin loads under that name"
)]
#[ignore = "flipped by T2"]
fn a_file_entry_declaring_a_builtins_name_is_refused() {
    let dir = TempDir::new().unwrap();
    file(dir.path(), "impostor.wasm", b"\0asm impostor");
    let builtins = Builtins(&[("@acme/core", &b"\0asm core"[..])]);
    let runtime = InProcessRuntime::new()
        .binary(b"\0asm impostor", extension("@acme/core"))
        .binary(b"\0asm core", extension("@acme/core"));
    for all in [
        entries(&["impostor.wasm"]),
        entries(&["@acme/core=impostor.wasm"]),
        entries(&["impostor.wasm", "@acme/core"]),
    ] {
        let loaded = Installed::at(dir.path()).load(&all, &builtins, &runtime, &PlainDisk);
        let refused = &loaded.enabled[0];
        assert!(!refused.loaded, "{all:?}: {refused:?}");
        let reports = reported(dir.path(), &loaded);
        assert_eq!(reports.len(), 1, "{all:?}: {reports:?}");
        assert_eq!(reports[0].code, "E028");
        assert!(
            reports[0]
                .message
                .contains(&format!("extension entry '{}'", all[0])),
            "{:?}",
            reports[0]
        );
        assert!(
            reports[0]
                .message
                .contains("the name of a builtin extension"),
            "{:?}",
            reports[0]
        );
        // Only the builtin, when an entry names it, loads under its name.
        let builtin_loaded = loaded
            .enabled
            .iter()
            .any(|e| e.loaded && e.origin == Origin::Builtin);
        assert_eq!(loaded.declarations.len(), usize::from(builtin_loaded));
    }
}
