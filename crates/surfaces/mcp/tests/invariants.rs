use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

/// A server over a project holding `alpha`, a behavior with one
/// obligation, and `beta`, the feature that has it (test.spec).
fn test_server() -> Served {
    TestProject::new()
        .file(
            "test.spec",
            "behavior alpha \"Alpha\" {\n    contract \"MUST work\"\n    features [beta]\n    verify unit \"works\"\n}\nfeature beta \"Beta\" {\n    problem \"Users need it\"\n}\n",
        )
        .serve(&[TestExtension::software()])
}

// I:mcp_structured_error_responses — verify property "error response includes error code and message fields"
#[specforge_test(
    behavior = "mcp_structured_error_responses",
    verify = "error response includes error code and message fields"
)]
fn error_responses_have_code_and_message() {
    let mut server = McpServer::new();

    // Test parse error
    let resp = server.handle_message("invalid json").unwrap();
    let parsed: Value = serde_json::from_str(&resp).unwrap();
    assert!(parsed["error"]["code"].is_number());
    assert!(parsed["error"]["message"].is_string());

    // Test method not found
    let resp2 = call(&mut server, "nonexistent", json!({}));
    assert!(resp2["error"]["code"].is_number());
    assert!(resp2["error"]["message"].is_string());
}

// I:mcp_structured_error_responses — verify property "error codes are valid JSON-RPC codes"
#[test]
fn error_codes_are_valid() {
    let mut server = McpServer::new();

    // Parse error
    let resp = server.handle_message("not json").unwrap();
    let parsed: Value = serde_json::from_str(&resp).unwrap();
    let code = parsed["error"]["code"].as_i64().unwrap();
    assert_eq!(code, -32700);

    // Method not found
    let resp2 = call(&mut server, "nonexistent", json!({}));
    let code2 = resp2["error"]["code"].as_i64().unwrap();
    assert_eq!(code2, -32601);
}

// I:mcp_structured_error_responses — verify property "success responses never have error field"
#[specforge_test(
    behavior = "mcp_structured_error_responses",
    verify = "success responses never have error field"
)]
fn success_never_has_error() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "initialize", json!({}));
    assert!(resp["result"].is_object());
    assert!(resp["error"].is_null());
}

// I:mcp_tool_idempotency — verify property "query is idempotent"
#[specforge_test(
    behavior = "mcp_tool_idempotency",
    verify = "read-only tools return equivalent results for identical inputs"
)]
fn query_is_idempotent() {
    let mut server = test_server();
    let resp1 = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha"}),
    );
    let resp2 = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha"}),
    );
    let text1 = resp1["result"]["content"][0]["text"].as_str().unwrap();
    let text2 = resp2["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(text1, text2);
}

// I:mcp_tool_idempotency — verify property "export is idempotent"
#[specforge_test(
    behavior = "mcp_tool_idempotency",
    verify = "repeated calls with same params return identical results when graph unchanged"
)]
fn export_is_idempotent() {
    let mut server = test_server();
    let resp1 = call_tool(&mut server, "specforge.export", json!({"format": "graph"}));
    let resp2 = call_tool(&mut server, "specforge.export", json!({"format": "graph"}));
    let text1 = resp1["result"]["content"][0]["text"].as_str().unwrap();
    let text2 = resp2["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(text1, text2);
}

// I:mcp_tool_idempotency — verify property "stats is idempotent"
#[specforge_test(
    behavior = "mcp_tool_idempotency",
    verify = "read-only tools return equivalent results for identical inputs"
)]
fn stats_is_idempotent() {
    let mut server = test_server();
    let resp1 = call_tool(&mut server, "specforge.stats", json!({}));
    let resp2 = call_tool(&mut server, "specforge.stats", json!({}));
    let text1 = resp1["result"]["content"][0]["text"].as_str().unwrap();
    let text2 = resp2["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(text1, text2);
}

// I:mcp_tool_idempotency — verify property "trace is idempotent"
#[specforge_test(
    behavior = "mcp_tool_idempotency",
    verify = "read-only tools return equivalent results for identical inputs"
)]
fn trace_is_idempotent() {
    let mut server = test_server();
    let resp1 = call_tool(
        &mut server,
        "specforge.trace",
        json!({"entity_id": "alpha"}),
    );
    let resp2 = call_tool(
        &mut server,
        "specforge.trace",
        json!({"entity_id": "alpha"}),
    );
    let text1 = resp1["result"]["content"][0]["text"].as_str().unwrap();
    let text2 = resp2["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(text1, text2);
}

#[specforge_test(
    behavior = "mcp_subscription_cleanup",
    verify = "no orphan subscriptions remain after disconnect"
)]
fn no_orphan_subscriptions_after_disconnect() {
    let mut server = McpServer::new();
    call(&mut server, "initialize", json!({}));
    for uri in ["specforge://graph", "specforge://diagnostics"] {
        let resp = call(&mut server, "resources/subscribe", json!({"uri": uri}));
        assert!(resp["result"].is_object(), "{resp}");
    }
    assert!(!server.state().subscriptions().is_empty());

    server.disconnect();
    assert!(
        server.state().subscriptions().is_empty(),
        "no subscription left behind"
    );
}

#[test]
fn no_subscriptions_survive_shutdown() {
    let mut server = McpServer::new();
    call(&mut server, "initialize", json!({}));

    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert!(!server.state().subscriptions().is_empty());

    call(&mut server, "shutdown", json!({}));
    assert!(server.state().subscriptions().is_empty());
}

// I:mcp_structured_error_responses — verify property "error response includes entity_id when applicable"
#[specforge_test(
    behavior = "mcp_structured_error_responses",
    verify = "error response includes entity_id when applicable"
)]
fn error_includes_entity_id_when_applicable() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.inspect",
        json!({"entity_id": "unknown_entity"}),
    );
    // Entity-not-found is a tool execution error, not a protocol error.
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "entity_not_found", "{error}");
    assert_eq!(
        error["entity_id"], "unknown_entity",
        "the error names the offending entity: {error}"
    );
}

// I:mcp_structured_error_responses — verify property "no MCP endpoint returns a plain string error"
#[specforge_test(
    behavior = "mcp_structured_error_responses",
    verify = "no MCP endpoint returns a plain string error"
)]
fn no_core_tool_fails_with_a_plain_string_or_an_error_body() {
    let mut server = test_server();
    let probes = specforge_mcp::tools::CORE_TOOLS
        .iter()
        .map(|tool| (tool.name(), json!({})))
        .chain([
            ("specforge.inspect", json!({"entity_id": "nonexistent"})),
            (
                "specforge.find_definition",
                json!({"entity_id": "nonexistent"}),
            ),
            (
                "specforge.find_references",
                json!({"entity_id": "nonexistent"}),
            ),
            ("specforge.query", json!({"entity_id": "nonexistent"})),
            ("specforge.trace", json!({"entity_id": "nonexistent"})),
        ]);
    for (name, arguments) in probes {
        let resp = call_tool(&mut server, name, arguments.clone());
        assert!(
            resp.get("error").is_none(),
            "{name} {arguments}: a known tool's failure is a result: {resp}"
        );
        if resp["result"]["isError"] == true {
            // An McpError, never a bare message.
            crate::tool_errors::mcp_error(&resp);
        } else if let Some(text) = resp["result"]["content"][0]["text"].as_str()
            && let Ok(Value::Object(body)) = serde_json::from_str::<Value>(text)
        {
            // A success never reports a failure in its body.
            assert!(
                !body.contains_key("error"),
                "{name} {arguments}: success with an error body: {text}"
            );
        }
    }
}
