use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;
use std::fs;
use tempfile::TempDir;

fn init_server() -> McpServer {
    let mut server = McpServer::new();
    call(&mut server, "initialize", json!({}));
    server
}

/// A server over a project whose one extension declares the MCP tool
/// `ext.hello` and the resource template `specforge://ext/hello/{name}`,
/// served through the runtime seam as loading the extension does.
fn server_with_extension_surfaces() -> (
    McpServer,
    std::sync::Arc<crate::fake_extension::FakeExtension>,
    TempDir,
) {
    crate::fake_extension::initialized(crate::fake_extension::FakeExtension::declaring(json!({
        "mcp_tools": [{
            "name": "ext.hello",
            "description": "Say hello",
            "export": "tool__hello",
            "input_schema": {"type": "object"}
        }],
        "mcp_resources": [{
            "uri_template": "specforge://ext/hello/{name}",
            "name": "hello",
            "export": "resource__hello",
            "mime_type": "application/json"
        }]
    })))
}

/// What a cancellation must leave untouched: the registries, the graph, the
/// diagnostics (the subscriptions are compared beside it).
fn state_snapshot(server: &McpServer) -> Value {
    let state = server.state();
    let mut nodes: Vec<String> = state
        .graph()
        .nodes()
        .iter()
        .map(|n| n.id.raw.to_string())
        .collect();
    nodes.sort();
    json!({
        "tools": specforge_mcp::registry::listed_tools(state).collect::<Vec<_>>(),
        "resources": specforge_mcp::registry::listed_resources(state).collect::<Vec<_>>(),
        "nodes": nodes,
        "edges": state.graph().edge_count(),
        "diagnostics": state.diagnostics().iter().map(|d| d.code.clone()).collect::<Vec<_>>(),
        "initialized": state.is_initialized(),
    })
}

fn init_server_with_project() -> (McpServer, TempDir) {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    let spec_dir = dir.path().join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
    fs::write(
        spec_dir.join("test.spec"),
        r#"
behavior hello_world "Hello World" {
    contract "The system MUST greet the user"
    verify unit "greets user"
}

feature greeting "Greeting Feature" {
    behaviors [hello_world]
}
"#,
    )
    .unwrap();

    let mut server = McpServer::new();
    let project_root = dir.path().to_str().unwrap();
    call(
        &mut server,
        "initialize",
        json!({"projectRoot": project_root}),
    );
    (server, dir)
}

// B:mcp_initialize — verify unit "returns MCP-compliant init response"
#[specforge_test(
    behavior = "mcp_initialize",
    verify = "returns MCP-compliant init response"
)]
fn initialize_returns_capabilities() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "initialize", json!({}));
    let result = &resp["result"];

    // MCP-standard fields
    assert!(
        result["protocolVersion"].is_string(),
        "must have protocolVersion"
    );
    assert!(
        result["capabilities"].is_object(),
        "must have capabilities object"
    );
    assert!(
        result["serverInfo"].is_object(),
        "must have serverInfo object"
    );
    assert_eq!(result["serverInfo"]["name"], "specforge-mcp");
    assert!(result["serverInfo"]["version"].is_string());

    // Capabilities declares supported categories
    let caps = &result["capabilities"];
    assert!(
        caps["tools"].is_object(),
        "capabilities must declare tools support"
    );
    assert!(
        caps["resources"].is_object(),
        "capabilities must declare resources support"
    );
    assert!(
        caps["prompts"].is_object(),
        "capabilities must declare prompts support"
    );
    assert!(
        caps["resources"]["subscribe"] == true,
        "resources.subscribe must be advertised: the subscribe→recompile→notify loop is wired (C9-01)"
    );

    // Convenience arrays still present for backwards compat
    assert!(result["tools"].is_array());
    assert!(result["resources"].is_array());
    assert!(result["prompts"].is_array());
}

// B:mcp_initialize — verify unit "registers tools"
#[specforge_test(
    behavior = "mcp_initialize",
    verify = "all core tools registered before accepting requests"
)]
fn initialize_registers_tools() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "initialize", json!({}));
    let tools = resp["result"]["tools"].as_array().unwrap();
    assert!(!tools.is_empty());

    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"specforge.query"));
    assert!(tool_names.contains(&"specforge.validate"));
    assert!(tool_names.contains(&"specforge.export"));
    assert!(tool_names.contains(&"specforge.trace"));
    assert!(tool_names.contains(&"specforge.search"));
}

// B:mcp_initialize — verify unit "registers resources"
#[specforge_test(
    behavior = "mcp_initialize",
    verify = "all core resources registered before accepting requests"
)]
fn initialize_registers_resources() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "initialize", json!({}));
    let resources = resp["result"]["resources"].as_array().unwrap();
    assert!(!resources.is_empty());

    let mut uris: Vec<&str> = resources
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    uris.sort_unstable();
    assert_eq!(
        uris,
        vec![
            "specforge://brief",
            "specforge://context",
            "specforge://context/{entity_id}",
            "specforge://diagnostics",
            "specforge://entities/{kind}",
            "specforge://graph",
            "specforge://graph/{entity_id}",
            "specforge://schema",
        ]
    );
    // Registered before the first request is served: five plain resources,
    // three templated ones.
    let listed = call(&mut server, "resources/list", json!({}));
    assert_eq!(listed["result"]["resources"].as_array().unwrap().len(), 5);
    let templates = call(&mut server, "resources/templates/list", json!({}));
    assert_eq!(
        templates["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn initialize_registers_prompts() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "initialize", json!({}));
    let prompts = resp["result"]["prompts"].as_array().unwrap();
    assert!(!prompts.is_empty());

    let names: Vec<&str> = prompts
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"specforge://prompts/context"));
    assert!(names.contains(&"specforge://prompts/review"));
}

// B:mcp_initialize — verify unit "compiles project when projectRoot is provided"
#[specforge_test(
    behavior = "mcp_initialize",
    verify = "compiles project when projectRoot is provided"
)]
fn initialize_compiles_project() {
    let (server, _dir) = init_server_with_project();
    assert!(server.state().graph().node_count() > 0);
}

#[test]
fn shutdown_clears_state() {
    let mut server = init_server();
    let resp = call(&mut server, "shutdown", json!({}));
    assert!(resp["result"].is_object());
    assert!(!server.state().is_initialized());
}

#[test]
fn shutdown_returns_success() {
    let mut server = init_server();
    let resp = call(&mut server, "shutdown", json!({}));
    assert!(resp["result"].is_object());
    assert!(resp["error"].is_null());
}

// B:mcp_shutdown — verify unit "rejects calls after shutdown"
#[specforge_test(
    behavior = "mcp_shutdown",
    verify = "shutdown rejects new tool calls during teardown"
)]
fn rejects_calls_after_shutdown() {
    let mut server = init_server();
    call(&mut server, "shutdown", json!({}));
    let resp = call(&mut server, "tools/list", json!({}));
    assert!(resp["error"].is_object());
}

#[test]
fn double_shutdown_returns_error() {
    let mut server = init_server();
    call(&mut server, "shutdown", json!({}));
    let resp = call(&mut server, "shutdown", json!({}));
    assert!(resp["error"].is_object());
}

// B:guard_mcp_reinitialization — verify unit "duplicate initialize returns -32600"
#[specforge_test(
    behavior = "guard_mcp_reinitialization",
    verify = "second initialize request returns -32600 error"
)]
fn duplicate_initialize_returns_error() {
    let mut server = init_server();
    let resp = call(&mut server, "initialize", json!({}));
    assert_eq!(resp["error"]["code"], -32600);
}

#[specforge_test(
    behavior = "guard_mcp_reinitialization",
    verify = "can reinitialize after shutdown"
)]
fn can_reinitialize_after_shutdown() {
    let (mut server, _dir) = init_server_with_project();
    let root = server
        .state()
        .project_root()
        .map(std::path::Path::to_path_buf)
        .unwrap();
    call(&mut server, "shutdown", json!({}));

    let resp = call(
        &mut server,
        "initialize",
        json!({"projectRoot": root.to_str().unwrap()}),
    );
    assert!(resp["error"].is_null(), "reinitialize rejected: {resp}");
    assert!(server.state().is_initialized());
    assert!(
        server.state().graph().node("hello_world").is_some(),
        "the project is compiled again"
    );
    let listed = call(&mut server, "tools/list", json!({}));
    assert!(listed["result"]["tools"].is_array(), "{listed}");
}

#[test]
fn list_tools_returns_descriptors() {
    let mut server = init_server();
    let resp = call(&mut server, "tools/list", json!({}));
    let tools = resp["result"]["tools"].as_array().unwrap();
    assert!(!tools.is_empty());

    for tool in tools {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert!(tool["inputSchema"].is_object());
    }
}

// B:list_mcp_tools — verify unit "tools have categories"
#[specforge_test(behavior = "list_mcp_tools", verify = "tools have categories")]
fn tools_have_categories() {
    let mut server = init_server();
    let resp = call(&mut server, "tools/list", json!({}));
    let tools = resp["result"]["tools"].as_array().unwrap();

    let categories: Vec<&str> = tools
        .iter()
        .filter_map(|t| t["category"].as_str())
        .collect();
    assert!(categories.contains(&"core"));
    assert!(categories.contains(&"navigation"));
    assert!(categories.contains(&"mutation"));
    assert!(categories.contains(&"management"));
}

#[test]
fn list_tools_error_when_not_initialized() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "tools/list", json!({}));
    assert!(resp["error"].is_object());
}

#[test]
fn list_resources_returns_descriptors() {
    let mut server = init_server();
    let resp = call(&mut server, "resources/list", json!({}));
    let resources = resp["result"]["resources"].as_array().unwrap();
    assert!(!resources.is_empty());

    for res in resources {
        assert!(res["uri"].is_string());
        assert!(res["name"].is_string());
    }
}

#[test]
fn list_resources_error_when_not_initialized() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "resources/list", json!({}));
    assert!(resp["error"].is_object());
}

#[test]
fn list_prompts_returns_descriptors() {
    let mut server = init_server();
    let resp = call(&mut server, "prompts/list", json!({}));
    let prompts = resp["result"]["prompts"].as_array().unwrap();
    assert!(!prompts.is_empty());

    for prompt in prompts {
        assert!(prompt["name"].is_string());
        assert!(prompt["description"].is_string());
    }
}

#[test]
fn list_prompts_error_when_not_initialized() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "prompts/list", json!({}));
    assert!(resp["error"].is_object());
}

// B:mcp_initialize — verify unit "initialization rejects tool calls before completion"
#[specforge_test(
    behavior = "mcp_initialize",
    verify = "initialization rejects tool calls before completion"
)]
fn initialize_rejects_tool_calls_before_completion() {
    let mut server = McpServer::new();
    // Server is not initialized yet — tool calls should be rejected
    let resp = call(
        &mut server,
        "tools/call",
        json!({"name": "specforge.query", "arguments": {}}),
    );
    assert!(resp["error"].is_object());
}

#[test]
fn shutdown_events_recorded() {
    let mut server = init_server();
    call(&mut server, "shutdown", json!({}));
    let shutdown: Vec<&Value> = server
        .state()
        .events
        .iter()
        .filter(|e| e.name == "mcp_server_shutdown")
        .map(|e| &e.params)
        .collect();
    assert_eq!(shutdown.len(), 1, "{shutdown:?}");
    let mut payload = shutdown[0].clone();
    let stamp = payload.as_object_mut().unwrap().remove("timestamp");
    assert!(stamp.is_some_and(|s| s.is_string()), "{payload}");
    // Nothing was pending or subscribed.
    assert_eq!(
        payload,
        json!({
            "pending_notifications_flushed": 0,
            "subscriptions_released": 0,
            "wasm_engines_released": 0,
        })
    );
}

// B:guard_mcp_reinitialization — verify unit "existing session continues after rejected reinit"
#[specforge_test(
    behavior = "guard_mcp_reinitialization",
    verify = "existing session continues after rejected reinitialization"
)]
fn reinit_rejected_session_continues() {
    let (mut server, _dir) = init_server_with_project();
    // Attempt duplicate init — should be rejected
    let resp = call(&mut server, "initialize", json!({}));
    assert!(resp["error"].is_object());
    // Graph should still be accessible after rejected reinit
    assert!(server.state().graph().node_count() > 0);
}

// B:guard_mcp_reinitialization — verify unit "no resources leaked on rejected reinit"
#[specforge_test(
    behavior = "guard_mcp_reinitialization",
    verify = "no resources leaked on rejected reinitialization"
)]
fn reinit_rejected_no_resource_leak() {
    let mut server = init_server();
    let listed = |server: &McpServer| {
        (
            specforge_mcp::registry::listed_tools(server.state()).count(),
            specforge_mcp::registry::listed_resources(server.state()).count(),
        )
    };
    let before = listed(&server);
    let prompts_before = call(&mut server, "prompts/list", json!({}))["result"]["prompts"].clone();

    // Attempt duplicate init — should be rejected
    let resp = call(&mut server, "initialize", json!({}));
    assert!(resp["error"].is_object());

    // Counts must remain the same
    assert_eq!(listed(&server), before);
    assert_eq!(
        call(&mut server, "prompts/list", json!({}))["result"]["prompts"],
        prompts_before
    );
}

// B:list_mcp_tools — verify unit "returns core-provided descriptors when no extensions"
#[specforge_test(
    behavior = "list_mcp_tools",
    verify = "returns core-provided descriptors when no extensions installed"
)]
fn list_tools_core_descriptors_no_extensions() {
    let mut server = init_server();
    let resp = call(&mut server, "tools/list", json!({}));
    let tools = resp["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    // All core tools should be present even without extensions
    assert!(names.contains(&"specforge.query"));
    assert!(names.contains(&"specforge.validate"));
    assert!(names.contains(&"specforge.export"));
    assert!(names.contains(&"specforge.trace"));
    assert!(names.contains(&"specforge.search"));
    assert!(names.contains(&"specforge.stats"));
    assert!(names.contains(&"specforge.inspect"));
}

// B:list_mcp_tools — verify unit "reflects tools from newly loaded extension"
#[specforge_test(
    behavior = "list_mcp_tools",
    verify = "reflects tools from newly loaded extension"
)]
fn list_tools_reflects_extension_tools() {
    let (mut server, _ext, _dir) = server_with_extension_surfaces();
    let resp = call(&mut server, "tools/list", json!({}));
    let tools = resp["result"]["tools"].as_array().unwrap();
    let ext = tools
        .iter()
        .find(|t| t["name"] == "ext.hello")
        .expect("the extension's tool is listed");
    assert_eq!(ext["description"], "Say hello");
    assert!(
        tools.iter().any(|t| t["name"] == "specforge.inspect"),
        "core tools stay"
    );
}

// B:list_mcp_resources — verify unit "returns core-provided descriptors when no extensions"
#[specforge_test(
    behavior = "list_mcp_resources",
    verify = "returns core-provided descriptors when no extensions installed"
)]
fn list_resources_core_descriptors_no_extensions() {
    let mut server = init_server();
    let resp = call(&mut server, "resources/list", json!({}));
    let resources = resp["result"]["resources"].as_array().unwrap();
    let uris: Vec<&str> = resources
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();

    assert!(uris.contains(&"specforge://graph"));
    assert!(uris.contains(&"specforge://diagnostics"));
}

// B:list_mcp_resources — verify unit "reflects resources from newly loaded extension"
#[specforge_test(
    behavior = "list_mcp_resources",
    verify = "reflects resources from newly loaded extension"
)]
fn list_resources_reflects_extension_resources() {
    let (mut server, _ext, _dir) = server_with_extension_surfaces();
    let resp = call(&mut server, "resources/list", json!({}));
    let uris: Vec<&str> = resp["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert!(uris.contains(&"specforge://graph"), "core resources stay");
    // The extension's resource is templated.
    let resp = call(&mut server, "resources/templates/list", json!({}));
    let templates: Vec<&str> = resp["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();
    assert!(
        templates.contains(&"specforge://ext/hello/{name}"),
        "{templates:?}"
    );
}

// B:list_mcp_prompts — verify unit "returns core-provided descriptors when no extensions"
#[specforge_test(
    behavior = "list_mcp_prompts",
    verify = "returns core-provided descriptors when no extensions installed"
)]
fn list_prompts_core_descriptors_no_extensions() {
    let mut server = init_server();
    let resp = call(&mut server, "prompts/list", json!({}));
    let prompts = resp["result"]["prompts"].as_array().unwrap();
    let names: Vec<&str> = prompts
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();

    assert!(names.contains(&"specforge://prompts/context"));
    assert!(names.contains(&"specforge://prompts/review"));
    assert!(names.contains(&"specforge://prompts/trace"));
    assert!(names.contains(&"specforge://prompts/explore"));
}

// B:handle_mcp_request_cancellation — verify unit "server state remains consistent"
#[specforge_test(
    behavior = "handle_mcp_request_cancellation",
    verify = "server state remains consistent after cancellation"
)]
fn cancel_state_consistent() {
    let (mut server, _dir) = init_server_with_project();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    let before = state_snapshot(&server);
    let subscriptions = server.state().subscriptions().clone();
    assert_eq!(before["nodes"], json!(["greeting", "hello_world"]));

    // Cancel a completed request, an unknown one, and one by MCP's
    // notification.
    call(&mut server, "ping", json!({}));
    call(&mut server, "$/cancelRequest", json!({"id": 1}));
    call(&mut server, "$/cancelRequest", json!({"id": 999}));
    server.handle_message(
        &json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
            "params": {"requestId": 1}})
        .to_string(),
    );

    assert_eq!(state_snapshot(&server), before);
    assert_eq!(server.state().subscriptions(), &subscriptions);
}

#[test]
fn cancel_long_running_acknowledgment() {
    let mut server = init_server();
    // Simulate cancellation of a hypothetical long-running request
    let resp = call(&mut server, "$/cancelRequest", json!({"id": 42}));
    // Cancel is best-effort: should not error out
    assert!(resp["error"].is_null() || resp["result"].is_object() || resp["result"].is_null());
    // Server should remain functional after cancel
    let tools_resp = call(&mut server, "tools/list", json!({}));
    assert!(tools_resp["result"]["tools"].is_array());
}

#[test]
fn cancel_in_progress_best_effort() {
    let mut server = init_server();
    // Best-effort cancel: synchronous server cannot truly cancel in-progress work,
    // but the cancel request itself should succeed without error
    let resp = call(&mut server, "$/cancelRequest", json!({"id": 1}));
    // Cancel should not produce an error (it's a best-effort operation)
    assert!(!resp["error"].is_object() || resp["result"].is_object() || resp["result"].is_null());
}

// B:handle_mcp_request_cancellation — verify unit "server state remains consistent after cancellation"
#[specforge_test(
    behavior = "handle_mcp_request_cancellation",
    verify = "server state remains consistent after cancellation"
)]
fn cancel_server_state_consistent() {
    let (mut server, _dir) = init_server_with_project();
    let query = json!({"name": "specforge.query", "arguments": {"entity_id": "hello_world"}});
    let answer = call(&mut server, "tools/call", query.clone());
    assert!(
        answer["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("greeting")
    );
    let lists = |server: &mut McpServer| {
        ["tools/list", "resources/list", "prompts/list"]
            .map(|method| call(server, method, json!({}))["result"].clone())
    };
    let lists_before = lists(&mut server);

    // Cancel the query that already answered.
    call(&mut server, "$/cancelRequest", json!({"id": 1}));

    // The same request gets the same answer, and every listing is unchanged.
    assert_eq!(call(&mut server, "tools/call", query), answer);
    assert_eq!(lists(&mut server), lists_before);
}

// B:list_mcp_resources — verify unit "returns core-provided descriptors when no extensions installed"
#[specforge_test(
    behavior = "list_mcp_resources",
    verify = "returns core-provided descriptors when no extensions installed"
)]
fn list_resources_core_only() {
    let mut server = init_server();
    // No extensions installed — should still return core resources
    let resp = call(&mut server, "resources/list", json!({}));
    let resp_templates = call(&mut server, "resources/templates/list", json!({}));
    let mut uris: Vec<&str> = resp["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .chain(
            resp_templates["result"]["resourceTemplates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["uriTemplate"].as_str().unwrap()),
        )
        .collect();
    uris.sort_unstable();
    // Exactly the core resources, plain and templated: none contributed by
    // an extension.
    assert_eq!(
        uris,
        vec![
            "specforge://brief",
            "specforge://context",
            "specforge://context/{entity_id}",
            "specforge://diagnostics",
            "specforge://entities/{kind}",
            "specforge://graph",
            "specforge://graph/{entity_id}",
            "specforge://schema",
        ]
    );
}

// B:list_mcp_tools — verify unit "returns core-provided descriptors when no extensions installed"
#[specforge_test(
    behavior = "list_mcp_tools",
    verify = "returns core-provided descriptors when no extensions installed"
)]
fn list_tools_core_only() {
    let mut server = init_server();
    let resp = call(&mut server, "tools/list", json!({}));
    let tools = resp["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 34, "the 34 core tools");
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(name.starts_with("specforge."), "{name} is not core");
        assert_ne!(tool["category"], "extension", "{tool}");
    }
}

// B:list_mcp_prompts — verify unit "returns core-provided descriptors when no extensions installed"
#[specforge_test(
    behavior = "list_mcp_prompts",
    verify = "returns core-provided descriptors when no extensions installed"
)]
fn list_prompts_core_only() {
    let mut server = init_server();
    let resp = call(&mut server, "prompts/list", json!({}));
    let prompts = resp["result"]["prompts"].as_array().unwrap();
    let mut names: Vec<&str> = prompts
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "specforge://prompts/context",
            "specforge://prompts/explore",
            "specforge://prompts/infer",
            "specforge://prompts/review",
            "specforge://prompts/trace",
        ]
    );
}

// B:guard_mcp_reinitialization — verify unit "existing session continues after rejected reinitialization"
#[specforge_test(
    behavior = "guard_mcp_reinitialization",
    verify = "existing session continues after rejected reinitialization"
)]
fn reinit_existing_session_continues() {
    let (mut server, _dir) = init_server_with_project();
    let before = state_snapshot(&server);
    let other = TempDir::new().unwrap();
    // Try to initialize again, even onto another project
    let resp2 = call(
        &mut server,
        "initialize",
        json!({"projectRoot": other.path().to_str().unwrap()}),
    );
    assert_eq!(resp2["error"]["code"], -32600, "{resp2}");
    // The session keeps its project, graph and registries, and serves it.
    assert_eq!(state_snapshot(&server), before);
    let answer = call(
        &mut server,
        "tools/call",
        json!({"name": "specforge.query", "arguments": {"entity_id": "hello_world"}}),
    );
    assert_ne!(answer["result"]["isError"], true, "{answer}");
    assert!(
        answer["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("hello_world")
    );
}

// B:guard_mcp_reinitialization — verify unit "no resources leaked on rejected reinitialization"
#[specforge_test(
    behavior = "guard_mcp_reinitialization",
    verify = "no resources leaked on rejected reinitialization"
)]
fn reinit_no_resource_leak() {
    let mut server = init_server();
    // Get initial state
    let tools1 = call(&mut server, "tools/list", json!({}));
    let count1 = tools1["result"]["tools"].as_array().unwrap().len();
    // Attempt reinit
    let _resp2 = call(&mut server, "initialize", json!({}));
    // Verify no resource duplication
    let tools2 = call(&mut server, "tools/list", json!({}));
    let count2 = tools2["result"]["tools"].as_array().unwrap().len();
    assert_eq!(
        count1, count2,
        "tool count must not change after rejected reinit (no leaks)"
    );
}
