//! What `specforge extensions` and `specforge remove --dry-run` say about rows that are only
//! enabled, not installed, and about a file entry that never loaded (behaviors
//! list_installed_extensions and remove_extension).

use crate::support::{Cli, TestProject};
use serde_json::json;
use specforge_test_macros::test as specforge_test;

/// A builtin enabled, a `.wasm` entry whose file is missing, a named entry nothing installs.
fn project() -> TestProject {
    TestProject::new()
        .config(|c| c["extensions"] = json!(["@specforge/product", "gone.wasm", "@acme/x@1.0.0"]))
        .file("spec/a.spec", "feature login \"Login\" {\n}\n")
}

fn stdout(dir: &TestProject, args: &[&str]) -> String {
    let output = Cli::on(dir).command(args).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[specforge_test(
    behavior = "list_installed_extensions",
    verify = "the human listing names its rows the project's extensions, never installed ones, and counts those loaded"
)]
fn the_listing_does_not_call_enabled_rows_installed() {
    let text = stdout(&project(), &["extensions"]);
    assert!(!text.contains("installed"), "{text}");
    assert!(text.starts_with("Extensions of the project:\n"), "{text}");
    assert!(text.ends_with("\n3 extension(s), 1 loaded.\n"), "{text}");
}

#[specforge_test(
    behavior = "list_installed_extensions",
    verify = "the human listing counts one entity as one entity"
)]
fn one_entity_is_one_entity() {
    let text = stdout(&project(), &["extensions"]);
    assert!(text.contains("(builtin): 1 entity ("), "{text}");
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "a dry-run removal of a .wasm file entry names the entry without saying it loaded"
)]
fn a_dry_run_removal_of_a_file_entry_that_never_loaded_does_not_say_it_loaded() {
    assert_eq!(
        stdout(&project(), &["remove", "gone.wasm", "--dry-run"]),
        "Would disable extension 'gone.wasm' (the .wasm file entry gone.wasm; the file is left in place)\n"
    );
}
