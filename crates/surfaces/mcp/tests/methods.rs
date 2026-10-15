//! Which revision serves which MCP method: the method table
//! (`specforge_mcp::protocol::method`, ADR 0103 D5) as the wire shows it.

use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

const REVISION: &str = "2026-07-28";

/// Every method with whether the handshake revisions and the stateless
/// revision serve it, and whether the stateless result is cacheable.
const METHODS: [(&str, bool, bool, bool); 17] = [
    ("initialize", true, false, false),
    ("shutdown", true, false, false),
    ("ping", true, false, false),
    ("notifications/initialized", true, false, false),
    ("notifications/cancelled", true, true, false),
    ("$/cancelRequest", true, false, false),
    ("server/discover", false, true, true),
    ("subscriptions/listen", false, true, false),
    ("tools/list", true, true, true),
    ("prompts/list", true, true, true),
    ("resources/list", true, true, true),
    ("resources/templates/list", true, true, true),
    ("resources/read", true, true, true),
    ("tools/call", true, true, false),
    ("prompts/get", true, true, false),
    ("resources/subscribe", true, false, false),
    ("resources/unsubscribe", true, false, false),
];

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

/// The params the handler of `method` needs to be reached.
fn params(method: &str) -> Value {
    match method {
        "resources/read" | "resources/subscribe" | "resources/unsubscribe" => {
            json!({"uri": "specforge://graph"})
        }
        "tools/call" => json!({"name": "specforge.stats"}),
        "prompts/get" => json!({"name": "specforge://prompts/explore"}),
        "subscriptions/listen" => json!({"notifications": {}}),
        _ => json!({}),
    }
}

fn send(server: &mut McpServer, method: &str, params: Value) -> Option<Value> {
    let message = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    server
        .handle_message(&message.to_string())
        .map(|reply| serde_json::from_str(&reply).unwrap())
}

fn stateless(method: &str) -> Value {
    let mut params = params(method);
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": REVISION,
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    params
}

fn not_found(reply: &Option<Value>) -> bool {
    reply
        .as_ref()
        .is_some_and(|reply| reply["error"]["code"] == -32601)
}

#[specforge_test(
    behavior = "serve_stateless_mcp_requests",
    verify = "each method is answered in the revisions its method table row names and is -32601 in the others"
)]
fn each_method_is_served_in_its_revisions() {
    let dir = project();
    let mut handshake = McpServer::with_project_root(dir.path().to_path_buf());
    let init = send(
        &mut handshake,
        "initialize",
        json!({"protocolVersion": "2025-06-18"}),
    );
    assert!(init.is_some_and(|reply| reply["error"].is_null()));
    let mut modern = McpServer::with_project_root(dir.path().to_path_buf());
    // `shutdown` ends a handshake session: it goes last.
    let mut order: Vec<_> = METHODS.iter().filter(|row| row.0 != "shutdown").collect();
    order.extend(METHODS.iter().filter(|row| row.0 == "shutdown"));
    for &&(name, in_handshake, in_stateless, cacheable) in &order {
        let reply = send(&mut handshake, name, params(name));
        if name == "server/discover" {
            // Exists only in the stateless revision: without its `_meta`
            // the request is that revision's, and malformed.
            let reply = reply.unwrap();
            assert_eq!(reply["error"]["code"], -32602, "{name}: {reply}");
        } else {
            assert_eq!(
                not_found(&reply),
                !in_handshake,
                "{name} (handshake): {reply:?}"
            );
        }
        let reply = send(&mut modern, name, stateless(name));
        assert_eq!(
            not_found(&reply),
            !in_stateless,
            "{name} (stateless): {reply:?}"
        );
        if let Some(result) = reply
            .as_ref()
            .map(|reply| &reply["result"])
            .filter(|r| r.is_object())
        {
            let carries = !result["ttlMs"].is_null() && !result["cacheScope"].is_null();
            assert_eq!(carries, cacheable, "{name} (cache hints): {result}");
        }
    }
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a request that reads nothing of the project, such as server/discover, serves nothing; the next request that reads it serves the project created at the root"
)]
fn a_stateless_discover_serves_nothing() {
    let root = tempfile::TempDir::new().unwrap();
    let mut server = McpServer::with_project_root(root.path().to_path_buf());
    let reply = send(&mut server, "server/discover", stateless("server/discover"));
    assert!(reply.is_some_and(|reply| reply["error"].is_null()));
    // The project is created after the server started, before the discover.
    let created = project();
    for file in ["specforge.json", "main.spec"] {
        std::fs::copy(created.path().join(file), root.path().join(file)).unwrap();
    }
    let reply = send(&mut server, "server/discover", stateless("server/discover"));
    assert!(reply.is_some_and(|reply| reply["error"].is_null()));
    assert_eq!(server.state().project_root(), None);

    let mut call = stateless("tools/call");
    call["name"] = "specforge.stats".into();
    let stats = send(&mut server, "tools/call", call).unwrap();
    assert_eq!(
        stats["result"]["structuredContent"]["entity_count"], 1,
        "{stats}"
    );
    assert!(server.state().project_root().is_some());
}
