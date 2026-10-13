//! The stateless revision (MCP 2026-07-28): requests that name their
//! protocol version in `_meta`, served without `initialize`, and the
//! `subscriptions/listen` streams that replace `resources/subscribe`.

use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;
use std::path::{Path, PathBuf};

use crate::support::{Logged, events, logged, tool_json};
use crate::tool_errors::mcp_error;

const REVISION: &str = "2026-07-28";

/// A project on disk using `@specforge/software`, with behavior `alpha`.
fn project() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        json!({"name": "t", "version": "0.1.0", "extensions": ["@specforge/software"]}).to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("main.spec"),
        "behavior alpha \"Alpha\" {\n  contract \"MUST work\"\n}\n",
    )
    .unwrap();
    dir
}

/// A server for `dir` as `specforge mcp <dir>` starts it: nothing sent yet.
fn server(dir: &tempfile::TempDir) -> Logged {
    logged(McpServer::with_project_root(dir.path().to_path_buf()))
}

/// A server with no root (`McpServer::new()`): nothing served, nothing sent yet.
fn rootless() -> Logged {
    logged(McpServer::new())
}

/// The root the server serves, canonical.
fn served(server: &McpServer) -> Option<PathBuf> {
    server.state().project_root().map(canonical)
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// A stateless `tools/call` of `name`; its reply.
fn tool(server: &mut McpServer, name: &str, arguments: Value) -> Value {
    stateless(
        server,
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
}

/// Write a project at `root`: `specforge.json` with no extension, `main.spec` with `alpha`.
fn write_project(root: &Path) {
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"late","version":"0.1.0"}"#,
    )
    .unwrap();
    std::fs::write(root.join("main.spec"), "behavior alpha \"Alpha\" {\n}\n").unwrap();
}

/// The diagnostic codes a stateless `specforge.validate` reply reports.
fn codes(reply: &Value) -> Vec<String> {
    tool_json(reply)
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap().to_string())
        .collect()
}

/// The `_meta` a stateless request carries.
fn meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": REVISION,
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0"},
    })
}

/// `params` with the stateless `_meta` added.
fn with_meta(mut params: Value) -> Value {
    params["_meta"] = meta();
    params
}

fn send(server: &mut McpServer, id: u64, method: &str, params: Value) -> Option<Value> {
    let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    server
        .handle_message(&message.to_string())
        .map(|reply| serde_json::from_str(&reply).unwrap())
}

/// A stateless request's reply.
fn stateless(server: &mut McpServer, method: &str, params: Value) -> Value {
    send(server, 1, method, with_meta(params)).expect("a request gets a reply")
}

#[specforge_test(
    behavior = "serve_stateless_mcp_requests",
    verify = "server/discover is answered without initialize and lists 2026-07-28"
)]
fn discover_needs_no_initialize() {
    let dir = project();
    let mut server = server(&dir);
    let reply = stateless(&mut server, "server/discover", json!({}));
    let result = &reply["result"];
    assert_eq!(result["supportedVersions"], json!([REVISION]), "{reply}");
    assert_eq!(result["capabilities"]["resources"]["subscribe"], true);
    assert!(result["capabilities"]["tools"].is_object(), "{reply}");
    assert!(result["instructions"].is_string(), "{reply}");
}

#[specforge_test(
    behavior = "serve_stateless_mcp_requests",
    verify = "a request with a protocol version in its _meta is served without initialize"
)]
fn a_stateless_request_is_served_without_initialize() {
    let dir = project();
    let mut server = server(&dir);
    // The project `specforge mcp` was started for is compiled and served.
    let reply = stateless(
        &mut server,
        "tools/call",
        json!({"name": "specforge.query", "arguments": {"entity_id": "alpha"}}),
    );
    let result = &reply["result"];
    assert_eq!(result["isError"], false, "{reply}");
    assert_eq!(
        result["structuredContent"]["nodes"][0]["id"], "alpha",
        "{reply}"
    );

    // Without the _meta, the handshake rules still hold.
    let legacy = send(&mut server, 2, "tools/list", json!({})).unwrap();
    assert_eq!(legacy["error"]["code"], -32600, "{legacy}");
}

#[specforge_test(
    behavior = "serve_stateless_mcp_requests",
    verify = "a stateless result carries resultType complete and the server info"
)]
fn a_stateless_result_is_typed_and_names_the_server() {
    let dir = project();
    let mut server = server(&dir);
    let reply = stateless(
        &mut server,
        "tools/call",
        json!({"name": "specforge.stats", "arguments": {}}),
    );
    let result = &reply["result"];
    assert_eq!(result["resultType"], "complete", "{reply}");
    assert_eq!(
        result["_meta"]["io.modelcontextprotocol/serverInfo"],
        json!({"name": "specforge-mcp", "version": env!("CARGO_PKG_VERSION")})
    );
    // A tool result is not cacheable.
    assert!(result.get("ttlMs").is_none(), "{reply}");
}

#[specforge_test(
    behavior = "serve_stateless_mcp_requests",
    verify = "list and read results carry ttlMs and cacheScope"
)]
fn lists_and_reads_carry_caching_hints() {
    let dir = project();
    let mut server = server(&dir);
    for (method, params) in [
        ("server/discover", json!({})),
        ("tools/list", json!({})),
        ("prompts/list", json!({})),
        ("resources/list", json!({})),
        ("resources/templates/list", json!({})),
        ("resources/read", json!({"uri": "specforge://graph"})),
    ] {
        let reply = stateless(&mut server, method, params);
        let result = &reply["result"];
        assert_eq!(result["resultType"], "complete", "{method}: {reply}");
        assert_eq!(result["ttlMs"], 0, "{method}: {reply}");
        assert_eq!(result["cacheScope"], "private", "{method}: {reply}");
    }
}

#[specforge_test(
    behavior = "serve_stateless_mcp_requests",
    verify = "a stateless request without the client capabilities is -32602"
)]
fn a_request_without_client_capabilities_is_malformed() {
    let dir = project();
    let mut server = server(&dir);
    let version_only = json!({"_meta": {"io.modelcontextprotocol/protocolVersion": REVISION}});
    let reply = send(&mut server, 4, "tools/list", version_only).unwrap();
    assert_eq!(reply["id"], 4);
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("io.modelcontextprotocol/clientCapabilities"),
        "{reply}"
    );
    // server/discover exists only in this revision: without _meta it is
    // malformed too.
    let reply = send(&mut server, 5, "server/discover", json!({})).unwrap();
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
}

#[specforge_test(
    behavior = "serve_stateless_mcp_requests",
    verify = "an unsupported protocol version is -32022 naming the supported versions"
)]
fn an_unsupported_version_names_the_supported_ones() {
    let dir = project();
    let mut server = server(&dir);
    let mut params = with_meta(json!({}));
    params["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!("2025-11-25");
    let reply = send(&mut server, 6, "tools/list", params).unwrap();
    assert_eq!(reply["id"], 6);
    assert_eq!(reply["error"]["code"], -32022, "{reply}");
    assert_eq!(
        reply["error"]["data"],
        json!({"supported": [REVISION], "requested": "2025-11-25"})
    );
}

#[specforge_test(
    behavior = "serve_stateless_mcp_requests",
    verify = "ping and resources/subscribe are not stateless methods"
)]
fn the_revision_has_no_ping_or_resource_subscribe() {
    let dir = project();
    let mut server = server(&dir);
    for (method, params) in [
        ("ping", json!({})),
        ("resources/subscribe", json!({"uri": "specforge://graph"})),
        ("logging/setLevel", json!({"level": "info"})),
    ] {
        let reply = stateless(&mut server, method, params);
        assert_eq!(reply["error"]["code"], -32601, "{method}: {reply}");
    }
    // A handshake session keeps ping.
    send(
        &mut server,
        2,
        "initialize",
        json!({"protocolVersion": "2025-11-25"}),
    );
    let reply = send(&mut server, 3, "ping", json!({})).unwrap();
    assert_eq!(reply["result"], json!({}), "{reply}");
}

#[specforge_test(
    behavior = "serve_stateless_mcp_requests",
    verify = "a stateless request after initialize is served under its own revision"
)]
fn a_stateless_request_after_initialize_keeps_its_revision() {
    let dir = project();
    let mut server = server(&dir);
    // 2025-03-26 has no structuredContent.
    send(
        &mut server,
        1,
        "initialize",
        json!({"protocolVersion": "2025-03-26"}),
    );
    let call = json!({"name": "specforge.query", "arguments": {"entity_id": "alpha"}});
    let modern = stateless(&mut server, "tools/call", call.clone());
    assert_eq!(
        modern["result"]["structuredContent"]["nodes"][0]["id"], "alpha",
        "{modern}"
    );
    assert_eq!(modern["result"]["resultType"], "complete");

    // The handshake session is untouched.
    let legacy = send(&mut server, 2, "tools/call", call).unwrap();
    assert!(
        legacy["result"].get("structuredContent").is_none(),
        "{legacy}"
    );
    assert!(legacy["result"].get("resultType").is_none(), "{legacy}");
    assert_eq!(server.state().protocol_version(), "2025-03-26");
}

/// How many of the listed tools carry an `outputSchema`.
fn with_output_schema(reply: &Value) -> usize {
    reply["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("no tools in {reply}"))
        .iter()
        .filter(|tool| tool.get("outputSchema").is_some())
        .count()
}

#[specforge_test(
    behavior = "serve_stateless_mcp_requests",
    verify = "a stateless request's revision ends with the request"
)]
fn a_stateless_requests_revision_ends_with_it() {
    let dir = project();
    let mut server = server(&dir);
    let unknown = json!({"uri": "specforge://nope"});

    let stateless_read = stateless(&mut server, "resources/read", unknown.clone());
    assert_eq!(stateless_read["error"]["code"], -32602, "{stateless_read}");
    // The next request, without `_meta`, is a handshake request.
    let bare = send(&mut server, 2, "resources/read", unknown.clone()).unwrap();
    assert_eq!(bare["error"]["code"], -32600, "{bare}");
    assert_eq!(bare["error"]["message"], "Server not initialized");

    send(
        &mut server,
        3,
        "initialize",
        json!({"protocolVersion": "2025-03-26", "capabilities": {}}),
    );
    let handshake = send(&mut server, 4, "resources/read", unknown.clone()).unwrap();
    assert_eq!(handshake["error"]["code"], -32002, "{handshake}");
    let again = stateless(&mut server, "resources/read", unknown.clone());
    assert_eq!(again["error"]["code"], -32602, "{again}");
    let handshake = send(&mut server, 5, "resources/read", unknown).unwrap();
    assert_eq!(handshake["error"]["code"], -32002, "{handshake}");

    // 2025-03-26 has no structuredContent, so no outputSchema either.
    let modern = stateless(&mut server, "tools/list", json!({}));
    assert!(with_output_schema(&modern) > 0, "{modern}");
    let legacy = send(&mut server, 6, "tools/list", json!({})).unwrap();
    assert_eq!(with_output_schema(&legacy), 0, "{legacy}");
}

// --- subscriptions/listen ---

/// Open a listen stream with request id 7 for `filter`; the notifications
/// it queued.
fn listen(server: &mut McpServer, filter: Value) -> Vec<Value> {
    let reply = send(
        server,
        7,
        "subscriptions/listen",
        with_meta(json!({"notifications": filter})),
    );
    assert!(reply.is_none(), "the stream stays open: {reply:?}");
    server.take_notifications()
}

/// Any stateless request: it brings the served project up to date with
/// disk, and the update's notifications are queued.
fn bring_up_to_date(server: &mut McpServer) {
    stateless(
        server,
        "tools/call",
        json!({"name": "specforge.stats", "arguments": {}}),
    );
}

/// Add an entity to the project and bring it in with a request.
fn change_graph(server: &mut McpServer, dir: &tempfile::TempDir) {
    std::fs::write(
        dir.path().join("more.spec"),
        "behavior beta \"Beta\" {\n  contract \"MUST also work\"\n}\n",
    )
    .unwrap();
    bring_up_to_date(server);
}

#[specforge_test(
    behavior = "listen_for_mcp_resource_updates",
    verify = "subscriptions/listen is acknowledged first with the resources honoured"
)]
fn listen_is_acknowledged_with_what_is_honoured() {
    let dir = project();
    let mut server = server(&dir);
    let queued = listen(
        &mut server,
        json!({
            "resourceSubscriptions": ["specforge://graph", "specforge://nope"],
            "toolsListChanged": true,
        }),
    );
    assert_eq!(
        queued,
        [json!({
            "jsonrpc": "2.0",
            "method": "notifications/subscriptions/acknowledged",
            "params": {
                "_meta": {"io.modelcontextprotocol/subscriptionId": 7},
                "notifications": {"resourceSubscriptions": ["specforge://graph"]},
            },
        })]
    );
}

#[specforge_test(
    behavior = "listen_for_mcp_resource_updates",
    verify = "a recompile that changes a listened resource sends resources/updated with the subscription id"
)]
fn a_change_reaches_the_stream() {
    let dir = project();
    let mut server = server(&dir);
    listen(
        &mut server,
        json!({"resourceSubscriptions": ["specforge://graph"]}),
    );

    // A request that finds nothing changed sends nothing.
    bring_up_to_date(&mut server);
    assert!(server.take_notifications().is_empty());

    change_graph(&mut server, &dir);
    assert_eq!(
        server.take_notifications(),
        [json!({
            "jsonrpc": "2.0",
            "method": "notifications/resources/updated",
            "params": {
                "_meta": {"io.modelcontextprotocol/subscriptionId": 7},
                "uri": "specforge://graph",
            },
        })]
    );
}

#[specforge_test(
    behavior = "listen_for_mcp_resource_updates",
    verify = "a listen stream receives no notification type it did not ask for"
)]
fn a_stream_hears_only_what_it_asked_for() {
    let dir = project();
    let mut server = server(&dir);
    // Only the diagnostics: a graph change it doesn't watch sends nothing,
    // and never the handshake era's specforge/graphChanged.
    listen(
        &mut server,
        json!({"resourceSubscriptions": ["specforge://diagnostics"]}),
    );
    change_graph(&mut server, &dir);
    let sent = server.take_notifications();
    assert!(
        sent.iter()
            .all(|n| n["method"] == "notifications/resources/updated"
                && n["params"]["uri"] == "specforge://diagnostics"),
        "{sent:?}"
    );

    // A broken spec changes the diagnostics: that is heard.
    std::fs::write(dir.path().join("broken.spec"), "behavior {\n").unwrap();
    bring_up_to_date(&mut server);
    let sent = server.take_notifications();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["params"]["uri"], "specforge://diagnostics");
}

#[specforge_test(
    behavior = "listen_for_mcp_resource_updates",
    verify = "cancelling the listen request ends the stream"
)]
fn cancelling_the_listen_request_ends_the_stream() {
    let dir = project();
    let mut server = server(&dir);
    listen(
        &mut server,
        json!({"resourceSubscriptions": ["specforge://graph"]}),
    );
    let cancel = json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": {"requestId": 7, "reason": "done"},
    });
    assert!(server.handle_message(&cancel.to_string()).is_none());
    assert!(server.state().subscriptions().is_empty());
    assert!(
        events(&server, "mcp_subscription_removed")
            .iter()
            .any(|e| e["resourceUri"] == "specforge://graph"),
        "no mcp_subscription_removed"
    );

    change_graph(&mut server, &dir);
    assert!(server.take_notifications().is_empty());
}

#[specforge_test(
    behavior = "listen_for_mcp_resource_updates",
    verify = "the end of the connection ends the stream"
)]
fn the_end_of_the_connection_ends_the_stream() {
    let dir = project();
    let mut server = server(&dir);
    listen(
        &mut server,
        json!({"resourceSubscriptions": ["specforge://graph"]}),
    );
    server.disconnect();
    assert_eq!(
        events(&server, "mcp_subscription_removed"),
        [json!({"resourceUri": "specforge://graph", "subscriptionId": "7"})]
    );
    change_graph(&mut server, &dir);
    assert!(server.take_notifications().is_empty());
}

#[specforge_test(
    behavior = "serve_mcp_prompt",
    verify = "a stateless prompts/get renders without initialize"
)]
fn stateless_prompt_get_renders() {
    let dir = project();
    let mut server = server(&dir);
    let reply = stateless(
        &mut server,
        "prompts/get",
        json!({"name": "specforge://prompts/context", "arguments": {"entity_id": "alpha"}}),
    );
    let result = &reply["result"];
    assert_eq!(result["resultType"], "complete", "{reply}");
    let messages = result["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 2, "{reply}");
    let payload: Value =
        serde_json::from_str(messages[1]["content"]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["entity_id"], "alpha");
    assert_eq!(payload["contract_text"], "MUST work");
}

#[test]
fn pin_a_stateless_path_is_adopted() {
    let dir = project();
    let mut server = rootless();
    let reply = tool(
        &mut server,
        "specforge.validate",
        json!({"path": dir.path().to_str().unwrap()}),
    );
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    assert_eq!(served(&server), Some(canonical(dir.path())));
    let stats = tool(&mut server, "specforge.stats", json!({}));
    assert_eq!(tool_json(&stats)["entity_count"], 1, "{stats}");
}

#[test]
fn pin_a_stateless_init_serves_what_it_created() {
    let parent = tempfile::TempDir::new().unwrap();
    let created = parent.path().join("created");
    let mut server = rootless();
    let reply = tool(
        &mut server,
        "specforge.init",
        json!({"path": created.to_str().unwrap(), "name": "created"}),
    );
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    assert_eq!(served(&server), Some(canonical(&created)));
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "while nothing is served, a project created at or above the server's root is served by the next request, in either revision"
)]
fn a_project_created_at_the_root_is_served_by_the_next_stateless_request() {
    let root = tempfile::TempDir::new().unwrap();
    let mut server = logged(McpServer::with_project_root(root.path().to_path_buf()));
    stateless(&mut server, "server/discover", json!({}));
    write_project(root.path());
    let stats = tool(&mut server, "specforge.stats", json!({}));
    assert_eq!(tool_json(&stats)["entity_count"], 1, "{stats}");
    assert_eq!(served(&server), Some(canonical(root.path())));
    let extensions = tool(&mut server, "specforge.extensions", json!({}));
    assert_eq!(extensions["result"]["isError"], false, "{extensions}");
}

#[test]
fn pin_a_stateless_use_cached_reads_an_earlier_compile() {
    let dir = project();
    let mut server = server(&dir);
    assert!(!codes(&tool(&mut server, "specforge.validate", json!({}))).contains(&"E001".into()));
    std::fs::write(dir.path().join("broken.spec"), "behavior {\n").unwrap();
    let cached = codes(&tool(
        &mut server,
        "specforge.validate",
        json!({"use_cached": true}),
    ));
    assert!(!cached.contains(&"E001".to_string()), "{cached:?}");
    let mut fresh = self::server(&dir);
    let control = codes(&tool(
        &mut fresh,
        "specforge.validate",
        json!({"use_cached": true}),
    ));
    assert!(control.contains(&"E001".to_string()), "{control:?}");
}

#[test]
fn pin_a_stateless_no_project_refusal_says_to_call_a_tool_with_a_path() {
    let mut server = rootless();
    let reply = tool(&mut server, "specforge.extensions", json!({}));
    let message = mcp_error(&reply)["message"].as_str().unwrap().to_string();
    assert!(
        message.contains("call a tool that takes a `path` first"),
        "{message}"
    );
}

#[specforge_test(
    behavior = "serve_stateless_mcp_requests",
    verify = "a stateless request after initialize reads the project the handshake session serves"
)]
fn a_stateless_request_after_initialize_reads_what_the_session_serves() {
    let alpha = |server: &mut McpServer| {
        let reply = tool(server, "specforge.query", json!({"entity_id": "alpha"}));
        tool_json(&reply)["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|n| n["id"] == "alpha"))
    };
    let dir = project();
    let mut named = rootless();
    send(
        &mut named,
        1,
        "initialize",
        json!({"protocolVersion": "2025-11-25", "projectRoot": dir.path().to_str().unwrap()}),
    );
    assert!(alpha(&mut named));

    let mut adopted = rootless();
    send(&mut adopted, 1, "initialize", json!({}));
    let reply = send(
        &mut adopted,
        2,
        "tools/call",
        json!({"name": "specforge.validate", "arguments": {"path": dir.path().to_str().unwrap()}}),
    )
    .unwrap();
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    assert!(alpha(&mut adopted));
}
