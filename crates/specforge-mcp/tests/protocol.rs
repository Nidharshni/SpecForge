use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

fn call_raw(server: &mut McpServer, input: &str) -> Option<String> {
    server.handle_message(input)
}

// -- JSON-RPC 2.0 framing --

// B:handle_mcp_protocol_error — verify unit "parse error returns -32700"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "malformed JSON produces -32700 Parse error"
)]
fn parse_error_returns_32700() {
    let mut server = McpServer::new();
    let resp = call_raw(&mut server, "not valid json").unwrap();
    let parsed: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(parsed["error"]["code"], -32700);
    assert_eq!(parsed["jsonrpc"], "2.0");
    // JSON-RPC 2.0: the id is required, and null when it could not be read.
    assert_eq!(parsed.get("id"), Some(&Value::Null), "{resp}");
}

// B:handle_mcp_protocol_error — verify unit "invalid request returns -32600"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "returns -32600 for invalid request"
)]
fn invalid_request_returns_32600() {
    let mut server = McpServer::new();
    let resp = call_raw(&mut server, r#"{"id":1}"#).unwrap();
    let parsed: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(parsed["error"]["code"], -32600);
    assert_eq!(parsed["id"], 1, "{resp}");
}

// B:handle_mcp_protocol_error — verify unit "missing method returns -32600"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "returns -32600 for invalid request"
)]
fn missing_method_returns_32600() {
    let mut server = McpServer::new();
    let resp = call_raw(&mut server, r#"{"jsonrpc":"2.0","id":1}"#).unwrap();
    let parsed: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(parsed["error"]["code"], -32600);
}

// B:handle_mcp_protocol_error — verify unit "unknown method returns -32601"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "invalid method produces -32601 Method not found"
)]
fn unknown_method_returns_32601() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "nonexistent_method", json!({}));
    assert_eq!(resp["error"]["code"], -32601);
}

// B:handle_mcp_protocol_error — verify unit "invalid jsonrpc version returns -32600"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "returns -32600 for invalid request"
)]
fn invalid_jsonrpc_version_returns_32600() {
    let mut server = McpServer::new();
    let resp = call_raw(&mut server, r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#).unwrap();
    let parsed: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(parsed["error"]["code"], -32600);
}

// B:handle_mcp_protocol_error — verify unit "response always has jsonrpc 2.0 field"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "response always has jsonrpc 2.0 field"
)]
fn response_always_has_jsonrpc_field() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "ping", json!({}));
    assert_eq!(resp["jsonrpc"], "2.0");
}

// B:handle_mcp_protocol_error — verify unit "error response includes id from request"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "error response includes id from request"
)]
fn error_response_includes_request_id() {
    let mut server = McpServer::new();
    let req = json!({"jsonrpc": "2.0", "id": 42, "method": "nonexistent"});
    let resp = server.handle_message(&req.to_string()).unwrap();
    let parsed: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(parsed["id"], 42);
    assert!(parsed["error"].is_object());
}

// B:handle_mcp_protocol_error — verify unit "success response includes id from request"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "success response includes id from request"
)]
fn success_response_includes_request_id() {
    let mut server = McpServer::new();
    let req = json!({"jsonrpc": "2.0", "id": 99, "method": "ping"});
    let resp = server.handle_message(&req.to_string()).unwrap();
    let parsed: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(parsed["id"], 99);
    assert!(parsed["result"].is_object());
}

#[test]
fn cancel_request_returns_success() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "$/cancelRequest", json!({"id": 1}));
    assert!(resp["result"].is_object());
}

// Notifications (no id) should not produce a response
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "notifications produce no response"
)]
fn notifications_produce_no_response() {
    let mut server = McpServer::new();
    let req = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let resp = server.handle_message(&req.to_string());
    assert!(resp.is_none());
}

fn init_server() -> Logged {
    let mut server = logged(McpServer::new());
    call(&mut server, "initialize", json!({}));
    server
}

// B:handle_mcp_protocol_error — verify unit "missing required params produces -32602"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "missing required params produces -32602 Invalid params"
)]
fn missing_tool_name_returns_32602() {
    let mut server = init_server();
    let resp = call(&mut server, "tools/call", json!({}));
    assert_eq!(resp["error"]["code"], -32602);
}

// B:handle_mcp_protocol_error — verify unit "error response does not leak internal state"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "error response does not leak internal state"
)]
fn error_does_not_leak_internal_state() {
    let mut server = server_with_corrupt_inference_manifest();
    let root = server.root().to_str().unwrap().to_string();
    let failing = [
        ("nonexistent_method", json!({})),
        // Internal failure: the inference manifest does not parse.
        (
            "tools/call",
            json!({"name": "specforge.infer_session", "arguments": {"action": "start"}}),
        ),
    ];
    let mut responses: Vec<Value> = failing
        .into_iter()
        .map(|(method, params)| call(&mut server, method, params))
        .collect();
    // Internal failure: a file read inside the project root fails.
    server.remove("specforge-infer.json");
    responses.push(call(
        &mut server,
        "tools/call",
        json!({"name": "specforge.infer_session",
            "arguments": {"action": "mark_analyzed", "source_file": "src/missing.rs"}}),
    ));

    // A server fault: a handler that panics.
    let ext = crate::fake_extension::FakeExtension::new().with_panic("mcp__check");
    let (mut faulty, _ext, _dir) = crate::fake_extension::initialized(ext);
    responses.push(call(
        &mut faulty,
        "tools/call",
        json!({"name": "specforge.cmds.check", "arguments": {}}),
    ));

    for resp in responses {
        // A protocol error, or a failed tool call's McpError.
        let error = match resp["error"].as_object() {
            Some(error) => {
                let mut keys: Vec<&str> = error.keys().map(String::as_str).collect();
                keys.sort_unstable();
                assert!(
                    keys == ["code", "message"] || keys == ["code", "data", "message"],
                    "only code/message/data: {resp}"
                );
                Value::Object(error.clone())
            }
            None => crate::tool_errors::mcp_error(&resp),
        };
        let message = error["message"].as_str().unwrap();
        assert!(!message.is_empty());
        for leak in [root.as_str(), "panicked", "RUST_BACKTRACE", ".rs:", "0x"] {
            assert!(!message.contains(leak), "{leak:?} leaks in {message:?}");
        }
    }
}

// B:handle_mcp_protocol_error — verify unit "server remains operational after protocol error"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "server remains operational after protocol error"
)]
fn server_operational_after_protocol_error() {
    let mut server = McpServer::new();
    call_raw(&mut server, "not valid json");
    let resp = call(&mut server, "ping", json!({}));
    assert!(resp["result"].is_object());
    assert!(resp["error"].is_null());
}

// B:handle_mcp_protocol_error — verify unit "returns -32603 for internal error"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "returns -32603 for internal error"
)]
fn internal_error_code_defined() {
    // An extension tool whose handler panics: a server fault, not a tool
    // failure.
    let ext = crate::fake_extension::FakeExtension::new().with_panic("mcp__check");
    let (mut server, _ext, _dir) = crate::fake_extension::initialized(ext);
    let resp = call(
        &mut server,
        "tools/call",
        json!({"name": "specforge.cmds.check", "arguments": {}}),
    );
    assert_eq!(resp["error"]["code"], -32603, "{resp}");
    assert_eq!(resp["id"], 1, "{resp}");
    assert!(
        !resp["error"]["message"]
            .as_str()
            .unwrap()
            .contains("panicked"),
        "{resp}"
    );
    // The server stays up and keeps serving tools.
    assert!(call(&mut server, "ping", json!({}))["result"].is_object());
    let stats = call(
        &mut server,
        "tools/call",
        json!({"name": "specforge.stats", "arguments": {}}),
    );
    assert_eq!(stats["result"]["isError"], false, "{stats}");
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "a failed tool call is an isError result whose content is an McpError with a code"
)]
fn a_tool_that_cannot_read_its_project_file_fails_with_an_mcp_error() {
    // An execution failure inside the tool: the inference manifest does
    // not parse. Before D4-a this was a -32603 the agent could not act on.
    let mut server = server_with_corrupt_inference_manifest();
    let resp = call(
        &mut server,
        "tools/call",
        json!({"name": "specforge.infer_session", "arguments": {"action": "start"}}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert_eq!(error["tool"], "specforge.infer_session", "{error}");
    assert_eq!(error["diagnostic"]["code"], "E071", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("specforge-infer.json"),
        "{error}"
    );
}

#[specforge_test(
    behavior = "handle_mcp_request_cancellation",
    verify = "notifications/cancelled is accepted without a response"
)]
fn mcp_cancel_notification_gets_no_response() {
    let mut server = init_server();
    let req = json!({"jsonrpc": "2.0", "id": 7, "method": "ping", "params": {}});
    server.handle_message(&req.to_string());
    let note = json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
        "params": {"requestId": 7, "reason": "user aborted"}});

    assert!(server.handle_message(&note.to_string()).is_none());
    let event = events(&server, "mcp_request_cancelled")
        .into_iter()
        .next()
        .expect("mcp_request_cancelled emitted");
    assert_eq!(event["requestId"], "7");
    assert_eq!(event["wasInProgress"], false);
}

#[specforge_test(
    behavior = "handle_mcp_request_cancellation",
    verify = "cancellation of completed request is a no-op"
)]
fn cancel_completed_request_is_noop() {
    let mut server = init_server();
    let req = json!({"jsonrpc": "2.0", "id": 5, "method": "ping", "params": {}});
    server.handle_message(&req.to_string());
    let resp = call(&mut server, "$/cancelRequest", json!({"id": 5}));
    assert!(resp["result"].is_object());
    assert!(resp["error"].is_null());
}

#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "a tool that detects invalid arguments returns an isError result, not -32602"
)]
fn a_missing_tool_argument_is_an_is_error_result() {
    let mut server = init_server();
    // A well-formed tools/call whose tool lacks a required argument: the
    // tool's input is invalid, the request is not (MCP 2025-11-25).
    let req = json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "tools/call",
        "params": { "name": "specforge.query" }
    });
    let resp_str = server.handle_message(&req.to_string()).unwrap();
    let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["message"], "Missing required parameter: entity_id");
    assert_eq!(error["argument"], "entity_id", "{error}");
    assert_eq!(error["tool"], "specforge.query", "{error}");
}

#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "tools/call arguments that are not an object produce -32602 Invalid params"
)]
fn arguments_that_are_not_an_object_are_invalid_params() {
    let mut server = init_server();
    let resp = call(
        &mut server,
        "tools/call",
        json!({"name": "specforge.query", "arguments": "entity_id=x"}),
    );
    assert_eq!(resp["error"]["code"], -32602, "{resp}");
}

/// An initialized server over a temp project whose inference manifest is
/// corrupt, so reading it fails inside the server.
fn server_with_corrupt_inference_manifest() -> Served {
    TestProject::new()
        .file("specforge-infer.json", "{ not json")
        .serve(&[])
}
