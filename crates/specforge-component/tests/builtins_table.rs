//! The builtin table (`specforge_component::builtins::BUILTINS`) is the one list of builtins: it is
//! the `extensions/` directory, by the naming rule, and the (name, bytes) view the extension load
//! reads is its rows.

use specforge_component::builtins::{BUILTIN_EXTENSIONS, BUILTINS};
use specforge_test_macros::test as specforge_test;
use std::path::Path;

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "every builtin is listed once, by the directory its crate lives in"
)]
fn the_table_is_the_extensions_directory() {
    let extensions = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions");
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
