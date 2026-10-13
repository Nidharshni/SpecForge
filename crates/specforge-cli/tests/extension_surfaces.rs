//! What the two surfaces serve from a project's extension commands:
//! `@specforge/product`'s MCP tools as `tools/list` lists them, and, for each,
//! the command line of the same name. The help the host renders from a
//! declaration is pinned once, over a fake, in `src/extension_command.rs`.

use crate::e2e_fixtures::{find_response, mcp_request};
use crate::product_commands::{setup_product_project, structured_session};
use assert_cmd::cargo_bin_cmd;
use serde_json::Value;
use specforge_test_macros::test as specforge_test;
use tempfile::TempDir;

const PRODUCT: &str = "@specforge/product";

/// Every tool `specforge mcp` lists for the product project, in order.
fn listed_tools(dir: &TempDir) -> Vec<Value> {
    let responses = structured_session(dir, &[mcp_request(1, "tools/list", serde_json::json!({}))]);
    find_response(&responses, 1).unwrap()["result"]["tools"]
        .as_array()
        .unwrap()
        .clone()
}

/// The product command a product tool runs, as the command line names it.
fn cli_name(tool: &Value) -> String {
    let name = tool["name"].as_str().unwrap();
    let id = name
        .strip_prefix("specforge.product.")
        .unwrap_or_else(|| panic!("{name} is not a product tool"));
    id.replace('_', "-")
}

/// `specforge product <args>`'s stdout, at a fixed width and without
/// colour.
fn product_help(dir: &TempDir, args: &[&str]) -> String {
    let output = cargo_bin_cmd!("specforge")
        .env("COLUMNS", "100")
        .env("NO_COLOR", "1")
        .arg("product")
        .args(args)
        .arg("--path")
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn pinned_product_tools_listing() {
    let dir = setup_product_project();
    let tools = listed_tools(&dir);
    // The listed order, core first: pinned apart from the schemas.
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    insta::assert_json_snapshot!("product_tool_names", names);
    let product: Vec<&Value> = tools.iter().filter(|t| t["source"] == PRODUCT).collect();
    insta::assert_json_snapshot!("product_tools", product);
}

/// The commands `specforge product --help` lists, in order, without clap's
/// own `help`.
fn listed_commands(help: &str) -> Vec<String> {
    help.lines()
        .skip_while(|line| *line != "Commands:")
        .skip(1)
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| *name != "help")
        .map(str::to_string)
        .collect()
}

#[specforge_test(
    behavior = "auto_promote_commands_to_mcp_tools",
    verify = "each auto-promoted tool is a command line of the same name whose help shows the tool's description and each of its arguments"
)]
fn every_product_tool_is_a_product_command_line() {
    let dir = setup_product_project();
    let tools = listed_tools(&dir);
    let product: Vec<&Value> = tools.iter().filter(|t| t["source"] == PRODUCT).collect();
    assert!(!product.is_empty());
    let commands: Vec<String> = product.iter().map(|t| cli_name(t)).collect();
    // `specforge product <unknown>` would list them too, but as an error:
    // `--help` lists them.
    assert_eq!(listed_commands(&product_help(&dir, &["--help"])), commands);
    for (tool, command) in product.iter().zip(&commands) {
        let help = product_help(&dir, &[command, "--help"]);
        let description = tool["description"].as_str().unwrap();
        assert!(help.starts_with(description), "{command}: {help}");
        assert!(
            help.contains(&format!("Usage: specforge product {command} ")),
            "{help}"
        );
        let schema = &tool["inputSchema"];
        let required: Vec<&str> = schema["required"]
            .as_array()
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        for (name, property) in schema["properties"].as_object().unwrap() {
            let shown = if required.contains(&name.as_str()) {
                format!("<{}>", name.to_uppercase())
            } else {
                format!("--{}", name.replace('_', "-"))
            };
            assert!(help.contains(&shown), "{command}: no {shown} in {help}");
            if let Some(text) = property["description"].as_str() {
                assert!(
                    help.contains(text),
                    "{command}: {name}'s description: {help}"
                );
            }
        }
    }
}

/// `specforge product <args> --format json`'s stdout, as JSON.
fn product_json(dir: &TempDir, args: &[&str]) -> Value {
    let output = cargo_bin_cmd!("specforge")
        .arg("product")
        .args(args)
        .args(["--format", "json", "--path"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{args:?}: {output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "the CLI and MCP send a command's export the same args for the same input, its declared defaults applied by the host"
)]
fn the_cli_and_mcp_answer_one_command_alike() {
    let dir = setup_product_project();
    let calls = [
        (vec!["features"], serde_json::json!({})),
        (
            vec!["features", "--status", "done", "--limit", "1"],
            serde_json::json!({"status": "done", "limit": 1}),
        ),
        (
            vec!["milestone-completion", "m1"],
            serde_json::json!({"milestone": "m1"}),
        ),
    ];
    let requests: Vec<String> = calls
        .iter()
        .enumerate()
        .map(|(i, (argv, arguments))| {
            let name = format!("specforge.product.{}", argv[0].replace('-', "_"));
            mcp_request(
                i as u64 + 1,
                "tools/call",
                serde_json::json!({"name": name, "arguments": arguments}),
            )
        })
        .collect();
    let responses = structured_session(&dir, &requests);
    for (i, (argv, _)) in calls.iter().enumerate() {
        let response = find_response(&responses, i as u64 + 1).unwrap();
        assert_eq!(response["result"]["isError"], false, "{argv:?}: {response}");
        assert_eq!(
            response["result"]["structuredContent"],
            product_json(&dir, argv),
            "{argv:?}"
        );
    }
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "over MCP an argument the command's declaration refuses is the INVALID_INPUT error object the CLI writes, and the export is not called"
)]
fn one_mistake_is_one_error_object_on_both_surfaces() {
    let dir = setup_product_project();
    let mistakes = [
        (
            vec!["features", "--limit", "-1"],
            serde_json::json!({"limit": -1}),
        ),
        (vec!["milestone-completion"], serde_json::json!({})),
        (
            vec!["features", "--status", "bogus"],
            serde_json::json!({"status": "bogus"}),
        ),
    ];
    let requests: Vec<String> = mistakes
        .iter()
        .enumerate()
        .map(|(i, (argv, arguments))| {
            let name = format!("specforge.product.{}", argv[0].replace('-', "_"));
            mcp_request(
                i as u64 + 1,
                "tools/call",
                serde_json::json!({"name": name, "arguments": arguments}),
            )
        })
        .collect();
    let responses = structured_session(&dir, &requests);
    for (i, (argv, _)) in mistakes.iter().enumerate() {
        let output = cargo_bin_cmd!("specforge")
            .arg("product")
            .args(argv)
            .args(["--format", "json", "--path"])
            .arg(dir.path())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{argv:?}: {output:?}");
        let cli: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(cli["code"], "INVALID_INPUT", "{argv:?}: {cli}");
        let response = find_response(&responses, i as u64 + 1).unwrap();
        assert_eq!(response["result"]["isError"], true, "{argv:?}: {response}");
        assert_eq!(response["result"]["structuredContent"], cli, "{argv:?}");
    }
}

/// A project enabling the vendored sandbox probe (`fixtures/sandbox-probe`),
/// whose `probe` command prints its whole input and whose `trap` command
/// traps.
fn probe_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let probe = specforge_component::committed::PROBE.path();
    std::fs::copy(probe, dir.path().join("probe.wasm")).unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":["./probe.wasm"]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("t.spec"),
        "probe_target t1 \"Target\" {\n}\n",
    )
    .unwrap();
    dir
}

/// The product project of the evidence pins: two features, a completed
/// milestone, two behaviors and a recorded report proving one of them.
fn evidence_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":["@specforge/product","@specforge/software","@specforge/testing"]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("main.spec"),
        r#"feature f_proven "Proven" {
  status done
}

feature f_half "Half" {
  status done
}

milestone m1 "One" {
  status completed
  features [f_proven, f_half]
}

behavior b_proven "Proven" {
  features [f_proven]
  verify unit "works"
}

behavior b_half "Half" {
  features [f_half]
  verify unit "works"
  verify unit "still works"
}
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("specforge-report.json"),
        r#"{"runner":"fixture","results":{"b_proven":{"tests":[{"name":"t1","status":"pass","verify":"works"}]},"b_half":{"tests":[{"name":"t2","status":"pass","verify":"works"}]}}}"#,
    )
    .unwrap();
    dir
}

/// `specforge mcp .` run in `dir` (the project served by a relative path),
/// fed an `initialize` and `requests`, and every line it wrote, parsed.
fn mcp_served_at_dot(dir: &TempDir, requests: &[String]) -> Vec<Value> {
    use std::io::Write;
    let mut all = vec![mcp_request(
        0,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "e2e", "version": "0"}
        }),
    )];
    all.extend_from_slice(requests);
    let mut child = std::process::Command::new(assert_cmd::cargo_bin!("specforge"))
        .args(["mcp", "."])
        .current_dir(dir.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to start specforge mcp");
    let stdin = child.stdin.as_mut().unwrap();
    for request in &all {
        writeln!(stdin, "{request}").unwrap();
    }
    stdin.flush().unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn tool_call(id: u64, name: &str, arguments: Value) -> String {
    mcp_request(
        id,
        "tools/call",
        serde_json::json!({"name": name, "arguments": arguments}),
    )
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "the CLI and MCP send a command's export the same project root, absolute and canonical"
)]
fn a_command_is_told_the_project_root_on_each_surface() {
    let dir = probe_project();
    let output = cargo_bin_cmd!("specforge")
        .current_dir(dir.path())
        .args(["probe", "probe", "--path", ".", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let cli: Value = serde_json::from_slice(&output.stdout).unwrap();
    let canonical = std::fs::canonicalize(dir.path()).unwrap();
    assert_eq!(cli["cwd"], canonical.display().to_string());

    let responses = mcp_served_at_dot(
        &dir,
        &[tool_call(1, "specforge.probe.probe", serde_json::json!({}))],
    );
    let response = find_response(&responses, 1).unwrap();
    let mcp: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(mcp["cwd"], cli["cwd"], "{response}");
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "a trapped command is E028 on both surfaces: one error object on the CLI's stderr under json, a structured MCP error over MCP"
)]
fn a_trapped_command_fails_in_each_surfaces_shape() {
    let dir = probe_project();
    let prefix = "command cmd__trap() of '@test/probe' trapped: ";
    let output = cargo_bin_cmd!("specforge")
        .args(["probe", "trap", "--format", "json", "--path"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.trim_end().lines().count(), 1, "{stderr}");
    let cli: Value = serde_json::from_str(stderr.trim_end()).unwrap();
    assert_eq!(cli["code"], "E028", "{cli}");
    assert!(
        cli["message"].as_str().unwrap().starts_with(prefix),
        "{cli}"
    );
    assert!(cli["suggestion"].is_string(), "{cli}");

    let responses = structured_session(
        &dir,
        &[tool_call(1, "specforge.probe.trap", serde_json::json!({}))],
    );
    let response = find_response(&responses, 1).unwrap();
    assert_eq!(response["result"]["isError"], true, "{response}");
    let mcp: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(mcp["code"], "internal_error", "{mcp}");
    assert_eq!(mcp["diagnostic"]["code"], "E028", "{mcp}");
    assert!(
        mcp["message"].as_str().unwrap().starts_with(prefix),
        "{mcp}"
    );
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "the CLI and MCP send a command the same evidence for the same recorded report"
)]
fn the_cli_and_mcp_send_a_command_the_same_evidence() {
    let dir = evidence_project();
    let output = cargo_bin_cmd!("specforge")
        .args([
            "product",
            "milestone-completion",
            "m1",
            "--format",
            "json",
            "--path",
        ])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let cli: Value = serde_json::from_slice(&output.stdout).unwrap();
    let responses = structured_session(
        &dir,
        &[tool_call(
            1,
            "specforge.product.milestone_completion",
            serde_json::json!({"milestone": "m1"}),
        )],
    );
    let response = find_response(&responses, 1).unwrap();
    assert_eq!(response["result"]["isError"], false, "{response}");
    assert_eq!(response["result"]["structuredContent"], cli);
    assert_eq!(cli["evidence"]["state"], "recorded", "{cli}");
    assert_eq!(cli["proven_count"], 1, "{cli}");
    assert_eq!(
        cli["proven_features"],
        serde_json::json!(["f_proven"]),
        "{cli}"
    );
}
