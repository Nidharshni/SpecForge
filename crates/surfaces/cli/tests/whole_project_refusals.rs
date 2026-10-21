//! A refusal of the whole project (its registry configuration, the user's credential store) is the
//! operation's, given once: never once per locked package or per configured registry (plan 10).

use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;

/// Two registry installs: a per-package report prints two rows.
const TWO_LOCKED: &str = r#"{"lockfile_version":1,"entries":[
    {"name":"@acme/a","version":"1.0.0","source":"registry","wasm_hash":"00"},
    {"name":"@acme/b","version":"1.0.0","source":"registry","wasm_hash":"01"}]}"#;

/// A trailing comma: the store exists and does not parse (R012).
const BROKEN_STORE: &str = r#"{"registries": {"main": {"token_env": "P10_TOKEN"},}}"#;

/// Nothing listens here: a request would be a network error (R005), never E067 or R012.
const UNREACHABLE: &str = "http://registry.invalid";

fn project(registries: serde_json::Value) -> TestProject {
    let dir = TestProject::new().raw_config(
        serde_json::json!({
            "name": "demo", "version": "0.1.0", "extensions": [],
            "registries": registries,
        })
        .to_string(),
    );
    dir.write("specforge.lock", TWO_LOCKED);
    dir
}

fn break_the_store(dir: &TestProject) {
    let path = dir.home().join(".specforge/credentials.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, BROKEN_STORE).unwrap();
}

/// `stderr` gives `code` once and charges it to no locked package.
fn refused_once(stderr: &str, code: &str) {
    assert_eq!(
        stderr.matches(&format!("error[{code}]")).count(),
        1,
        "{stderr}"
    );
    assert!(
        !stderr.contains("@acme/a:") && !stderr.contains("@acme/b:"),
        "{stderr}"
    );
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "a refusal of the project's registry configuration or the user's credential store refuses the update once, before any registry is asked"
)]
fn update_refuses_an_unreadable_registries_entry_once() {
    let dir = project(serde_json::json!([
        {"alias": "main", "url": format!("{UNREACHABLE}/v1"), "default_registry": true}
    ]));
    let document = Cli::on(&dir).document(&["update"]);
    assert_eq!(document.code, 1);
    assert_eq!(document.json["code"], "E067", "{}", document.json);
    assert!(
        document.json.get("batch_update_completed").is_none(),
        "{}",
        document.json
    );
    refused_once(&Cli::on(&dir).run(&["update"]).stderr, "E067");
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "a refusal of the project's registry configuration or the user's credential store refuses the update once, before any registry is asked"
)]
fn update_refuses_an_unreadable_credential_store_once() {
    let dir = project(serde_json::json!([
        {"alias": "main", "url": UNREACHABLE, "default_registry": true}
    ]));
    break_the_store(&dir);
    let document = Cli::on(&dir).document(&["update"]);
    assert_eq!(document.code, 1);
    assert_eq!(document.json["code"], "R012", "{}", document.json);
    assert!(
        document.json.get("extensions").is_none(),
        "{}",
        document.json
    );
    refused_once(&Cli::on(&dir).run(&["update"]).stderr, "R012");
}

#[specforge_test(
    behavior = "search_registry",
    verify = "a credential store that can't be read refuses the search once, before any request: it is no registry's failure"
)]
fn search_refuses_an_unreadable_credential_store_once() {
    let dir = project(serde_json::json!([
        {"alias": "one", "url": UNREACHABLE, "default_registry": true},
        {"alias": "two", "url": "http://other.invalid"}
    ]));
    break_the_store(&dir);
    let document = Cli::on(&dir).document(&["search", "x"]);
    assert_eq!(document.code, 1);
    assert_eq!(document.json["code"], "R012", "{}", document.json);
    let stderr = Cli::on(&dir).run(&["search", "x"]).stderr;
    assert_eq!(stderr.matches("error[R012]").count(), 1, "{stderr}");
    assert!(!stderr.contains("search failed on registry"), "{stderr}");
}
