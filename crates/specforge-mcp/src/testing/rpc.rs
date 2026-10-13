//! The one request-helper set. Every request has id 1; a request with no
//! reply panics, naming the method.

use crate::McpServer;
use serde_json::{Value, json};

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

/// The latest handshake revision: the one `initialize` negotiates by default.
pub fn latest() -> crate::lifecycle::Revision {
    crate::lifecycle::Revision::Negotiated(crate::lifecycle::LATEST_PROTOCOL_VERSION)
}

/// The core tools as `tools/list` describes them, in listing order.
pub fn core_tools() -> Vec<crate::registry::ToolDescriptor<'static>> {
    crate::tools::CORE_TOOLS
        .iter()
        .map(|tool| tool.descriptor(latest()))
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
