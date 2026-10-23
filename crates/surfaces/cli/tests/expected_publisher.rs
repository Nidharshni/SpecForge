//! An unsigned package is held to its name's expected publisher (ADR 0221). Process tests: the user's pins
//! are read from `HOME` (ADR 0029 D9a).

use crate::registry::known_keys;
use crate::support::mcp_process::{initialize, parse_tool_content, request, session};
use crate::support::registry::{Package, greet_wasm, project_on, serve};
use crate::support::{Cli, TestProject};
use serde_json::{Value, json};
use specforge_registry_server::testing::LocalRegistry;
use specforge_test_macros::test as specforge_test;

/// `specforge <args> --format json`: its exit status and parsed stdout.
fn run(dir: &TestProject, args: &[&str]) -> (bool, Value) {
    let mut args = args.to_vec();
    args.extend(["--format", "json"]);
    let output = Cli::on(dir).command(&args).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json = serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    (output.status.success(), json)
}

/// The user pins `@sdk/greet` to `key_id`.
fn pin(dir: &TestProject, key_id: &str) {
    let mut known = specforge_registry_client::KnownKeys::default();
    known.pin("@sdk/greet", key_id);
    specforge_registry_client::save_known_keys_at(&known_keys(dir.home()), &known).unwrap();
}

/// The requests that downloaded a package.
fn downloads(registry: &LocalRegistry) -> Vec<String> {
    registry
        .requests()
        .into_iter()
        .filter(|r| r.ends_with("/download"))
        .collect()
}

/// One `specforge.add_extension` call over MCP: the tool's content.
fn mcp_add(dir: &TestProject, arguments: Value) -> Value {
    let replies = session(
        dir,
        dir.root(),
        ".",
        &[
            initialize(1),
            request(
                2,
                "tools/call",
                json!({"name": "specforge.add_extension", "arguments": arguments}),
            ),
        ],
        &[],
    );
    let reply = replies
        .iter()
        .find(|r| r["id"] == 2)
        .expect("a reply to the call");
    parse_tool_content(reply)
}

#[specforge_test(
    invariant = "publisher_trust",
    verify = "an unsigned package is refused for a name whose publisher key is pinned, by add, its dry run and MCP add_extension, downloading nothing"
)]
fn an_unsigned_package_for_a_pinned_name_is_refused_on_every_surface() {
    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]); // unsigned
    let dir = project_on(&registry);
    pin(
        &dir,
        &specforge_registry_client::SigningKey::generate().key_id(),
    );
    let store = std::fs::read(known_keys(dir.home())).unwrap();

    for extra in [
        &["--allow-unsigned"][..],
        &["--allow-unsigned", "--yes"],
        &["--allow-unsigned", "--dry-run"],
    ] {
        let mut args = vec!["add", "@sdk/greet@0.1.0"];
        args.extend(extra);
        let (ok, json) = run(&dir, &args);
        assert!(!ok, "{extra:?}: {json}");
        assert_eq!(json["code"], "R-TRUST-007", "{extra:?}: {json}");
    }
    let error = mcp_add(&dir, json!({"specifiers": ["@sdk/greet@0.1.0"]}));
    assert_eq!(error["diagnostic"]["code"], "R-TRUST-007", "{error}");

    assert!(!dir.root().join("specforge.lock").exists());
    assert_eq!(std::fs::read(known_keys(dir.home())).unwrap(), store);
    assert_eq!(downloads(&registry), Vec::<String>::new());
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "with no pin of the user's, an unsigned version of a name specforge.lock records a key for is refused by add and update, and the lock keeps its key"
)]
fn an_unsigned_version_of_a_locked_name_is_refused_and_the_lock_keeps_its_key() {
    let key = specforge_registry_client::SigningKey::generate();
    let registry = serve(vec![
        Package::new("@sdk/greet", "0.1.0", greet_wasm()).signed_by(&key),
        Package::new("@sdk/greet", "0.2.0", b"\0asm greet 0.2.0".to_vec()), // unsigned
    ]);
    let dir = project_on(&registry);
    let (ok, json) = run(&dir, &["add", "@sdk/greet@0.1.0"]);
    assert!(ok, "{json}");
    // A teammate's machine: no pins of the user's, the project's lock only.
    std::fs::remove_file(known_keys(dir.home())).unwrap();
    let lock_path = dir.root().join("specforge.lock");
    let lock = std::fs::read_to_string(&lock_path).unwrap();
    assert!(lock.contains(&key.key_id()), "{lock}");

    let (ok, json) = run(&dir, &["add", "@sdk/greet@0.2.0", "--allow-unsigned"]);
    assert!(!ok, "{json}");
    assert_eq!(json["code"], "R-TRUST-007", "{json}");
    let expected = format!(
        "specforge.lock records its publisher key '{}'",
        key.key_id()
    );
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains(&expected),
        "{json}"
    );

    let (ok, json) = run(&dir, &["update", "--major", "--allow-unsigned"]);
    assert!(!ok, "{json}");
    let greet = json["extensions"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["name"] == "@sdk/greet"))
        .unwrap_or_else(|| panic!("no greet row: {json}"));
    assert_eq!(greet["status"], "failed", "{json}");
    assert_eq!(greet["code"], "R-TRUST-007", "{json}");

    assert_eq!(std::fs::read_to_string(&lock_path).unwrap(), lock);
    assert!(
        !downloads(&registry)
            .iter()
            .any(|r| r.ends_with("/0.2.0/download")),
        "{:?}",
        registry.requests()
    );
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "add_extension takes no allow_unsigned: an unsigned package is refused with R-TRUST-001 naming the terminal command"
)]
fn mcp_add_extension_accepts_no_unsigned_package() {
    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]); // unsigned
    let dir = project_on(&registry);

    let error = mcp_add(&dir, json!({"specifiers": ["@sdk/greet@0.1.0"]}));
    assert_eq!(error["diagnostic"]["code"], "R-TRUST-001", "{error}");
    let suggestion = error["diagnostic"]["suggestion"]
        .as_str()
        .unwrap_or_default();
    assert!(
        suggestion.contains("`specforge add --allow-unsigned`")
            && suggestion.contains("in a terminal"),
        "{error}"
    );

    let error = mcp_add(
        &dir,
        json!({"specifiers": ["@sdk/greet@0.1.0"], "allow_unsigned": true}),
    );
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert!(
        error
            .to_string()
            .contains("unknown argument 'allow_unsigned'"),
        "{error}"
    );
    assert!(!dir.root().join("specforge.lock").exists());
    assert_eq!(downloads(&registry), Vec::<String>::new());
}
