//! The builtin table (`specforge_component::builtins::BUILTINS`) is the one list of builtins: it is
//! the `extensions/` directory, by the naming rule, and the (name, bytes) view the extension load
//! reads is its rows.

use specforge_component::builtins::{BUILTIN_EXTENSIONS, BUILTINS};
use specforge_component::committed::{COMMITTED, FIXTURES};
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "every builtin is listed once, by the directory its crate lives in"
)]
fn the_table_is_the_extensions_directory() {
    let extensions = specforge_repo_root::repo_root().join("extensions");
    let mut on_disk: Vec<String> = std::fs::read_dir(extensions)
        .expect("extensions/")
        .flatten()
        .filter(|entry| entry.path().join("Cargo.toml").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut listed: Vec<&str> = BUILTINS.iter().map(|b| b.dir).collect();
    listed.sort();
    assert_eq!(
        listed, on_disk,
        "BUILTINS lists every crate under extensions/, once"
    );

    for builtin in BUILTINS {
        assert_eq!(
            builtin.name,
            format!("@specforge/{}", builtin.dir),
            "{}",
            builtin.dir
        );
        assert_eq!(
            builtin.artifact,
            format!("specforge_ext_{}.wasm", builtin.dir.replace('-', "_")),
            "{}",
            builtin.dir
        );
        assert!(
            !builtin.bytes.is_empty(),
            "{} embeds its component",
            builtin.dir
        );
    }

    assert_eq!(BUILTIN_EXTENSIONS.len(), BUILTINS.len());
    for (view, builtin) in BUILTIN_EXTENSIONS.iter().zip(BUILTINS) {
        assert_eq!(view.0, builtin.name);
        assert_eq!(view.1, builtin.bytes);
    }
}

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "every fixture crate is listed once, by its directory under fixtures/"
)]
fn the_fixtures_are_the_fixtures_directory() {
    let fixtures = specforge_repo_root::repo_root().join("fixtures");
    let mut on_disk: Vec<String> = std::fs::read_dir(&fixtures)
        .expect("fixtures/")
        .flatten()
        .filter(|entry| entry.path().join("Cargo.toml").is_file())
        .map(|entry| format!("fixtures/{}", entry.file_name().to_string_lossy()))
        .collect();
    on_disk.sort();
    let mut listed: Vec<&str> = FIXTURES.iter().map(|b| b.crate_dir).collect();
    listed.sort();
    assert_eq!(
        listed, on_disk,
        "FIXTURES lists every crate under fixtures/, once"
    );

    // Each committed blob's wasm/ holds its artifact, and `inputs.json` once it is recorded.
    for blob in COMMITTED {
        let wasm = blob.path().parent().expect("a file path").to_path_buf();
        let mut held: Vec<String> = std::fs::read_dir(&wasm)
            .unwrap_or_else(|e| panic!("{}: {e}", wasm.display()))
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "inputs.json")
            .collect();
        held.sort();
        assert_eq!(held, [blob.artifact], "{}: its wasm/ directory", blob.name);
    }
}
