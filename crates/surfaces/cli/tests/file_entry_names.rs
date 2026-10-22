//! A .wasm file entry's declared name is a package name, on every surface that reads the load
//! (ADR 0223).

use crate::support::{Cli, TestProject};
use serde_json::{Value, json};
use specforge_test_macros::test as specforge_test;

/// The committed greet component declaring `name` (ten bytes, the length of "@sdk/greet") in its place.
fn greet_declaring(name: &[u8; 10]) -> Vec<u8> {
    let blob = specforge_component::committed::GREET.read();
    let at = blob.windows(10).position(|w| w == b"@sdk/greet").unwrap();
    assert_eq!(blob.windows(10).filter(|w| *w == b"@sdk/greet").count(), 1);
    [&blob[..at], &name[..], &blob[at + 10..]].concat()
}

fn project(entries: &[&str]) -> TestProject {
    let project = TestProject::new().without_config();
    project.write(
        "specforge.json",
        json!({"name": "p", "version": "0.1.0", "spec_root": "spec", "extensions": entries})
            .to_string(),
    );
    project
}

/// `(name, source, status)` of each row of `rows`.
fn rows(rows: &Value) -> Vec<(String, String, String)> {
    let text = |row: &Value, key: &str| row[key].as_str().unwrap_or("None").to_string();
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| (text(row, "name"), text(row, "source"), text(row, "status")))
        .collect()
}

fn row(name: &str, source: &str, status: &str) -> (String, String, String) {
    (name.into(), source.into(), status.into())
}

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "check, doctor and the extensions listing report a .wasm file entry declaring no package name as E072 on its entry, not loaded"
)]
fn a_file_entry_declaring_no_package_name_is_refused_by_check_doctor_and_the_listing() {
    let project = project(&["x.wasm"]);
    project.write("x.wasm", greet_declaring(b"@SDK/greet"));
    let cli = Cli::on(&project);

    let check = cli.run(&["check", "--format", "json"]).exits(1).json();
    let errors: Vec<&Value> = check
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["severity"] == "Error")
        .collect();
    assert_eq!(errors.len(), 1, "{check}");
    assert_eq!(errors[0]["code"], "E072");
    assert!(
        errors[0]["message"]
            .as_str()
            .unwrap()
            .contains("extension entry 'x.wasm'"),
        "{check}"
    );
    let listed = rows(&cli.document(&["extensions"]).json["extensions"]);
    assert_eq!(listed, [row("x.wasm", "file:x.wasm", "not_loaded")]);
    cli.run(&["doctor", "--format", "json"]).exits(1);
}

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "two .wasm file entries each run their own component in either order, when one declares the other's entry text"
)]
fn two_file_entries_keep_their_own_extensions_in_either_order() {
    for entries in [["first.wasm", "second.wasm"], ["second.wasm", "first.wasm"]] {
        let project = project(&entries);
        project.write("second.wasm", greet_declaring(b"first.wasm"));
        project.write("first.wasm", specforge_component::committed::DOCREF.read());
        let cli = Cli::on(&project);

        let mut listed = rows(&cli.document(&["extensions"]).json["extensions"]);
        listed.sort();
        assert_eq!(
            listed,
            [
                row("@sdk/docref", "file:first.wasm", "loaded"),
                row("first.wasm", "file:second.wasm", "loaded"),
            ],
            "{entries:?}"
        );
        let greeted = cli.run(&["greet", "hello", "bob"]).exits(0);
        assert_eq!(greeted.stdout, "Hello, bob!\n", "{entries:?}");
    }
}
