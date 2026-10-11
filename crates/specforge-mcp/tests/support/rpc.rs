//! The one request-helper set. Every request has id 1; a request with no
//! reply panics, naming the method.

use serde_json::{Value, json};
use specforge_mcp::McpServer;

/// Send the request `method` with `params`; its reply.
pub fn call(server: &mut McpServer, method: &str, params: Value) -> Value {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let reply = server
        .handle_message(&request.to_string())
        .unwrap_or_else(|| panic!("{method} has no reply"));
    serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{method}: not JSON ({e}): {reply}"))
}

/// Call the tool `name` with `arguments`; the reply.
pub fn call_tool(server: &mut McpServer, name: &str, arguments: Value) -> Value {
    call(
        server,
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
}

/// A tool response's first text content.
pub fn tool_text(response: &Value) -> String {
    response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no tool result text in {response}"))
        .to_string()
}

/// That text parsed as JSON (panics, naming the response, when it is not).
pub fn tool_json(response: &Value) -> Value {
    let text = tool_text(response);
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("tool text is not JSON ({e}): {response}"))
}

/// [`call_tool`], then [`tool_json`].
pub fn tool(server: &mut McpServer, name: &str, arguments: Value) -> Value {
    tool_json(&call_tool(server, name, arguments))
}

/// Get the prompt `name` with `arguments`; the reply.
pub fn get_prompt(server: &mut McpServer, name: &str, arguments: Value) -> Value {
    call(
        server,
        "prompts/get",
        json!({"name": name, "arguments": arguments}),
    )
}

/// A prompt's payload text: its second user message (the first is the
/// instruction; ADR 0004 amendment).
pub fn prompt_text(response: &Value) -> String {
    response["result"]["messages"][1]["content"]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no prompt payload in {response}"))
        .to_string()
}

/// A prompt's JSON payload: its second user message, parsed.
pub fn prompt_payload(response: &Value) -> Value {
    let text = prompt_text(response);
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("prompt payload is not JSON ({e}): {response}"))
}

/// Read the resource `uri`; the reply.
pub fn read_resource(server: &mut McpServer, uri: &str) -> Value {
    call(server, "resources/read", json!({"uri": uri}))
}

/// A resource read's first content text.
pub fn resource_text(response: &Value) -> String {
    response["result"]["contents"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no resource text in {response}"))
        .to_string()
}

/// A resource read's first content item, and its text parsed as JSON.
pub fn resource(server: &mut McpServer, uri: &str) -> (Value, Value) {
    let response = read_resource(server, uri);
    let text = resource_text(&response);
    let parsed = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("resource text is not JSON ({e}): {response}"));
    (response["result"]["contents"][0].clone(), parsed)
}

/// The params of every `name` event, oldest first, each without its
/// `timestamp` once that is checked to be RFC 3339 (`mcp_initialized` has
/// none).
pub fn events(server: &McpServer, name: &str) -> Vec<Value> {
    server
        .state()
        .events
        .iter()
        .filter(|e| e.name == name)
        .map(|e| {
            let mut params = e.params.clone();
            if name != "mcp_initialized" {
                let stamp = params
                    .as_object_mut()
                    .and_then(|o| o.remove("timestamp"))
                    .unwrap_or_else(|| panic!("{name} has no timestamp: {}", e.params));
                let stamp = stamp.as_str().unwrap_or_default();
                assert!(
                    chrono::DateTime::parse_from_rfc3339(stamp).is_ok(),
                    "{name} timestamp is not RFC 3339: {stamp}"
                );
            }
            params
        })
        .collect()
}

/// `mcp_mutation_completed`'s params (without `timestamp`, as [`events`]
/// returns them) for `tool`.
pub fn completed(tool: &str, files: usize, entities: usize, success: bool) -> Value {
    json!({
        "toolName": tool,
        "files_changed": files,
        "entities_affected": entities,
        "success": success,
    })
}

/// Panics unless an `mcp_tool_invoked` event names `tool`.
pub fn assert_tool_invoked(server: &McpServer, tool: &str) {
    let invoked = events(server, "mcp_tool_invoked");
    assert!(
        invoked.iter().any(|p| p["toolName"] == tool),
        "no mcp_tool_invoked for {tool}: {invoked:?}"
    );
}

/// Panics unless an `mcp_prompt_invoked` event names `prompt`.
pub fn assert_prompt_invoked(server: &McpServer, prompt: &str) {
    let invoked = events(server, "mcp_prompt_invoked");
    assert!(
        invoked.iter().any(|p| p["promptName"] == prompt),
        "no mcp_prompt_invoked for {prompt}: {invoked:?}"
    );
}

/// The core tools as `tools/list` describes them, in listing order.
pub fn core_tools() -> Vec<specforge_mcp::types::McpToolDescriptor> {
    specforge_mcp::tools::CORE_TOOLS
        .iter()
        .map(|tool| tool.descriptor())
        .collect()
}

/// Open a `subscriptions/listen` stream (MCP 2026-07-28) with request id
/// `id` on `uris`; the notifications it queued (its acknowledgement).
pub fn listen(server: &mut McpServer, id: Value, uris: &[&str]) -> Vec<Value> {
    let request = json!({
        "jsonrpc": "2.0", "id": id, "method": "subscriptions/listen",
        "params": {
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {},
            },
            "notifications": {"resourceSubscriptions": uris},
        },
    });
    assert!(
        server.handle_message(&request.to_string()).is_none(),
        "a listen stream has no reply"
    );
    server.take_notifications()
}
