//! How a core tool's dispatch refuses a call, for every tool in the table
//! (plan 02): the pin that holds the pipeline's refusals, events and
//! invocation categories byte for byte through the tool-module refactor.
//!
//! The pin is unlinked: it holds what dispatch does today.

use serde_json::{Value, json};
use specforge_mcp::McpServer;

use crate::support::replies::writing;
use crate::support::{call, call_tool};
use crate::tool_errors::mcp_error;

/// A server that serves no project: initialize names none.
fn serving_nothing() -> McpServer {
    let mut server = McpServer::new();
    let reply = call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
    server
}

/// The names of the events recorded since `before`, an invocation by its
/// category and a mutation's completion by its success.
fn events_since(server: &McpServer, before: usize) -> String {
    let names: Vec<String> = server.state().events[before..]
        .iter()
        .map(|event| match event.name.as_str() {
            "mcp_tool_invoked" => format!(
                "mcp_tool_invoked:{}",
                event.params["category"].as_str().unwrap_or("-")
            ),
            "mcp_mutation_completed" => {
                format!("mcp_mutation_completed:{}", event.params["success"])
            }
            other => other.to_string(),
        })
        .collect();
    names.join(",")
}

/// `ok`, or the refusal's code, argument and sorted data keys.
fn outcome(response: &Value) -> String {
    if response["result"]["isError"] != true {
        return "ok".to_string();
    }
    let error = mcp_error(response);
    let argument = error["argument"].as_str().unwrap_or("-");
    let mut keys: Vec<&str> = error["data"]
        .as_object()
        .map(|data| data.keys().map(String::as_str).collect())
        .unwrap_or_default();
    keys.sort_unstable();
    format!(
        "{} argument={argument} data={}",
        error["code"].as_str().unwrap_or("?"),
        keys.join(",")
    )
}

fn line(label: &str, server: &mut McpServer, tool: &str, arguments: Value) -> String {
    let before = server.state().events.len();
    let response = call_tool(server, tool, arguments);
    format!(
        "{label} {tool} => {} | events: {}",
        outcome(&response),
        events_since(server, before)
    )
}

#[test]
fn dispatch_refusals_today() {
    let mut served = writing();
    let mut nothing = serving_nothing();
    let mut lines = Vec::new();
    for tool in specforge_mcp::tools::CORE_TOOLS {
        lines.push(line(
            "served",
            &mut served,
            tool.name(),
            json!({"specforge_bogus": 1}),
        ));
    }
    for tool in specforge_mcp::tools::CORE_TOOLS {
        lines.push(line("nothing", &mut nothing, tool.name(), json!({})));
    }
    insta::assert_snapshot!(lines.join("\n"));
}
