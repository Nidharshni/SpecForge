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
use specforge_mcp::testing::{Served, rpc};
use std::path::{Path, PathBuf};

use crate::parity::normalized;
use crate::support::{Cli, TestProject};

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

/// A copy of the parity fixture `name` in a temporary directory.
fn fixture(name: &str) -> TestProject {
    TestProject::copy_of(fixtures_dir().join(name))
}

/// `specforge check <root> <args>`: exit code, stdout (parsed when it is
/// JSON) and stderr.
fn check(project: &TestProject, args: &[&str]) -> (i32, Value, String) {
    let mut argv = vec!["check"];
    argv.extend(args);
    let out = Cli::on(project).run(&argv);
    let stdout = serde_json::from_str(&out.stdout).unwrap_or(Value::String(out.stdout));
    (out.code, stdout, out.stderr)
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
            let (exit, stdout, stderr) = check(&dir, &args);
            let doc = json!({"exit": exit, "stdout": stdout, "stderr": stderr});
            let snapshot = if flags.is_empty() {
                format!("cli_check_json_{name}")
            } else {
                format!("cli_check_json_{name}_{}", suffix(flags))
            };
            insta::assert_snapshot!(snapshot, normalized(&doc, dir.root()));
        }
    }
}

/// The human rendering (no TTY, so no colour): the annotated diagnostics
/// and the summary line, byte for byte (architecture plan 04, T0).
#[test]
fn cli_check_human_golden() {
    for name in FIXTURES {
        let dir = fixture(name);
        let (exit, _, stderr) = check(&dir, &[]);
        let doc = json!({"exit": exit, "stderr": stderr});
        insta::assert_snapshot!(
            format!("cli_check_human_{name}"),
            normalized(&doc, dir.root())
        );
    }
}

#[test]
fn cli_check_lint_values_golden() {
    for profile in ["inferred", "pedantic", "nonsense"] {
        let dir = fixture("body_parser_type");
        let (exit, stdout, _) = check(&dir, &["--format", "json", "--lint", profile]);
        let doc = json!({"exit": exit, "stdout": stdout});
        insta::assert_snapshot!(
            format!("cli_check_lint_{profile}"),
            normalized(&doc, dir.root())
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
        let (exit, _, stderr) = check(&dir, &args);
        let cache_file = std::fs::read_to_string(dir.root().join(CACHE))
            .map(Value::String)
            .unwrap_or(Value::Null);
        let doc = json!({"exit": exit, "stderr": stderr, "cache_file": cache_file});
        insta::assert_snapshot!(
            format!("cli_check_cache_{name}_{}", suffix(flags)),
            normalized(&doc, dir.root())
        );
    }
}

/// `specforge.validate` with `arguments`: the whole `result`, its text
/// block parsed, or the JSON-RPC error.
fn validate(server: &mut Served, arguments: &Value) -> Value {
    let resp = rpc::call_tool(server, "specforge.validate", arguments.clone());
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
        let mut server = Served::rooted(fixture(name));
        let root = server.root().to_path_buf();
        let calls: Vec<Value> = validate_arguments()
            .into_iter()
            .map(|arguments| {
                let result = validate(&mut server, &arguments);
                json!({"arguments": arguments, "result": result})
            })
            .collect();
        insta::assert_snapshot!(
            format!("mcp_validate_{name}"),
            normalized(&Value::Array(calls), &root)
        );
    }
}
