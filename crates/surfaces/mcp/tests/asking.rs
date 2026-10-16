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
    assert!(!message.contains("--no-run"), "{message}");
    let suggestion = diagnostic["suggestion"].as_str().unwrap_or_default();
    assert!(
        suggestion.contains("`specforge collect` in a terminal"),
        "{error}"
    );
}
