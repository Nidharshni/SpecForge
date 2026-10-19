//! What only the CLI prints (search, publish, extension validate) writes a diagnostic as every command does, and
//! carries no `null` (ADR 0052 D1).

use crate::support::{Cli, TestProject};
use specforge_repo_root::repo_root;
use specforge_test_macros::test as specforge_test;

/// The sorted keys of one diagnostic `check --format json` writes without its `data`.
fn shared_keys() -> Vec<String> {
    let dir = TestProject::new().without_config().files(&[(
        "main.spec",
        "feature alpha \"A\" { problem \"p\" }\nbehavior gamma \"G\" { features [alpha, nope] }\n",
    )]);
    let json = Cli::on(&dir).document(&["check"]).json;
    keys_of(&json[0])
}

fn keys_of(diagnostic: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = diagnostic
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| *key != "data")
        .cloned()
        .collect();
    keys.sort();
    keys
}

#[test]
#[ignore = "flipped by T5"]
fn search_writes_its_diagnostics_as_check_does() {
    let dir = TestProject::new().config(|c| {
        c["registries"] = serde_json::json!([
            {"alias": "down", "url": "http://127.0.0.1:9", "default_registry": true}
        ]);
    });

    let document = Cli::on(&dir).document(&["search", "x"]);

    assert_eq!(document.code, 1);
    assert!(
        document.json.get("contributes").is_none(),
        "{}",
        document.json
    );
    let diagnostic = &document.json["diagnostics"][0];
    assert_eq!(keys_of(diagnostic), shared_keys(), "{diagnostic}");
    assert_eq!(diagnostic["severity"], "Error");
}

#[specforge_test(
    behavior = "validate_wasm_extension_locally",
    verify = "specforge extension validate writes its diagnostics as check does"
)]
fn validate_writes_its_diagnostics_as_check_does() {
    let dir = TestProject::new().without_config();
    let formal = repo_root().join("extensions/formal/wasm/specforge_ext_formal.wasm");

    let output = Cli::on(&dir)
        .bare_command(&["extension", "validate", "--format", "json", "--dir"])
        .arg(formal)
        .output()
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostic = &json["diagnostics"][0];
    assert_eq!(keys_of(diagnostic), shared_keys(), "{diagnostic}");
    assert_eq!(diagnostic["severity"], "Info");
}
