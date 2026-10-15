//! Coverage characterization corpus (architecture plan 02, step S1).
//!
//! Pins what `analyze coverage`, `stats` and the MCP coverage views report
//! today on one small project (`fixtures/coverage/fx1`) and on the shipped
//! `examples/todo-app`. The surfaces disagree; each disagreement is marked
//! with the step that removes it, and that step turns the assertion red on
//! purpose and rewrites it.
//!
//! fx1 holds:
//! - `login`: one obligation, proven by a passing test that names it;
//! - `logout`: no obligation, one passing test;
//! - `reset_password`: one obligation, named by its test only up to case
//!   (the slug matches, the text does not);
//! - `Status`: a union type;
//! - `Payload`: a struct field named `verify` *and* a verify statement the
//!   passing test names;
//! - `signin`: a feature (not testable) with one passing, unnamed test;
//! - `no_lost_login`: a formal property with one obligation and no test.
//!
//! The `*_today` tests prove no spec obligation (they pin current behavior,
//! bugs included), so they carry no `specforge_test` link.

use crate::support::pairing::{mcp_calls, served};
use crate::support::{Cli, TestProject};
use serde_json::{Value, json};
use specforge_mcp::testing::Served;
use specforge_test_macros::test as specforge_test;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/coverage")
        .join(name)
}

/// A scratch copy of a corpus project, so no surface writes into the repo.
pub(crate) fn project(name: &str) -> TestProject {
    TestProject::copy_of(fixture_dir(name))
}

/// A scratch copy of the shipped `examples/todo-app`.
fn todo_app() -> TestProject {
    TestProject::copy_of(specforge_repo_root::repo_root().join("examples/todo-app"))
}

/// The `@specforge/testing:coverage` pass of `analyze coverage --json`.
pub(crate) fn analyze_coverage(project: &TestProject) -> Value {
    let out = Cli::on(project)
        .command(&["analyze", "coverage", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let doc: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("analyze --json is not JSON ({e}): {stdout}"));
    doc["passes"]
        .as_array()
        .and_then(|passes| {
            passes
                .iter()
                .find(|p| p["pass"] == "@specforge/testing:coverage")
        })
        .cloned()
        .unwrap_or_else(|| panic!("no coverage pass: {doc}"))
}

/// Each finding of a pass as `(code, message)`, sorted.
fn findings(pass: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = pass["findings"]
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
    out.sort();
    out
}

pub(crate) fn stats(project: &TestProject) -> Value {
    let out = Cli::on(project)
        .command(&["stats", "--format", "json"])
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stats is not JSON ({e}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

/// `specforge.coverage` rows as `id -> (status, obligations, proven)`.
pub(crate) fn coverage_rows(content: &Value) -> BTreeMap<String, (String, u64, u64)> {
    content["entities"]
        .as_array()
        .unwrap_or_else(|| panic!("coverage is not an array: {content}"))
        .iter()
        .map(|row| {
            (
                row["entity_id"].as_str().unwrap().to_string(),
                (
                    row["status"].as_str().unwrap().to_string(),
                    row["obligations"].as_u64().unwrap(),
                    row["proven"].as_u64().unwrap(),
                ),
            )
        })
        .collect()
}

#[test]
fn fx1_analyze_coverage_today() {
    let tmp = project("fx1");
    let pass = analyze_coverage(&tmp);
    let summary = &pass["summary"];

    // Testable: login, logout, reset_password and Payload, as in stats.
    // The formal property accepts verify statements but its kind is not
    // testable (S4); the union Status owes no obligations (D2-b, S10).
    assert_eq!(summary["testable_total"], 4, "{summary}");
    assert_eq!(summary["testable_exempt"], 1, "{summary}");
    assert_eq!(summary["testable_verified"], 3, "{summary}");
    assert_eq!(summary["obligations"], 4, "{summary}");
    assert_eq!(
        summary["test_results"]["obligations_proven"], 2,
        "{summary}"
    );
    // login and Payload. logout and the feature signin have passing tests
    // but no obligation, so nothing proves them (D2-a, S10).
    assert_eq!(
        summary["discharge_funnel"]["entities_proven"], 2,
        "{summary}"
    );
    assert_eq!(summary["testable_proven"], 2, "{summary}");

    let expected: Vec<(String, String)> = [
        ("A001", "behavior 'logout' declares no verify obligations"),
        (
            "A015",
            "behavior 'reset_password' has 1 obligation(s) no passing test proves: \"Reset link expires after one hour\"",
        ),
        (
            "A015",
            "property 'no_lost_login' has 1 obligation(s) no passing test proves: \"no login is lost\"",
        ),
        (
            "A016",
            "tests name obligation(s) behavior 'reset_password' does not declare: \"reset link expires after one hour\"",
        ),
    ]
    .iter()
    .map(|(c, m)| (c.to_string(), m.to_string()))
    .collect();
    assert_eq!(findings(&pass), expected);
}

#[test]
fn fx1_stats_today() {
    let tmp = project("fx1");
    let stats = stats(&tmp);
    // Testable kinds: behavior and type (5 entities, less the union
    // Status, D2-b); the property is not.
    assert_eq!(stats["testable_count"], 4, "{stats}");
    // login, reset_password, Payload (its statement sits behind a struct
    // field named `verify`, S2) and the property, which counts toward
    // "verified" though its kind is not testable.
    assert_eq!(stats["verified_count"], 4, "{stats}");
    assert_eq!(stats["declared_pct"], 75.0, "{stats}");
}

#[test]
fn fx1_mcp_coverage_today() {
    let mut tmp = served(project("fx1"));
    let results = mcp_calls(
        &mut tmp,
        &[
            json!({"name": "specforge.coverage", "arguments": {}}),
            json!({"name": "specforge.inspect", "arguments": {"entity_id": "Payload"}}),
            json!({"name": "specforge.stats", "arguments": {}}),
        ],
    );

    let rows = coverage_rows(&results[0]);
    let row =
        |status: &str, obligations: u64, proven: u64| (status.to_string(), obligations, proven);
    // The union Status owes nothing: it does not count toward coverage, so
    // the unfiltered view leaves it out, as stats does (plan 02 D3).
    let expected: BTreeMap<String, (String, u64, u64)> = [
        // Payload's statement sits behind its `verify` struct field (S2).
        ("Payload", row("covered", 1, 1)),
        ("login", row("covered", 1, 1)),
        ("logout", row("uncovered", 0, 0)),
        ("reset_password", row("uncovered", 1, 0)),
    ]
    .into_iter()
    .map(|(id, r)| (id.to_string(), r))
    .collect();
    assert_eq!(rows, expected, "{}", results[0]);

    let inspect = &results[1];
    // `testable` is the kind's; `declared` the entity's own (D2-d, S10).
    assert_eq!(inspect["testable"], true, "{inspect}");
    assert_eq!(inspect["declared"], true, "{inspect}");
    assert_eq!(
        inspect["verify_declarations"],
        json!(["unit Payload schema is valid"]),
        "{inspect}"
    );
    assert_eq!(inspect["coverage_status"], "covered", "{inspect}");

    assert_eq!(results[2]["declared_pct"], 75.0, "{}", results[2]);
}

#[test]
fn todo_app_analyze_and_stats_today() {
    let example = todo_app();
    let pass = analyze_coverage(&example);
    // The same 16 testable entities stats counts; the formal property
    // no_lost_completion is not testable (S4), and the failure mode
    // lost_task and the union TaskStatus owe no obligations (D2-b, S10).
    assert_eq!(pass["summary"]["testable_total"], 16, "{}", pass["summary"]);
    assert_eq!(pass["summary"]["testable_exempt"], 2, "{}", pass["summary"]);
    let a001: Vec<String> = findings(&pass)
        .into_iter()
        .filter(|(code, _)| code == "A001")
        .map(|(_, message)| message)
        .collect();
    assert!(a001.is_empty(), "{a001:?}");

    let stats = stats(&example);
    assert_eq!(stats["testable_count"], 16, "{stats}");
    assert_eq!(stats["verified_count"], 16, "{stats}");
}

/// The ids of the entities a finding with `code` names (`kind 'id' ...`).
fn named_by(pass: &Value, code: &str) -> std::collections::BTreeSet<String> {
    findings(pass)
        .into_iter()
        .filter(|(c, _)| c == code)
        .filter_map(|(_, message)| message.split('\'').nth(1).map(str::to_string))
        .collect()
}

/// MCP `specforge.coverage` and `analyze coverage` read one rule: an
/// entity is covered exactly when analyze proves it: it declares
/// obligations, and against the recorded tests analyze reports neither an
/// unproven obligation (A015) nor a failing test (A014) for it.
fn assert_mcp_coverage_matches_analyze(served: &mut Served) {
    let pass = analyze_coverage(served.project());
    let report: Value = std::fs::read_to_string(served.root().join("specforge-report.json"))
        .map(|raw| serde_json::from_str(&raw).unwrap())
        .unwrap_or(Value::Null);
    assert!(report.is_object(), "the corpus records tests");
    let (a014, a015) = (named_by(&pass, "A014"), named_by(&pass, "A015"));
    let rows = coverage_rows(
        &mcp_calls(
            served,
            &[json!({"name": "specforge.coverage", "arguments": {}})],
        )[0],
    );
    // MCP lists the entities that count toward coverage, as analyze counts
    // them.
    let summary = &pass["summary"];
    assert_eq!(
        rows.len() as u64,
        summary["testable_total"].as_u64().unwrap(),
        "MCP lists the testable entities analyze counts"
    );
    let covered = rows.values().filter(|(status, _, _)| status == "covered");
    assert_eq!(
        covered.count() as u64,
        summary["testable_proven"].as_u64().unwrap(),
        "MCP covers exactly as many as analyze proves"
    );
    for (id, (status, obligations, _)) in &rows {
        let proven = *obligations > 0 && !a014.contains(id) && !a015.contains(id);
        assert_eq!(
            status == "covered",
            proven,
            "{id}: MCP says {status}, analyze proven = {proven}"
        );
    }
}

#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "specforge.coverage reports covered exactly for the entities analyze coverage proves"
)]
fn mcp_coverage_matches_analyze_coverage() {
    assert_mcp_coverage_matches_analyze(&mut served(project("fx1")));
    assert_mcp_coverage_matches_analyze(&mut served(todo_app()));
}

/// `specforge.coverage {}` lists exactly the entities `specforge stats`
/// counts as testable, and covers exactly as many as analyze proves.
fn assert_mcp_coverage_rows_are_what_stats_counts(served: &mut Served) {
    let results = mcp_calls(
        served,
        &[
            json!({"name": "specforge.coverage", "arguments": {}}),
            json!({"name": "specforge.coverage", "arguments": {"status_filter": "covered"}}),
        ],
    );
    let stats = stats(served.project());
    assert_eq!(
        results[0]["entities"].as_array().unwrap().len() as u64,
        stats["testable_count"].as_u64().unwrap(),
        "{}",
        results[0]
    );
    assert!(
        results[0]["entities"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["exempt"] == false)
    );
    let proven = analyze_coverage(served.project())["summary"]["testable_proven"].clone();
    assert_eq!(
        json!(results[1]["entities"].as_array().unwrap().len()),
        proven,
        "{}",
        results[1]
    );
}

#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "with no filters the coverage rows are the entities stats counts as testable"
)]
fn mcp_coverage_rows_are_what_stats_counts() {
    assert_mcp_coverage_rows_are_what_stats_counts(&mut served(project("fx1")));
    assert_mcp_coverage_rows_are_what_stats_counts(&mut served(todo_app()));
}

#[specforge_test(
    behavior = "compute_project_statistics",
    verify = "stats reports the declared and proof percentages"
)]
fn stats_reports_declared_and_proof_percentages() {
    let tmp = project("fx1");
    let stats = stats(&tmp);
    // 3 of the 4 testable entities declare obligations; login and Payload
    // are proven.
    assert_eq!(stats["declared_count"], 3, "{stats}");
    assert_eq!(stats["declared_pct"], 75.0, "{stats}");
    assert!(stats.get("coverage_pct").is_none(), "{stats}");
    assert_eq!(stats["proof_pct"], 50.0, "{stats}");

    let human = Cli::on(&tmp).run(&["stats"]).stdout;
    assert!(human.contains("Declared: 75% of 4 testable"), "{human}");
    assert!(human.contains("Proven:   50% of 4 testable"), "{human}");

    // No recorded tests: no proof percentage.
    tmp.remove("specforge-report.json");
    assert_eq!(self::stats(&tmp)["proof_pct"], Value::Null);
}

#[specforge_test(
    behavior = "provide_mcp_stats_tool",
    verify = "response includes the declared and proof percentages"
)]
fn mcp_stats_reports_declared_and_proof_percentages() {
    let stats = &mcp_calls(
        &mut served(project("fx1")),
        &[json!({"name": "specforge.stats", "arguments": {}})],
    )[0];
    assert_eq!(stats["declared_pct"], 75.0, "{stats}");
    assert_eq!(stats["declared_pct"], 75.0, "{stats}");
    assert_eq!(stats["proof_pct"], 50.0, "{stats}");

    // A report that can't be read is an error result, not "no proof".
    let bad = fx1_with_a_malformed_report();
    let result = &mcp_calls(
        &mut served(bad),
        &[json!({"name": "specforge.stats", "arguments": {}})],
    )[0];
    assert_eq!(result["isError"]["code"], "schema_mismatch", "{result}");
}

/// fx1 with a `specforge-report.json` cut off mid-write.
fn fx1_with_a_malformed_report() -> TestProject {
    project("fx1").file(
        "specforge-report.json",
        r#"{"runner": "fixture", "results": {"login": {"tests": ["#,
    )
}

#[test]
fn fx1_malformed_report_is_an_error_on_every_surface() {
    let tmp = fx1_with_a_malformed_report();

    // CLI: exit 2, naming the file.
    let out = Cli::on(&tmp).run(&["analyze", "coverage"]);
    assert_eq!(out.code, 2);
    let stderr = out.stderr;
    assert!(
        stderr.contains("invalid test results") && stderr.contains("specforge-report.json"),
        "{stderr}"
    );

    // MCP tools: an isError result carrying the McpError (S5, D2-e).
    let results = mcp_calls(
        &mut served(tmp),
        &[
            json!({"name": "specforge.coverage", "arguments": {}}),
            json!({"name": "specforge.inspect", "arguments": {"entity_id": "login"}}),
            json!({"name": "specforge.query",
                   "arguments": {"entity_id": "login", "include_coverage": true}}),
            json!({"name": "specforge.analyze", "arguments": {"pass": "coverage"}}),
            json!({"method": "prompts/get",
                   "params": {"name": "specforge://prompts/review", "arguments": {}}}),
        ],
    );
    for (tool, result) in [
        "specforge.coverage",
        "specforge.inspect",
        "specforge.query",
        "specforge.analyze",
    ]
    .iter()
    .zip(&results)
    {
        let error = &result["isError"];
        assert_eq!(error["code"], "schema_mismatch", "{tool}: {result}");
        assert_eq!(error["tool"], *tool, "{result}");
        assert_eq!(error["diagnostic"]["code"], "E045", "{tool}: {result}");
    }
    // A prompt has no error result: a JSON-RPC error with the McpError.
    let review = &results[4]["error"];
    assert_eq!(review["code"], -32603, "{}", results[4]);
    assert_eq!(review["data"]["code"], "schema_mismatch", "{}", results[4]);
}
