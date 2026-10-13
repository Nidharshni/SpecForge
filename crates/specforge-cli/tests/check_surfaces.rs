//! `specforge check` and MCP `specforge.validate`, characterized
//! (architecture plan 08, T0).
//!
//! What each surface answers on the parity fixtures, pinned as insta
//! snapshots before the two surfaces move onto one check operation: the
//! CLI's exit code and JSON output, with and without `--strict`, its lint
//! profile and build cache handling, and MCP validate's whole result for
//! each argument set it takes. A later ticket that changes an answer on
//! purpose updates its snapshot in the same diff.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

use crate::parity::normalized;

/// The parity fixtures these tests run: clean, warnings only
/// (`body_parser_type`), errors, warnings and infos (`product_cycle`), and a
/// resolver error (`missing_import`).
const FIXTURES: &[&str] = &[
    "clean",
    "body_parser_type",
    "product_cycle",
    "missing_import",
];

const CACHE: &str = "specforge-cache.json";

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parity")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A copy of the parity fixture `name` in a temporary directory.
fn fixture(name: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    copy_tree(&fixtures_dir().join(name), dir.path());
    dir
}

/// `specforge check <root> <args>`: exit code, stdout (parsed when it is
/// JSON) and stderr.
fn check(root: &Path, args: &[&str]) -> (i32, Value, String) {
    let out = Command::new(assert_cmd::cargo_bin!("specforge"))
        .env("HOME", std::env::temp_dir().join("specforge-parity-home"))
        .arg("check")
        .args(args)
        .arg("--path")
        .arg(root)
        .current_dir(root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stdout = serde_json::from_str(&stdout).unwrap_or(Value::String(stdout));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.code().unwrap_or(-1), stdout, stderr)
}

fn suffix(flags: &[&str]) -> String {
    flags
        .iter()
        .map(|f| f.trim_start_matches('-').replace('-', "_"))
        .collect::<Vec<_>>()
        .join("_")
}

#[test]
fn cli_check_json_golden() {
    for name in FIXTURES {
        for flags in [&[][..], &["--strict"][..]] {
            let dir = fixture(name);
            let mut args = vec!["--format", "json"];
            args.extend(flags);
            let (exit, stdout, stderr) = check(dir.path(), &args);
            let doc = json!({"exit": exit, "stdout": stdout, "stderr": stderr});
            let snapshot = if flags.is_empty() {
                format!("cli_check_json_{name}")
            } else {
                format!("cli_check_json_{name}_{}", suffix(flags))
            };
            insta::assert_snapshot!(snapshot, normalized(&doc, dir.path()));
        }
    }
}

/// The human rendering (no TTY, so no colour): the annotated diagnostics
/// and the summary line, byte for byte (architecture plan 04, T0).
#[test]
fn cli_check_human_golden() {
    for name in FIXTURES {
        let dir = fixture(name);
        let (exit, _, stderr) = check(dir.path(), &[]);
        let doc = json!({"exit": exit, "stderr": stderr});
        insta::assert_snapshot!(
            format!("cli_check_human_{name}"),
            normalized(&doc, dir.path())
        );
    }
}

#[test]
fn cli_check_lint_values_golden() {
    for profile in ["inferred", "pedantic", "nonsense"] {
        let dir = fixture("body_parser_type");
        let (exit, stdout, _) = check(dir.path(), &["--format", "json", "--lint", profile]);
        let doc = json!({"exit": exit, "stdout": stdout});
        insta::assert_snapshot!(
            format!("cli_check_lint_{profile}"),
            normalized(&doc, dir.path())
        );
    }
}

#[test]
fn cli_check_cache_golden() {
    let runs: [(&str, &[&str]); 3] = [
        ("body_parser_type", &["--cache"]),
        ("body_parser_type", &["--strict", "--cache"]),
        ("product_cycle", &["--cache"]),
    ];
    for (name, flags) in runs {
        let dir = fixture(name);
        let mut args = vec!["--format", "json"];
        args.extend(flags);
        let (exit, _, stderr) = check(dir.path(), &args);
        let cache_file = std::fs::read_to_string(dir.path().join(CACHE))
            .map(Value::String)
            .unwrap_or(Value::Null);
        let doc = json!({"exit": exit, "stderr": stderr, "cache_file": cache_file});
        insta::assert_snapshot!(
            format!("cli_check_cache_{name}_{}", suffix(flags)),
            normalized(&doc, dir.path())
        );
    }
}

/// An in-process MCP server serving `root`, initialized.
fn server(root: &Path) -> specforge_mcp::McpServer {
    let mut server = specforge_mcp::McpServer::with_project_root(root.to_path_buf());
    let init = json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}});
    server.handle_message(&init.to_string());
    server
}

/// `specforge.validate` with `arguments`: the whole `result`, its text
/// block parsed, or the JSON-RPC error.
fn validate(server: &mut specforge_mcp::McpServer, arguments: &Value) -> Value {
    let req = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "specforge.validate", "arguments": arguments}
    });
    let resp: Value = serde_json::from_str(&server.handle_message(&req.to_string()).unwrap())
        .expect("the server answers JSON");
    if let Some(error) = resp.get("error") {
        return json!({"error": error});
    }
    let mut result = resp["result"].clone();
    if let Some(blocks) = result["content"].as_array_mut() {
        for block in blocks {
            if let Some(text) = block["text"].as_str()
                && let Ok(parsed) = serde_json::from_str::<Value>(text)
            {
                block["text"] = parsed;
            }
        }
    }
    if result.get("_meta").is_none() {
        result["_meta"] = json!("<absent>");
    }
    result
}

/// Every argument set the pins call validate with.
fn validate_arguments() -> Vec<Value> {
    vec![
        json!({}),
        json!({"strict": true}),
        json!({"severity_filter": "error"}),
        json!({"severity_filter": "warning"}),
        json!({"severity_filter": "info"}),
        json!({"strict": true, "severity_filter": "error"}),
        json!({"severity_filter": "Error"}),
        json!({"severity_filter": "errors"}),
        json!({"lint": ["nonsense"]}),
    ]
}

#[test]
fn mcp_validate_golden() {
    for name in FIXTURES {
        let dir = fixture(name);
        let mut server = server(dir.path());
        let calls: Vec<Value> = validate_arguments()
            .into_iter()
            .map(|arguments| {
                let result = validate(&mut server, &arguments);
                json!({"arguments": arguments, "result": result})
            })
            .collect();
        insta::assert_snapshot!(
            format!("mcp_validate_{name}"),
            normalized(&Value::Array(calls), dir.path())
        );
    }
}
