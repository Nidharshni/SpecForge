//! `specforge.add_extension` runs the add `specforge add` runs (plan 03,
//! O4.3): builtins offline, the ADR-0001 diamond gate, and a config
//! `specforge check` accepts.

use crate::support::registry::{Package, serve};
use crate::support::{Cli, TestProject};
use serde_json::{Value, json};
use specforge_mcp::testing::Served;
use specforge_test_macros::test as specforge_test;
use std::path::Path;

/// The reply to `specforge.add_extension` with `arguments`, on a server serving `project` as `specforge mcp
/// <root>` does.
fn mcp_add(served: &mut Served, arguments: Value) -> Value {
    served.call_tool("specforge.add_extension", arguments)
}

/// What the add did for its one specifier: the first entry of the reply's `added`.
fn payload(reply: &Value) -> Value {
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no result: {reply}"));
    let document: Value = serde_json::from_str(text).unwrap();
    document["added"][0].clone()
}

/// `(ok, error count)` of a fresh `specforge check` of `root`.
pub(crate) fn check(project: &TestProject) -> (bool, Vec<String>) {
    let out = Cli::on(project).run(&["check", "--format", "json"]);
    let diagnostics: Vec<Value> = serde_json::from_str(&out.stdout).unwrap_or_default();
    let errors = diagnostics
        .iter()
        .filter(|d| d["severity"] == "Error" || d["severity"] == "error")
        .map(|d| format!("{} {}", d["code"], d["message"]))
        .collect();
    (out.code == 0, errors)
}

fn project(extensions: &[&str], registries: Value) -> TestProject {
    TestProject::new()
        .config(|c| {
            c["name"] = "p".into();
            c["spec_root"] = "spec".into();
            c["registries"] = registries;
        })
        .enabling(extensions)
        .file(
            "spec/main.spec",
            "behavior alpha \"Alpha\" {\n  category \"core\"\n  contract \"The system MUST work\"\n}\n",
        )
}

fn extensions(root: &Path) -> Vec<String> {
    let config: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("specforge.json")).unwrap())
            .unwrap();
    config["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "a builtin is enabled with no registry and no network"
)]
fn mcp_enables_a_builtin_offline() {
    // No registry configured: the old handler asked one and failed.
    let mut dir = Served::rooted(project(&["@specforge/software"], json!([])));

    let reply = mcp_add(&mut dir, json!({"specifiers": ["@specforge/cargo-test"]}));

    let added = payload(&reply);
    assert_eq!(added["source"], "builtin", "{added}");
    assert_eq!(added["action"], "enabled", "{added}");
    // Its required builtin peer comes first, as with `specforge add`.
    assert_eq!(
        extensions(dir.root()),
        [
            "@specforge/software",
            "@specforge/testing",
            "@specforge/cargo-test"
        ]
    );
}

#[specforge_test(
    invariant = "init_config_validity",
    verify = "specforge add or the MCP add_extension tool followed by specforge check produces zero errors"
)]
fn mcp_add_then_check_is_clean() {
    let mut dir = Served::rooted(project(&["@specforge/software"], json!([])));

    let reply = mcp_add(&mut dir, json!({"specifiers": ["@specforge/product"]}));
    assert!(reply["error"].is_null(), "{reply}");

    let (ok, errors) = check(dir.project());
    assert!(ok && errors.is_empty(), "{errors:?}");
}

#[specforge_test(
    invariant = "init_config_validity",
    verify = "specforge add or the MCP add_extension tool followed by specforge check produces zero errors"
)]
fn mcp_local_add_then_check_loads_the_extension() {
    let mut dir = Served::rooted(
        project(&["@specforge/software"], json!([]))
            .file(
                "spec/greet.spec",
                "greeting hello \"Hello\" {\n  style warm\n}\n",
            )
            .file("greet.wasm", crate::support::registry::greet_wasm()),
    );
    let wasm = dir.root().join("greet.wasm");

    // It used to write `greet@0.0.0`, which check failed with E028.
    let reply = mcp_add(&mut dir, json!({"specifiers": [wasm.to_str().unwrap()]}));
    let added = payload(&reply);
    assert_eq!(added["extension"], "@sdk/greet", "{added}");
    assert_eq!(added["version"], "0.1.0", "{added}");
    assert_eq!(
        extensions(dir.root()),
        ["@specforge/software", "@sdk/greet"]
    );

    let (ok, errors) = check(dir.project());
    assert!(ok && errors.is_empty(), "{errors:?}");
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "a version diamond with a locked peer is refused with R-RES-006, as specforge add refuses it"
)]
fn mcp_add_refuses_a_version_diamond() {
    let registry = serve(vec![
        Package::new("@acme/base", "1.0.0", b"base".to_vec()),
        Package::new("@acme/base", "2.0.0", b"base".to_vec()),
        Package::new("@acme/app", "1.0.0", b"app".to_vec()).with_peer("@acme/base", "^2.0"),
    ]);
    let dir = project(
        &["@specforge/software", "@acme/base", "@acme/other"],
        registry.config_entry(),
    );
    // `@acme/base` is locked at 1.0.0 for `@acme/other`, which takes any 1+.
    dir.write(
        "specforge.lock",
        json!({"lockfile_version": 1, "entries": [
            {"name": "@acme/base", "version": "1.0.0", "source": "registry", "wasm_hash": ""},
            {"name": "@acme/other", "version": "1.0.0", "source": "registry", "wasm_hash": "",
             "peer_dependencies": [{"name": "@acme/base", "version": ">=1.0.0"}]},
        ]})
        .to_string(),
    );
    let mut dir = Served::rooted(dir);
    let lock_before = std::fs::read(dir.root().join("specforge.lock")).unwrap();

    let reply = mcp_add(
        &mut dir,
        json!({"specifiers": ["@acme/app@1.0.0"], "allow_unsigned": true}),
    );

    let error = crate::e2e_fixtures::tool_error(&reply);
    assert_eq!(error["diagnostic"]["code"], "R-RES-006", "{reply}");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("@acme/base 2.0.0"), "{message}");
    assert_eq!(
        std::fs::read(dir.root().join("specforge.lock")).unwrap(),
        lock_before,
        "nothing is installed"
    );
    assert!(!dir.root().join(".specforge/extensions/@acme/app").exists());
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an install that leaves a locked extension's peer unsatisfied is refused before anything is written, local or from a registry"
)]
fn mcp_add_refuses_an_install_a_locked_extension_does_not_accept() {
    let registry = serve(vec![Package::new(
        "@sdk/greet",
        "0.1.0",
        crate::support::registry::greet_wasm(),
    )]);
    let dir = project(
        &["@specforge/software", "@acme/app"],
        registry.config_entry(),
    );
    // `@acme/app` is locked and wants `@sdk/greet ^2.0`, which the registry never published.
    dir.write(
        "specforge.lock",
        json!({"lockfile_version": 1, "entries": [
            {"name": "@acme/app", "version": "1.0.0", "source": "registry", "wasm_hash": "",
             "peer_dependencies": [{"name": "@sdk/greet", "version": "^2.0"}]},
        ]})
        .to_string(),
    );
    let mut dir = Served::rooted(dir);
    let lock_before = std::fs::read(dir.root().join("specforge.lock")).unwrap();

    let reply = mcp_add(
        &mut dir,
        json!({"specifiers": ["@sdk/greet@0.1.0"], "allow_unsigned": true}),
    );

    let error = crate::e2e_fixtures::tool_error(&reply);
    assert_eq!(error["diagnostic"]["code"], "R-RES-005", "{reply}");
    let message = error["message"].as_str().unwrap();
    assert!(
        message.contains("installing '@sdk/greet' 0.1.0 breaks '@acme/app': "),
        "{message}"
    );
    assert_eq!(
        message.matches("@acme/app wants @sdk/greet ^2.0").count(),
        1,
        "{message}"
    );
    assert_eq!(
        std::fs::read(dir.root().join("specforge.lock")).unwrap(),
        lock_before,
        "nothing is installed"
    );
    assert!(!dir.root().join(".specforge/extensions/@sdk/greet").exists());
}
