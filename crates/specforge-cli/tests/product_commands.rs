//! The `specforge product <command>` commands: `@specforge/product`'s
//! `cmd__product_*` exports, which the CLI routes to from the commands the
//! extension declares (ADR 0008). Output, flags and names are the ones the
//! built-in `product` subcommands had.

use crate::e2e_fixtures::{
    find_response, mcp_raw_session_in, mcp_request, mcp_session_in, parse_tool_content,
};
use assert_cmd::cargo_bin_cmd;
use specforge_test_macros::test as specforge_test;
use std::fs;
use tempfile::TempDir;

pub fn setup_product_project() -> TempDir {
    let dir = TempDir::new().unwrap();

    let config = serde_json::json!({
        "name": "test-project",
        "version": "0.1.0",
        "extensions": ["@specforge/product"]
    });
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();

    fs::write(
        dir.path().join("spec.spec"),
        r#"persona dev "Developer" {
    description "A software developer"
    status active
}

channel cli "CLI" {
    description "Command-line interface"
    status active
}

feature f1 "Core Feature" {
    status proposed
    priority high
}

feature f2 "Secondary Feature" {
    status done
    priority medium
    depends_on [f1]
}

journey j1 "Dev Flow" {
    persona dev
    channels [cli]
    features [f1, f2]
    description "Developer uses CLI"
}

module mod1 "Core Module" {
    features [f1]
}

milestone m1 "Launch" {
    status planned
    features [f1, f2]
    modules [mod1]
}

deliverable d1 "CLI App" {
    artifact_type cli
    status draft
    journeys [j1]
    modules [mod1]
    milestones [m1]
}

term specification "Specification" {
    definition "A formal description of system behavior"
}

release r1 "Initial Release" {
    version "1.0.0"
    status planned
    deliverables [d1]
    milestones [m1]
}
"#,
    )
    .unwrap();

    dir
}

#[test]
fn test_product_features_json() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "features",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 2);
}

#[test]
fn test_product_features_filter_status() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "features",
        "--path",
        dir.path().to_str().unwrap(),
        "--status",
        "proposed",
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
    assert_eq!(result["features"][0]["id"], "f1");
}

#[test]
fn test_product_milestones_json() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "milestones",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_milestone_completion() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "milestone-completion",
        "m1",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["milestone_id"], "m1");
    assert_eq!(result["total_features"], 2);
    assert_eq!(result["done_count"], 1); // f2 is done
    assert_eq!(result["done_features"], serde_json::json!(["f2"]));
    assert_eq!(result["completion_ratio"], 0.5);
}

#[test]
fn deliverable_completion_takes_details_as_a_flag() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "deliverable-completion",
        "d1",
        "--details",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // m1 is planned, so none of d1's one milestone is completed.
    assert_eq!(
        (
            result["milestone_count"].clone(),
            result["completed_count"].clone()
        ),
        (serde_json::json!(1), serde_json::json!(0))
    );
    assert_eq!(result["milestone_details"][0]["milestone_id"], "m1");
    assert_eq!(result["milestone_details"][0]["done_count"], 1);
}

#[test]
fn test_product_feature_impact() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "feature-impact",
        "f1",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["feature_id"], "f1");
    assert!(!result["affected_journeys"].as_array().unwrap().is_empty());
    assert!(!result["affected_milestones"].as_array().unwrap().is_empty());
    assert!(!result["affected_modules"].as_array().unwrap().is_empty());
}

#[specforge_test(
    behavior = "pe_query_feature_impact",
    verify = "a feature that only relates to the feature is not a dependent"
)]
fn a_related_feature_is_not_a_dependent_in_the_impact() {
    let dir = setup_product_project();
    // f3 relates to f1 (`features`); f2 depends on it (`depends_on`).
    fs::write(
        dir.path().join("related.spec"),
        "feature f3 \"Related Feature\" {\n    status proposed\n    features [f1]\n}\n",
    )
    .unwrap();
    let output = cargo_bin_cmd!("specforge")
        .args([
            "product",
            "feature-impact",
            "f1",
            "--format",
            "json",
            "--path",
        ])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result["dependent_features"],
        serde_json::json!(["f2"]),
        "{result}"
    );
}

#[test]
fn test_product_feature_dependents() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "feature-dependents",
        "f1",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // f2 depends on f1.
    assert_eq!(
        result,
        serde_json::json!({"feature_id": "f1", "dependents": ["f2"], "count": 1})
    );
}

#[test]
fn test_product_health() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "health",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(result["score"]["overall"].as_f64().unwrap() > 0.0);
    assert!(!result["entity_counts"].as_array().unwrap().is_empty());
}

#[test]
fn test_product_modules_json() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "modules",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_terms_json() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "terms",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_releases_json() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "releases",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_personas_json() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "personas",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_channels_json() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "channels",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["total"], 1);
}

#[test]
fn test_product_journey_coverage() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "journey-coverage",
        "j1",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["journey_id"], "j1");
    assert_eq!(result["total_features"], 2);
    assert_eq!(result["covered_count"], 1); // f2 is done
    assert_eq!(result["uncovered_features"], serde_json::json!(["f1"]));
}

#[test]
fn test_product_persona_features() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "persona-features",
        "dev",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // f1 and f2 via journey j1
    assert_eq!(result["features"], serde_json::json!(["f1", "f2"]));
    assert_eq!(result["via_journey_ids"], serde_json::json!(["j1"]));
    assert_eq!(result["count"], 2);
}

#[test]
fn test_product_channel_features() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "channel-features",
        "cli",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // f1 and f2 via journey j1
    assert_eq!(result["channel_id"], "cli");
    assert_eq!(result["features"], serde_json::json!(["f1", "f2"]));
    assert_eq!(result["count"], 2);
}

#[test]
fn test_product_bulk_status() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "bulk-status",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let kinds: Vec<&str> = result["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "feature",
            "milestone",
            "deliverable",
            "persona",
            "channel",
            "release"
        ]
    );
}

#[test]
fn test_product_nonexistent_milestone_exits_one() {
    let dir = setup_product_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args([
        "product",
        "milestone-completion",
        "nonexistent",
        "--path",
        dir.path().to_str().unwrap(),
    ]);
    cmd.assert().failure();
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "exit code, stdout, stderr returned to CLI"
)]
fn an_extension_command_prints_what_its_export_returns() {
    let dir = setup_product_project();
    let path = dir.path().to_str().unwrap();
    let output = cargo_bin_cmd!("specforge")
        .args(["product", "milestone-completion", "m1", "--path", path])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Milestone: m1 (planned)\nCompletion: 50% (1/2 features done)\n\
         Evidence:   none recorded (run `specforge collect`)\n  f1 [proposed]\n  f2 [done]\n"
    );
    assert!(output.stderr.is_empty(), "{output:?}");

    let output = cargo_bin_cmd!("specforge")
        .args(["product", "milestone-completion", "nope", "--path", path])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "error: milestone 'nope' not found\n"
    );
}

#[test]
fn an_extension_command_has_the_declared_command_line() {
    let dir = setup_product_project();
    let path = dir.path().to_str().unwrap();
    // `ext:command` names the same command as `ext command`.
    let output = cargo_bin_cmd!("specforge")
        .args(["product:features", "--path", path, "--limit", "1"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "id  title         status    priority\nf1  Core Feature  proposed  high\n1 of 2 features; --offset 1 for more\n"
    );
    // The declared enum refuses other values; an undeclared flag is refused.
    for args in [
        ["product", "features", "--format", "xml"],
        ["product", "journeys", "--status", "done"],
    ] {
        let output = cargo_bin_cmd!("specforge")
            .args(args)
            .args(["--path", path])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
    }
    // A name no built-in command or extension has is refused.
    let output = cargo_bin_cmd!("specforge")
        .args(["nonesuch", "features", "--path", path])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand 'nonesuch'"),
        "{output:?}"
    );
}

#[specforge_test(
    behavior = "surface_list_features",
    verify = "status filter reduces result set"
)]
fn the_features_command_is_an_mcp_tool_with_its_filters() {
    let dir = setup_product_project();
    let call = |id: u64, args: serde_json::Value| {
        mcp_request(
            id,
            "tools/call",
            serde_json::json!({"name": "specforge.product.features", "arguments": args}),
        )
    };
    let responses = mcp_session_in(
        &dir,
        &[
            call(1, serde_json::json!({})),
            call(2, serde_json::json!({"status": "done"})),
        ],
    );
    let all = parse_tool_content(find_response(&responses, 1).unwrap());
    assert_eq!(all["total"], 2, "{all}");
    let done = parse_tool_content(find_response(&responses, 2).unwrap());
    assert_eq!(done["total"], 1, "{done}");
    assert_eq!(done["features"][0]["id"], "f2");
}

#[specforge_test(
    behavior = "surface_list_features",
    verify = "pagination offset and limit are respected"
)]
fn the_features_command_pages_after_counting() {
    let dir = setup_product_project();
    let output = cargo_bin_cmd!("specforge")
        .args(["product", "features", "--offset", "1", "--limit", "1"])
        .args(["--format", "json", "--path", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["total"], 2);
    let ids: Vec<&str> = result["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["f2"]);
}

#[test]
fn completions_include_the_commands_of_the_project_here() {
    let dir = setup_product_project();
    let output = cargo_bin_cmd!("specforge")
        .current_dir(dir.path())
        .args(["completions", "bash"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let script = String::from_utf8_lossy(&output.stdout);
    assert!(
        script.contains("specforge__subcmd__product__subcmd__milestone__subcmd__completion"),
        "the product commands are completed"
    );

    // Outside a project, the built-ins only.
    let empty = TempDir::new().unwrap();
    let output = cargo_bin_cmd!("specforge")
        .current_dir(empty.path())
        .args(["completions", "bash"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let script = String::from_utf8_lossy(&output.stdout);
    assert!(script.contains("specforge__subcmd__check"));
    assert!(!script.contains("specforge__subcmd__product"));

    // `--path` names the project from anywhere.
    let output = cargo_bin_cmd!("specforge")
        .current_dir(empty.path())
        .args(["completions", "bash", "--path"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let script = String::from_utf8_lossy(&output.stdout);
    assert!(
        script.contains("specforge__subcmd__product__subcmd__milestone__subcmd__completion"),
        "the product commands of the --path project are completed"
    );
}

#[specforge_test_macros::test(
    behavior = "locate_project_root",
    verify = "a command given a path that does not exist refuses it as file_not_found"
)]
fn completions_refuse_a_path_that_does_not_exist() {
    let empty = TempDir::new().unwrap();
    let output = cargo_bin_cmd!("specforge")
        .current_dir(empty.path())
        .args(["completions", "bash", "--path", "nope"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("error[file_not_found]: path not found: nope"),
        "{output:?}"
    );
}

/// `requests` after an `initialize` negotiating MCP 2025-06-18, the first
/// revision with structured content.
pub fn structured_session(dir: &TempDir, requests: &[String]) -> Vec<serde_json::Value> {
    let initialize = mcp_request(
        0,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "e2e", "version": "0"}
        }),
    );
    let mut all = vec![initialize];
    all.extend_from_slice(requests);
    mcp_raw_session_in(dir, &all)
}

fn tool_call(id: u64, name: &str, args: serde_json::Value) -> String {
    mcp_request(
        id,
        "tools/call",
        serde_json::json!({"name": name, "arguments": args}),
    )
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "over MCP a command is asked for json and its JSON output is the tool's structured content"
)]
fn over_mcp_a_command_answers_json_as_structured_content() {
    let dir = setup_product_project();
    let responses = structured_session(
        &dir,
        &[
            mcp_request(1, "tools/list", serde_json::json!({})),
            tool_call(
                2,
                "specforge.product.milestone_completion",
                serde_json::json!({"milestone": "m1"}),
            ),
        ],
    );
    // The tool has no format argument: the host always asks for json.
    let tools = &find_response(&responses, 1).unwrap()["result"]["tools"];
    let tool = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "specforge.product.milestone_completion")
        .unwrap();
    assert!(
        tool["inputSchema"]["properties"].get("format").is_none(),
        "{tool}"
    );

    let response = find_response(&responses, 2).unwrap();
    let result = &response["result"];
    assert_eq!(result["isError"], false, "{response}");
    let structured = &result["structuredContent"];
    assert_eq!(structured["milestone_id"], "m1", "{response}");
    assert_eq!(parse_tool_content(response), *structured);
}

#[specforge_test(
    behavior = "surface_format_conventions",
    verify = "an MCP tool call returns the json payload"
)]
fn an_mcp_tool_call_returns_the_json_payload() {
    let dir = setup_product_project();
    let output = cargo_bin_cmd!("specforge")
        .args(["product", "health", "--format", "json", "--path"])
        .arg(dir.path())
        .output()
        .unwrap();
    let cli: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Asked for nothing, the tool answers what --format json prints.
    let responses = mcp_session_in(
        &dir,
        &[tool_call(
            1,
            "specforge.product.health",
            serde_json::json!({}),
        )],
    );
    assert_eq!(
        parse_tool_content(find_response(&responses, 1).unwrap()),
        cli
    );
}

#[specforge_test(
    behavior = "surface_error_handling",
    verify = "CLI errors go to stderr"
)]
fn a_command_that_cannot_answer_writes_its_error_to_stderr() {
    let dir = setup_product_project();
    let path = dir.path().to_str().unwrap();
    let run = |args: &[&str]| {
        cargo_bin_cmd!("specforge")
            .args(["product"])
            .args(args)
            .args(["--path", path])
            .output()
            .unwrap()
    };
    // A typo of m1: the nearest milestone is suggested.
    let human = run(&["milestone-completion", "m2"]);
    assert_eq!(human.status.code(), Some(1));
    assert!(human.stdout.is_empty(), "{human:?}");
    assert_eq!(
        String::from_utf8_lossy(&human.stderr),
        "error: milestone 'm2' not found\ndid you mean 'm1'?\n"
    );
    let json = run(&["milestone-completion", "m2", "--format", "json"]);
    assert_eq!(json.status.code(), Some(1));
    assert!(json.stdout.is_empty(), "{json:?}");
    let error: serde_json::Value = serde_json::from_slice(&json.stderr).unwrap();
    assert_eq!(
        error,
        serde_json::json!({"code": "ENTITY_NOT_FOUND", "message": "milestone 'm2' not found",
            "entity_id": "m2", "suggestion": "m1"})
    );
    // An input the command refuses exits 2, as clap's usage errors do.
    let invalid = run(&["features", "--limit=-1", "--format", "json"]);
    assert_eq!(invalid.status.code(), Some(2), "{invalid:?}");
    assert!(invalid.stdout.is_empty(), "{invalid:?}");
    let error: serde_json::Value = serde_json::from_slice(&invalid.stderr).unwrap();
    assert_eq!(error["code"], "INVALID_INPUT", "{error}");
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "under --format json a usage error the command line catches is one INVALID_INPUT error object on stderr, exit 2"
)]
fn under_json_a_usage_error_is_an_invalid_input_object() {
    let dir = setup_product_project();
    let path = dir.path().to_str().unwrap();
    let run = |args: &[&str]| {
        cargo_bin_cmd!("specforge")
            .arg("product")
            .args(args)
            .args(["--path", path])
            .output()
            .unwrap()
    };
    let cases: [(&[&str], serde_json::Value); 5] = [
        (
            &["features", "--status", "bogus"],
            serde_json::json!({"code": "INVALID_INPUT",
                "message": "status must be one of proposed, accepted, in_progress, done, deferred, deprecated, got 'bogus'"}),
        ),
        (
            &["milestone-completion"],
            serde_json::json!({"code": "INVALID_INPUT",
                "message": "missing required arg 'milestone'"}),
        ),
        (
            &["features", "--statsu", "done"],
            serde_json::json!({"code": "INVALID_INPUT",
                "message": "unknown argument '--statsu'", "suggestion": "--status"}),
        ),
        (
            &["features", "--limit", "abc"],
            serde_json::json!({"code": "INVALID_INPUT",
                "message": "limit must be a non-negative integer, got 'abc'"}),
        ),
        // A count below its minimum is the command line's to refuse, as it
        // is MCP's: the export never runs.
        (
            &["features", "--limit", "-1"],
            serde_json::json!({"code": "INVALID_INPUT",
                "message": "limit must be a non-negative integer, got -1"}),
        ),
    ];
    for (args, expected) in &cases {
        // `--format json` before or after the bad arg, in either spelling.
        for (before, after) in [
            (vec!["--format", "json"], vec![]),
            (vec![], vec!["--format", "json"]),
            (vec![], vec!["--format=json"]),
        ] {
            let (command, rest) = args.split_first().unwrap();
            let mut argv = vec![*command];
            argv.extend(&before);
            argv.extend(rest);
            argv.extend(&after);
            let output = run(&argv);
            assert_eq!(output.status.code(), Some(2), "{argv:?}: {output:?}");
            assert!(output.stdout.is_empty(), "{argv:?}: {output:?}");
            let error: serde_json::Value = serde_json::from_slice(&output.stderr)
                .unwrap_or_else(|e| panic!("{argv:?}: {e}: {output:?}"));
            assert_eq!(&error, expected, "{argv:?}");
        }
        // Under human the error is clap's usage text.
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{args:?}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with("error: ") && stderr.contains("For more information, try '--help'."),
            "{args:?}: {stderr}"
        );
    }
    // Help is clap's whatever the format, exit 0.
    let help = run(&["features", "--help", "--format", "json"]);
    assert_eq!(help.status.code(), Some(0), "{help:?}");
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("Usage: specforge product features"),
        "{help:?}"
    );
}

#[specforge_test(
    behavior = "surface_error_handling",
    verify = "MCP tool errors are isError results carrying the error object"
)]
fn over_mcp_an_error_is_an_is_error_result_with_the_object() {
    let dir = setup_product_project();
    let responses = structured_session(
        &dir,
        &[tool_call(
            1,
            "specforge.product.journey_coverage",
            serde_json::json!({"journey": "j2"}),
        )],
    );
    let response = find_response(&responses, 1).unwrap();
    let result = &response["result"];
    assert_eq!(result["isError"], true, "{response}");
    let expected = serde_json::json!({"code": "ENTITY_NOT_FOUND",
        "message": "journey 'j2' not found", "entity_id": "j2", "suggestion": "j1"});
    assert_eq!(parse_tool_content(response), expected);
    assert_eq!(result["structuredContent"], expected);
}

/// [`setup_product_project`] with `m1` due in 2000.
fn setup_overdue_project() -> TempDir {
    let dir = setup_product_project();
    let spec = fs::read_to_string(dir.path().join("spec.spec")).unwrap();
    let spec = spec.replace(
        "    status planned\n    features [f1, f2]",
        "    status planned\n    target_date \"2000-01-01\"\n    features [f1, f2]",
    );
    assert!(spec.contains("2000-01-01"));
    fs::write(dir.path().join("spec.spec"), spec).unwrap();
    dir
}

fn timeline(dir: &TempDir, extra: &[&str]) -> std::process::Output {
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.args(["product", "milestone-timeline", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .args(extra);
    cmd.output().unwrap()
}

#[specforge_test(
    behavior = "surface_milestone_timeline",
    verify = "as-of flag overrides current date for overdue calculation"
)]
fn the_timeline_compares_against_today_unless_as_of_says_otherwise() {
    let dir = setup_overdue_project();
    // The host passes today, long after 2000.
    let output = timeline(&dir, &[]);
    assert!(output.status.success(), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["milestones"][0]["milestone_id"], "m1");
    assert_eq!(result["milestones"][0]["is_overdue"], true);
    let output = timeline(&dir, &["--as-of", "1999-12-31"]);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["overdue_count"], 0);
    let output = timeline(&dir, &["--as-of", "31/12/1999"]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "INVALID_INPUT");
}

#[specforge_test(
    behavior = "pe_query_milestone_timeline",
    verify = "specforge check emits no I058 diagnostics (query-time only)"
)]
fn check_reports_no_overdue_milestone() {
    let dir = setup_overdue_project();
    let mut cmd = cargo_bin_cmd!("specforge");
    cmd.current_dir(dir.path()).arg("check");
    let output = cmd.output().unwrap();
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Infos are always reported, so an I058 would be among them.
    assert!(output.status.code().is_some_and(|c| c <= 1), "{all}");
    assert!(!all.contains("I058"), "{all}");
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "the CommandInput carries what the recorded tests prove, per entity that counts toward coverage"
)]
fn milestone_completion_reads_the_projects_recorded_tests() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        serde_json::json!({"name": "p", "version": "0.1.0",
            "extensions": ["@specforge/product", "@specforge/software", "@specforge/testing"]})
        .to_string(),
    )
    .unwrap();
    fs::write(
        dir.path().join("main.spec"),
        r#"feature f1 "One" {
  status done
}

feature f2 "Two" {
  status done
}

milestone m1 "M" {
  status completed
  features [f1, f2]
  exit_criteria ["done"]
}

behavior b1 "B1" {
  features [f1]
  verify unit "b1 works"
}

behavior b2 "B2" {
  features [f2]
  verify unit "b2 works"
  verify unit "b2 still works"
}
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("specforge-report.json"),
        serde_json::json!({"runner": "fixture", "results": {
            "b1": {"tests": [{"name": "t1", "status": "passed", "verify": "b1 works"}]},
            "b2": {"tests": [{"name": "t2", "status": "passed", "verify": "b2 works"}]},
        }})
        .to_string(),
    )
    .unwrap();
    let path = dir.path().to_str().unwrap();
    let output = cargo_bin_cmd!("specforge")
        .args([
            "product",
            "milestone-completion",
            "m1",
            "--format",
            "json",
            "--path",
            path,
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let mc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(mc["done_count"], 2);
    assert_eq!(mc["proven_features"], serde_json::json!(["f1"]), "{mc}");
    assert_eq!(mc["feature_evidence"][1]["proven_obligations"], 1);
}

/// A project enabling product, software and testing, whose `f_proven` is
/// implemented by a proven behavior, `f_half` by one with an unproven
/// obligation and `f_alone` by none; all three are done.
fn evidence_project(report: bool) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        serde_json::json!({"name": "p", "version": "0.1.0",
            "extensions": ["@specforge/product", "@specforge/software", "@specforge/testing"]})
        .to_string(),
    )
    .unwrap();
    fs::write(
        dir.path().join("main.spec"),
        r#"feature f_proven "Proven" {
  status done
}

feature f_half "Half" {
  status done
}

feature f_alone "Alone" {
  status done
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
    if report {
        fs::write(
            dir.path().join("specforge-report.json"),
            serde_json::json!({"runner": "fixture", "results": {
                "b_proven": {"tests": [{"name": "t1", "status": "passed", "verify": "works"}]},
                "b_half": {"tests": [{"name": "t2", "status": "passed", "verify": "works"}]},
            }})
            .to_string(),
        )
        .unwrap();
    }
    dir
}

fn delivery_evidence(dir: &TempDir) -> serde_json::Value {
    let output = cargo_bin_cmd!("specforge")
        .args([
            "analyze",
            "@specforge/product:delivery_evidence",
            "--format",
            "json",
            "--path",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{e}: {output:?}"))
}

#[specforge_test(
    behavior = "detect_done_feature_without_evidence",
    verify = "done feature with an unproven implementing behavior produces I071"
)]
#[specforge_test(
    behavior = "detect_done_feature_without_evidence",
    verify = "done feature whose implementing behaviors are all proven suppresses I071"
)]
fn delivery_evidence_reports_done_features_the_recorded_tests_do_not_prove() {
    let report = delivery_evidence(&evidence_project(true));
    let pass = &report["passes"][0];
    let mut findings: Vec<(String, String)> = pass["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["code"].as_str().unwrap().to_string(),
                f["message"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    findings.sort();
    assert_eq!(
        findings,
        [
            (
                "I071".to_string(),
                "feature 'f_alone' is done but no behavior implements it, so no recorded test can prove it".to_string()
            ),
            (
                "I071".to_string(),
                "feature 'f_half' is done but the recorded tests prove 0 of the 1 behaviors implementing it (1/2 obligations)".to_string()
            ),
        ],
        "{report}"
    );
}

#[specforge_test(
    behavior = "detect_done_feature_without_evidence",
    verify = "without recorded test results delivery_evidence reports nothing"
)]
fn delivery_evidence_without_a_report_reports_nothing() {
    let report = delivery_evidence(&evidence_project(false));
    let pass = &report["passes"][0];
    assert_eq!(pass["findings"], serde_json::json!([]), "{report}");
}
