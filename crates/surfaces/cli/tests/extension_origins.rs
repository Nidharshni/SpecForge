//! Where an extension comes from is one answer: the listing, doctor and remove name the origin the
//! extension load took (ADR 0028 D3, D9), however the `extensions` entries are ordered.

use crate::support::{Cli, TestProject};
use serde_json::{Value, json};
use specforge_test_macros::test as specforge_test;

/// `@specforge/product`'s component: a `.wasm` file declaring a builtin's name.
fn product_wasm() -> &'static [u8] {
    specforge_component::builtins::BUILTIN_EXTENSIONS
        .iter()
        .find(|(name, _)| *name == "@specforge/product")
        .map(|(_, bytes)| *bytes)
        .unwrap()
}

fn project(entries: Value, lock: Option<Value>) -> TestProject {
    let project = TestProject::new().without_config();
    project.write(
        "specforge.json",
        json!({"name": "p", "version": "0.1.0", "spec_root": "spec", "extensions": entries})
            .to_string(),
    );
    project.write("prod.wasm", product_wasm());
    project.write("greet.wasm", specforge_component::committed::GREET.read());
    if let Some(lock) = lock {
        project.write("specforge.lock", lock.to_string());
    }
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
    behavior = "list_installed_extensions",
    verify = "an extension's source is where the extension load takes it from, whatever order its entries are in"
)]
fn each_source_is_where_the_load_took_the_extension() {
    let stale_lock = json!({"lockfile_version": 1, "entries": [
        {"name": "@specforge/product", "version": "9.9.9", "source": "registry", "wasm_hash": "h"},
        {"name": "@acme/stale", "version": "3.0.0", "source": "local:/x/stale.wasm", "wasm_hash": "h"},
    ]});
    let cases = [
        // A file declaring a builtin's name is refused.
        (
            json!(["prod.wasm"]),
            None,
            vec![row("prod.wasm", "file:prod.wasm", "not_loaded")],
        ),
        // A builtin, and a file declaring it.
        (
            json!(["@specforge/product", "prod.wasm"]),
            None,
            vec![
                row("@specforge/product", "builtin", "loaded"),
                row("prod.wasm", "file:prod.wasm", "not_loaded"),
            ],
        ),
        // Neither loads.
        (
            json!(["@acme/tool=tool.wasm", "@acme/tool"]),
            None,
            vec![row("@acme/tool", "unknown", "not_loaded")],
        ),
        // A repeat, and another text for one file.
        (
            json!(["greet.wasm", "greet.wasm", "./greet.wasm"]),
            None,
            vec![
                row("./greet.wasm", "file:./greet.wasm", "not_loaded"),
                row("@sdk/greet", "file:greet.wasm", "loaded"),
            ],
        ),
        // A builtin's name with a stray lock entry, and a lock entry no entry enables.
        (
            json!(["@specforge/product"]),
            Some(stale_lock),
            vec![
                row("@acme/stale", "local:/x/stale.wasm", "not_configured"),
                row("@specforge/product", "builtin", "loaded"),
            ],
        ),
    ];
    for (entries, lock, expected) in cases {
        let project = project(entries.clone(), lock);
        let cli = Cli::on(&project);
        let listed = rows(&cli.document(&["extensions"]).json["extensions"]);
        let doctor = rows(&cli.document(&["doctor"]).json["extensions"]);
        assert_eq!(listed, expected, "{entries}");
        // Doctor lists what the listing lists, minus rows only the listing keeps.
        assert!(
            doctor.iter().all(|row| listed.contains(row)),
            "{entries}: {doctor:?} vs {listed:?}"
        );

        for (name, _, _) in listed.iter().filter(|(_, _, status)| status == "loaded") {
            let removed = cli.document(&["remove", name, "--dry-run"]);
            // A name two entries enable is refused (extension_conflict): no source to compare.
            if let Some(source) = removed.json["source"].as_str() {
                let loaded = listed
                    .iter()
                    .find(|(listed_name, _, status)| listed_name == name && status == "loaded");
                assert_eq!(
                    Some(source),
                    loaded.map(|(_, source, _)| source.as_str()),
                    "{entries}"
                );
            }
        }
    }
}
