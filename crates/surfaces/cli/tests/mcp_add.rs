//! `specforge.add_extension` runs the add `specforge add` runs (plan 03,
//! O4.3): builtins offline, the ADR-0001 diamond gate, and a config
//! `specforge check` accepts.

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
            "behavior alpha \"Alpha\" {\n  category core\n  contract \"The system MUST work\"\n}\n",
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
