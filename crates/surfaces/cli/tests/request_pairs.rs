//! One pairing table (ADR 0101 D5): each row spells one request twice, as a `specforge` command line and as
//! an MCP tool call over the same project, and both surfaces answer the same document or both refuse. Every
//! relation an operation reads (a sentinel, arguments that exclude or need each other) has a row.

use serde_json::{Value, json};
use std::path::Path;

use crate::support::pairing::{Outcome, cli_outcome, mcp_calls, mcp_outcome, served};
use crate::support::{Cli, TestProject};

struct Pair {
    label: &'static str,
    project: fn() -> TestProject,
    /// The command line; `{root}` in a token is the served project's root.
    cli: &'static [&'static str],
    /// `specforge`'s exit: 0 answered, 1 refused by the operation or a failed verdict, 2 refused by clap.
    exit: i32,
    tool: &'static str,
    arguments: fn() -> Value,
}

const PAIRS: &[Pair] = &[
    Pair {
        label: "collect_runner_named",
        project: cargo_test_project,
        cli: &[
            "collect",
            "--path",
            "{root}",
            "--runner",
            "cargo-test",
            "--no-run",
            "--format",
            "json",
        ],
        exit: 0,
        tool: "specforge.collect",
        arguments: || json!({"runner": "cargo-test"}),
    },
    Pair {
        label: "collect_runner_auto",
        project: cargo_test_project,
        cli: &[
            "collect", "--path", "{root}", "--runner", "auto", "--no-run", "--format", "json",
        ],
        exit: 1,
        tool: "specforge.collect",
        arguments: || json!({"runner": "auto"}),
    },
    Pair {
        label: "trace_entity_and_plan",
        project: rv1_with_plan,
        cli: &[
            "trace",
            "alpha",
            "--plan",
            "{root}/agent-plan.json",
            "--path",
            "{root}",
            "--format",
            "json",
        ],
        exit: 2,
        tool: "specforge.trace",
        arguments: || json!({"entity_id": "alpha", "plan": plan()}),
    },
    Pair {
        label: "export_with_and_without_schema",
        project: rv1,
        cli: &[
            "export",
            "--path",
            "{root}",
            "--format",
            "graph",
            "--with-schema",
            "--no-schema",
        ],
        exit: 2,
        tool: "specforge.export",
        arguments: || json!({"format": "graph", "with_schema": true, "no_schema": true}),
    },
    Pair {
        label: "export_depth_without_scope",
        project: rv1,
        cli: &[
            "export", "--path", "{root}", "--format", "graph", "--depth", "2",
        ],
        exit: 2,
        tool: "specforge.export",
        arguments: || json!({"format": "graph", "depth": 2}),
    },
    Pair {
        label: "model_depth_without_root",
        project: rv1,
        cli: &["model", "--path", "{root}", "--depth", "2"],
        exit: 2,
        tool: "specforge.model",
        arguments: || json!({"depth": 2}),
    },
];

/// The project of one behavior with a passing recorded test, for the cargo-test collector.
fn cargo_test_project() -> TestProject {
    TestProject::new()
        .raw_config(
            r#"{"name":"demo","version":"0.1.0","extensions":["@specforge/software","@specforge/testing","@specforge/cargo-test"]}"#,
        )
        .file("app.spec", "behavior login \"Login\" {\n  verify unit \"accepts valid credentials\"\n}\n")
        .file("Cargo.toml", "")
        .file(
            "target/specforge/demo.json",
            r#"{"entries":[{"entity_id":"login","test_name":"accepts","verify":"accepts valid credentials","status":"passed"}]}"#,
        )
}

/// A scratch copy of `fixtures/read_views/rv1`.
fn rv1() -> TestProject {
    TestProject::copy_of(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/read_views/rv1"),
    )
}

/// The agent plan of the trace rows: it names `alpha`.
fn plan() -> Value {
    json!({"plan_id": "p", "entries": [{"entity_id": "alpha", "action": "implement"}]})
}

/// `rv1` with [`plan`] written as `agent-plan.json`.
fn rv1_with_plan() -> TestProject {
    rv1().file("agent-plan.json", plan().to_string())
}

/// Run `label`'s row on both surfaces over one served project: assert the CLI's exit and that the
/// outcomes are equal; return both for a row's own assertions.
fn pair(label: &str) -> (Outcome, Outcome) {
    let row = PAIRS
        .iter()
        .find(|row| row.label == label)
        .unwrap_or_else(|| panic!("no row {label}"));
    let mut served = served((row.project)());
    let root = served.root().to_path_buf();
    let spelled = root.to_str().unwrap();
    let args: Vec<String> = row
        .cli
        .iter()
        .map(|t| t.replace("{root}", spelled))
        .collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = Cli::on(served.project()).bare(&args);
    assert_eq!(
        run.code, row.exit,
        "{label}: {args:?}\n{}{}",
        run.stdout, run.stderr
    );
    let call = json!({"name": row.tool, "arguments": (row.arguments)()});
    let answer = mcp_calls(&mut served, &[call]).remove(0);
    let (cli, mcp) = (cli_outcome(&run, &root), mcp_outcome(&answer, &root));
    assert_eq!(
        cli, mcp,
        "{label}: the surfaces disagree\ncli: {args:?}\ntool: {} {answer}",
        row.tool
    );
    (cli, mcp)
}

#[test]
fn every_row_has_a_unique_label() {
    let mut labels: Vec<&str> = PAIRS.iter().map(|row| row.label).collect();
    labels.sort_unstable();
    let rows = labels.len();
    labels.dedup();
    assert_eq!(labels.len(), rows, "a label names two rows");
}

#[test]
fn collect_with_a_named_runner_is_one_request() {
    let (cli, _) = pair("collect_runner_named");
    assert!(matches!(cli, Outcome::Answered(_)), "{cli:?}");
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_collect_tool",
    verify = "specforge.collect reads runner as specforge collect --runner reads it"
)]
fn collect_runner_auto_is_one_request() {
    assert_eq!(pair("collect_runner_auto").0, Outcome::Refused);
}

#[test]
#[ignore = "flipped by T5"]
fn trace_entity_and_plan_is_one_request() {
    assert_eq!(pair("trace_entity_and_plan").0, Outcome::Refused);
}

#[test]
#[ignore = "flipped by T6"]
fn export_both_schema_flags_is_one_request() {
    assert_eq!(pair("export_with_and_without_schema").0, Outcome::Refused);
}

#[test]
fn export_depth_without_scope_is_refused_on_both() {
    assert_eq!(pair("export_depth_without_scope").0, Outcome::Refused);
}

#[test]
fn model_depth_without_root_is_refused_on_both() {
    assert_eq!(pair("model_depth_without_root").0, Outcome::Refused);
}
