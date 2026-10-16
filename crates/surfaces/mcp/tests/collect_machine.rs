//! `specforge.collect` runs commands on the server's machine (ADR 0125): a scripted one here, so no test
//! spawns a process or reads the developer's consent store.

use crate::support::*;
use serde_json::json;
use specforge_mcp::McpServer;
use specforge_ops::analysis::collect::{CollectorMachine, RunnerOutput};
use specforge_ops::testing::{ScriptedMachine, ScriptedRun};
use specforge_test::prelude::*;
use std::sync::Arc;

/// A Rust project collected by `@specforge/cargo-test`, serving as the server's own project.
fn project() -> TestProject {
    TestProject::new()
        .enabling(&[
            "@specforge/software",
            "@specforge/testing",
            "@specforge/cargo-test",
        ])
        .file(
            "app.spec",
            "behavior alpha \"Alpha\" {\n  verify unit \"works\"\n}\n",
        )
        .file("Cargo.toml", "")
}

#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "specforge.collect runs a command the user approved, its output discarded"
)]
fn collect_runs_a_command_the_user_approved() {
    let machine = Arc::new(ScriptedMachine::new().approving_every_command().answering(
        ScriptedRun {
            files: vec![(
                    "t.json".into(),
                    json!({"entries": [
                        {"entity_id": "alpha", "test_name": "works", "status": "passed"}
                    ]})
                    .to_string(),
                )],
            exit_code: Some(0),
            ..ScriptedRun::default()
        },
    ));
    let server =
        McpServer::new().with_collector_machine(Arc::clone(&machine) as Arc<dyn CollectorMachine>);
    let mut served = Served::over(project(), server);

    let resp = call_tool(&mut served, "specforge.collect", json!({"run": true}));

    let result = tool_json(&resp);
    let runner = &result["runners"][0];
    assert_eq!(runner["ran"], true, "{result}");
    assert_eq!(runner["exit_code"], 0, "{result}");
    assert_eq!(machine.runs().len(), 1);
    assert_eq!(machine.runs()[0].output, RunnerOutput::Discard);
}

#[test]
fn collect_asks_nobody_and_runs_nothing_unapproved() {
    // The served harness's default machine: nothing approved, whatever is in the developer's home.
    let mut served = Served::initialized(project());

    let resp = call_tool(&mut served, "specforge.collect", json!({"run": true}));

    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["diagnostic"]["code"], "E059", "{error}");
}
