//! Who can be asked, over MCP (ADR 0110): nobody, so the operation's own refusal is what the agent reads.

use crate::operations_mgmt::{collect_project, test_server};
use crate::support::*;
use serde_json::json;
use specforge_test::prelude::*;

#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "specforge.collect refuses an unapproved command with the operation's own message"
)]
fn collect_refuses_an_unapproved_command_with_the_operations_own_message() {
    let mut server = test_server();
    let project = within(server.root(), collect_project());
    let resp = call_tool(
        &mut server,
        "specforge.collect",
        json!({"path": project.path().to_str().unwrap(), "run": true}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    let diagnostic = &error["diagnostic"];
    assert_eq!(diagnostic["code"], "E059", "{error}");
    let message = diagnostic["message"].as_str().unwrap();
    assert!(message.starts_with("running `"), "{message}");
    assert!(!message.contains("isn't approved"), "{message}");
    let suggestion = diagnostic["suggestion"].as_str().unwrap_or_default();
    assert!(
        suggestion.contains("`specforge collect` in a terminal"),
        "{error}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "a missing report's refusal tells the agent to run the tests, naming no CLI flag"
)]
fn collect_without_a_report_tells_the_agent_to_run_the_tests() {
    let mut served = TestProject::new()
        .enabling(&["@specforge/software", "@specforge/cargo-test"])
        .file("app.spec", "behavior login \"Log in\" {\n}\n")
        .serve_components();
    let reply = call_tool(&mut served, "specforge.collect", json!({}));
    let error = crate::tool_errors::mcp_error(&reply);
    assert_eq!(error["diagnostic"]["code"], "E045", "{error}");
    let message = error["diagnostic"]["message"].as_str().unwrap();
    assert!(
        message.contains("(`specforge collect` runs them)"),
        "{message}"
    );
    assert!(!message.contains("--"), "{message}");
}
