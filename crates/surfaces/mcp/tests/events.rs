use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

/// The params of the one `event_name` event.
fn only_event(server: &impl Logs, event_name: &str) -> Value {
    let params = events(server, event_name);
    assert_eq!(params.len(), 1, "{event_name}: {params:?}");
    params.into_iter().next().unwrap()
}

/// A server initialized over a project holding the behavior `alpha` and
/// the feature `beta` that has it, `@test/ext` declaring their kinds.
fn init_server() -> Served {
    TestProject::new()
        .file(
            "test.spec",
            "behavior alpha \"Alpha\" {\n    features [beta]\n}\nfeature beta \"Beta\" {\n}\n",
        )
        .serve(&[TestExtension::software()])
}

// E:mcp_initialized — verify integration "mcp initialization emits event with tool counts"
#[specforge_test(
    behavior = "mcp_initialized",
    verify = "mcp initialization emits event with tool counts"
)]
fn event_mcp_initialized() {
    let mut server = init_server();
    // The core surface: 34 tools, 8 resources, 5 prompts; the project's one
    // extension, @test/ext, declares kinds and contributes no surface.
    assert_eq!(
        only_event(&server, "mcp_initialized"),
        json!({
            "tools_registered": 34,
            "resources_registered": 8,
            "prompts_registered": 5,
            "extensions_loaded": 1,
            "surface_tools_registered": 0,
            "surface_resources_registered": 0,
            "auto_promoted_tools": 0,
        })
    );
    let listed = |server: &mut McpServer, method: &str, key: &str| {
        call(server, method, json!({}))["result"][key]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(listed(&mut server, "tools/list", "tools"), 34);
    // Eight resources: five plain, three templated.
    assert_eq!(listed(&mut server, "resources/list", "resources"), 5);
    assert_eq!(
        listed(&mut server, "resources/templates/list", "resourceTemplates"),
        3
    );
    assert_eq!(listed(&mut server, "prompts/list", "prompts"), 5);
}

// E:mcp_server_shutdown — verify integration "emits mcp_server_shutdown with correct counts when MCP server shuts down"
#[specforge_test(
    behavior = "mcp_server_shutdown",
    verify = "emits mcp_server_shutdown with correct counts when MCP server shuts down"
)]
fn event_mcp_server_shutdown() {
    let mut server = init_server();
    // Two subscriptions and two notifications waiting to be sent: a new
    // behavior changes the graph (the resource's update, then the delta),
    // not the diagnostics.
    for uri in ["specforge://graph", "specforge://diagnostics"] {
        call(&mut server, "resources/subscribe", json!({"uri": uri}));
    }
    server.write("more.spec", "behavior gamma \"Gamma\" {\n}\n");
    call_tool(&mut server, "specforge.stats", json!({}));
    call(&mut server, "shutdown", json!({}));
    // The served project's runtime goes with its session.
    assert_eq!(
        only_event(&server, "mcp_server_shutdown"),
        json!({
            "pending_notifications_flushed": 2,
            "subscriptions_released": 2,
            "wasm_engines_released": 1,
        })
    );
}

// E:mcp_initialization_failed — verify integration "emits mcp_initialization_failed when MCP server fails to initialize"
#[specforge_test(
    behavior = "mcp_initialization_failed",
    verify = "emits mcp_initialization_failed when MCP server fails to initialize"
)]
fn event_mcp_initialization_failed() {
    let mut server = init_server();
    call(&mut server, "initialize", json!({}));
    assert_eq!(
        only_event(&server, "mcp_initialization_failed"),
        json!({
            "error_kind": "already_initialized",
            "message": "Server already initialized",
        })
    );
}

// E:mcp_protocol_error_handled — verify integration "emits mcp_protocol_error_handled with correct errorCode for each error type"
#[specforge_test(
    behavior = "mcp_protocol_error_handled",
    verify = "emits mcp_protocol_error_handled with correct errorCode for each error type"
)]
fn event_mcp_protocol_error_handled() {
    let mut server = logged(McpServer::new());
    server.handle_message("not valid json");
    server.handle_message(r#"{"id": 2}"#);
    call(&mut server, "initialize", json!({}));
    call(&mut server, "no/such/method", json!({}));
    call(&mut server, "tools/call", json!({}));
    let errors: Vec<(i64, String)> = events(&server, "mcp_protocol_error_handled")
        .iter()
        .map(|p| {
            (
                p["errorCode"].as_i64().unwrap(),
                p["errorMessage"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let codes: Vec<i64> = errors.iter().map(|(c, _)| *c).collect();
    assert_eq!(codes, vec![-32700, -32600, -32601, -32602], "{errors:?}");
    assert!(errors.iter().all(|(_, m)| !m.is_empty()), "{errors:?}");
    let events = events(&server, "mcp_protocol_error_handled");
    // A parse error names no method; a routed one names its method.
    assert_eq!(
        events[0],
        json!({"errorCode": -32700, "errorMessage": "Parse error"})
    );
    assert_eq!(
        events[3],
        json!({
            "errorCode": -32602,
            "errorMessage": "Missing required parameter: name",
            "requestMethod": "tools/call",
        })
    );
}

// E:mcp_request_cancelled — verify integration "emits mcp_request_cancelled with correct requestId and wasInProgress flag"
#[specforge_test(
    behavior = "mcp_request_cancelled",
    verify = "emits mcp_request_cancelled with correct requestId and wasInProgress flag"
)]
fn event_mcp_request_cancelled() {
    let mut server = init_server();
    call(&mut server, "ping", json!({}));
    // The MCP notification names the request by requestId.
    server.handle_message(
        &json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
            "params": {"requestId": 41}})
        .to_string(),
    );
    call(&mut server, "$/cancelRequest", json!({"id": 42}));
    let events = events(&server, "mcp_request_cancelled");
    // Requests run one at a time, so none is in progress when a cancel
    // arrives; the id is named as a string.
    assert_eq!(
        events,
        vec![
            json!({"requestId": "41", "wasInProgress": false}),
            json!({"requestId": "42", "wasInProgress": false}),
        ]
    );
}

// E:mcp_discovery_invoked — verify integration "emits mcp_discovery_invoked with correct discoveryType when agent lists tools, prompts, or resources"
#[specforge_test(
    behavior = "mcp_discovery_invoked",
    verify = "emits mcp_discovery_invoked with correct discoveryType when agent lists tools, prompts, or resources"
)]
fn event_mcp_discovery_invoked() {
    let mut server = init_server();
    call(&mut server, "tools/list", json!({}));
    call(&mut server, "prompts/list", json!({}));
    call(&mut server, "resources/list", json!({}));
    let discoveries: Vec<(String, u64)> = events(&server, "mcp_discovery_invoked")
        .iter()
        .map(|p| {
            (
                p["discoveryType"].as_str().unwrap().to_string(),
                p["resultCount"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        discoveries,
        vec![
            ("tools".to_string(), 34),
            ("prompts".to_string(), 5),
            ("resources".to_string(), 5),
        ]
    );
}

// E:mcp_resource_read — verify integration "emits mcp_resource_read with correct resourceUri when agent reads any MCP resource"
#[specforge_test(
    behavior = "mcp_resource_read",
    verify = "emits mcp_resource_read with correct resourceUri when agent reads any MCP resource"
)]
fn event_mcp_resource_read() {
    let mut server = init_server();
    call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://graph"}),
    );
    call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://diagnostics"}),
    );
    // An unknown resource is a protocol error, not a read.
    call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://nowhere"}),
    );
    assert_eq!(
        events(&server, "mcp_resource_read"),
        vec![
            json!({"resourceUri": "specforge://graph", "format": "application/json"}),
            json!({"resourceUri": "specforge://diagnostics", "format": "application/json"}),
        ]
    );
}

// E:mcp_tool_invoked — verify integration "emits mcp_tool_invoked with correct toolName, category, and parameters for any tool call"
#[specforge_test(
    behavior = "mcp_tool_invoked",
    verify = "emits mcp_tool_invoked with correct toolName, category, and parameters for any tool call"
)]
fn event_mcp_tool_invoked() {
    let mut server = init_server();
    call_tool(&mut server, "specforge.stats", json!({}));
    call_tool(
        &mut server,
        "specforge.inspect",
        json!({"entity_id": "alpha"}),
    );
    call_tool(&mut server, "specforge.format", json!({"check": true}));
    call_tool(&mut server, "specforge.doctor", json!({}));
    // infer_progress is registered as "inference": read-only, so core.
    call_tool(&mut server, "specforge.infer_progress", json!({}));
    // An unknown tool is a protocol error, not an invocation.
    call_tool(&mut server, "specforge.nothing", json!({}));
    let events = events(&server, "mcp_tool_invoked");
    assert_eq!(
        events,
        vec![
            json!({"toolName": "specforge.stats", "category": "core", "params": "{}"}),
            json!({"toolName": "specforge.inspect", "category": "navigation",
                "entityId": "alpha", "params": r#"{"entity_id":"alpha"}"#}),
            json!({"toolName": "specforge.format", "category": "mutation",
                "params": r#"{"check":true}"#}),
            json!({"toolName": "specforge.doctor", "category": "management", "params": "{}"}),
            json!({"toolName": "specforge.infer_progress", "category": "core", "params": "{}"}),
        ]
    );
}

// E:mcp_prompt_invoked — verify integration "emits mcp_prompt_invoked with correct promptName and arguments"
#[specforge_test(
    behavior = "mcp_prompt_invoked",
    verify = "emits mcp_prompt_invoked with correct promptName and arguments"
)]
fn event_mcp_prompt_invoked() {
    // The project declares the behavior `alpha`.
    let mut server = init_server();

    call(
        &mut server,
        "prompts/get",
        json!({"name": "specforge://prompts/context", "arguments": {"entity_id": "alpha"}}),
    );
    assert_eq!(
        only_event(&server, "mcp_prompt_invoked"),
        json!({"promptName": "specforge://prompts/context", "entityId": "alpha"})
    );
    call(
        &mut server,
        "prompts/get",
        json!({"name": "specforge://prompts/explore", "arguments": {"kind": "behavior"}}),
    );
    assert_eq!(
        events(&server, "mcp_prompt_invoked")[1],
        json!({"promptName": "specforge://prompts/explore", "kind": "behavior"})
    );
}

// E:mcp_delta_notified — verify integration "emits mcp_delta_notified with correct notification type and delta summary"
#[specforge_test(
    behavior = "mcp_delta_notified",
    verify = "emits mcp_delta_notified with correct notification type and delta summary"
)]
fn event_mcp_delta_notified() {
    // A project on disk, served; then a file written beside its own: the
    // routed read brings the project up to date and notifies.
    let mut server = init_server();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    server.write(
        "more.spec",
        "behavior gamma \"Gamma\" {\n}\nbehavior delta \"Delta\" {\n}\n",
    );

    call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://diagnostics"}),
    );

    let notifications = server.take_notifications();
    assert_eq!(
        notifications.len(),
        2,
        "subscribed client must receive the graph delta: {notifications:?}"
    );
    assert_eq!(notifications[1]["method"], "specforge/graphChanged");
    // gamma and delta were added.
    assert_eq!(
        only_event(&server, "mcp_delta_notified"),
        json!({"notificationType": "graph", "subscriberCount": 1,
            "addedNodes": 2, "removedNodes": 0, "modifiedNodes": 0})
    );
}

// E:mcp_mutation_completed — verify integration "emits mcp_mutation_completed with structured outcome after each mutation tool"
#[specforge_test(
    behavior = "mcp_mutation_completed",
    verify = "emits mcp_mutation_completed with structured outcome after each mutation tool"
)]
fn event_mcp_mutation_completed() {
    let mut server = init_server();
    // Two unformatted files: the formatter rewrites both.
    server.write("a.spec", "behavior a   \"A\" {\n}\n");
    server.write("b.spec", "behavior b   \"B\" {\n}\n");
    let resp = call_tool(&mut server, "specforge.format", json!({}));
    let result: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let changed = result["changed_files"].as_array().unwrap().len();
    assert!(changed >= 2, "{result}");
    assert_eq!(
        only_event(&server, "mcp_mutation_completed"),
        json!({
            "toolName": "specforge.format",
            "files_changed": changed,
            "entities_affected": 0,
            "success": true,
        })
    );

    // A read-only tool completes no mutation.
    call_tool(&mut server, "specforge.stats", json!({}));
    assert_eq!(events(&server, "mcp_mutation_completed").len(), 1);
}

// E:mcp_subscription_created — verify integration "emits mcp_subscription_created for each resource a client subscribes to or a listen stream names"
#[specforge_test(
    behavior = "mcp_subscription_created",
    verify = "emits mcp_subscription_created for each resource a client subscribes to or a listen stream names"
)]
fn event_mcp_subscription_created() {
    let mut server = init_server();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert_eq!(
        only_event(&server, "mcp_subscription_created"),
        json!({"resourceUri": "specforge://graph"})
    );
}

// E:mcp_subscription_removed — verify integration "emits mcp_subscription_removed for each resource whose subscription ends: unsubscribe, cancel, disconnect or shutdown"
#[specforge_test(
    behavior = "mcp_subscription_removed",
    verify = "emits mcp_subscription_removed for each resource whose subscription ends: unsubscribe, cancel, disconnect or shutdown"
)]
fn event_mcp_subscription_removed() {
    let mut server = init_server();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    call(
        &mut server,
        "resources/unsubscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert_eq!(
        only_event(&server, "mcp_subscription_removed"),
        json!({"resourceUri": "specforge://graph"})
    );
}

/// A session touching every MCP event: initialize (twice), subscribe,
/// the listings, a read, a tool, a prompt, a rename (a mutation and a
/// graph delta), a protocol error, a cancel and shutdown. The project's
/// directory lives as long as the returned value.
fn core_session() -> (Logged, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let config = json!({"name":"t","version":"0.1.0","extensions":[]});
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    std::fs::write(
        dir.path().join("test.spec"),
        "behavior alpha \"Alpha\" {\n    features [beta]\n}\nfeature beta \"Beta\" {\n}\n",
    )
    .unwrap();

    let mut server = logged(McpServer::new());
    call(
        &mut server,
        "initialize",
        json!({"projectRoot": dir.path().to_str().unwrap()}),
    );
    call(&mut server, "initialize", json!({}));
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    call(&mut server, "tools/list", json!({}));
    call(&mut server, "resources/list", json!({}));
    call(&mut server, "prompts/list", json!({}));
    call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://graph"}),
    );
    call_tool(
        &mut server,
        "specforge.inspect",
        json!({"entity_id": "alpha"}),
    );
    call(
        &mut server,
        "prompts/get",
        json!({"name": "specforge://prompts/context", "arguments": {"entity_id": "alpha"}}),
    );
    // A rename writes the file and recompiles: a mutation and a graph delta.
    call_tool(
        &mut server,
        "specforge.rename",
        json!({"entity_id": "alpha", "new_name": "gamma"}),
    );
    call(&mut server, "no/such/method", json!({}));
    call(&mut server, "$/cancelRequest", json!({"id": 7}));
    call(&mut server, "shutdown", json!({}));
    (server, dir)
}

/// The core session records every event `spec/events/mcp.spec` declares,
/// and each recorded event serializes to the shape `McpEvent` derives.
#[specforge_test(
    behavior = "mcp_event_record",
    verify = "every event the server records is one an events spec declares, its payload exactly the declared fields"
)]
fn a_session_records_every_mcp_event() {
    let (server, _dir) = core_session();
    let events = recorded(&server);
    let emitted: std::collections::BTreeSet<&str> =
        events.iter().map(|(name, _)| name.as_str()).collect();
    let spec =
        std::fs::read_to_string(specforge_repo_root::repo_root().join("spec/events/mcp.spec"))
            .unwrap();
    let declared: std::collections::BTreeSet<&str> = spec
        .lines()
        .filter_map(|line| line.strip_prefix("event "))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect();
    assert_eq!(declared.len(), 13, "{declared:?}");
    assert_eq!(emitted, declared, "the session emits every MCP event");

    let document = specforge_common::shape::document::<specforge_mcp::events::McpEvent>();
    for recorded in server.log().recorded() {
        let value = serde_json::to_value(&recorded.event).unwrap();
        let violations = specforge_common::shape::violations(document, &value);
        assert!(violations.is_empty(), "{}: {violations:?}", recorded.name());
    }
}

/// Every event `server` recorded, oldest first: its name and payload.
fn recorded(server: &impl Logs) -> Vec<(String, Value)> {
    server
        .log()
        .recorded()
        .iter()
        .map(|recorded| (recorded.name(), recorded.payload()))
        .collect()
}

/// Each recorded `(name, payload)` as `{"event": name, "payload": payload}`,
/// in order, its `timestamp` removed and every `durationMs` read as
/// `"[ms]"`: what the snapshot pins.
fn sequence(recorded: impl IntoIterator<Item = (String, Value)>) -> Vec<Value> {
    recorded
        .into_iter()
        .map(|(name, mut payload)| {
            if let Some(object) = payload.as_object_mut() {
                object.remove("timestamp");
                if object.contains_key("durationMs") {
                    object.insert("durationMs".into(), json!("[ms]"));
                }
            }
            json!({"event": name, "payload": payload})
        })
        .collect()
}

/// The events two sessions record, in order: the core session above, and
/// `@test/cmds`'s three surfaces dispatched (an explicit tool, a promoted
/// command, a resource). A characterization pin: the payloads and their
/// order stay as they are while the recorder changes.
#[test]
fn the_recorded_sequence_of_a_session() {
    use crate::fake_extension::{self, FakeExtension};

    let (core, _dir) = core_session();
    let core = sequence(recorded(&core));

    let (mut surfaces, _ext, _dir) = fake_extension::initialized(
        FakeExtension::new()
            .with_output("mcp__check", json!({"checked": true}))
            .with_output(
                "cmd__report",
                json!({"exit_code": 0, "stdout": "3 of 4 covered", "stderr": ""}),
            )
            .with_output(
                "mcp__summary",
                json!({"content": "{}", "mime_type": "application/json"}),
            ),
    );
    call_tool(&mut surfaces, "specforge.cmds.check", json!({}));
    call_tool(
        &mut surfaces,
        "specforge.cmds.report",
        json!({"style": "md"}),
    );
    read_resource(&mut surfaces, "specforge://ext/cmds/summary");
    let surfaces = sequence(recorded(&surfaces));

    insta::assert_json_snapshot!(json!({"core": core, "surfaces": surfaces}));
}

#[specforge_test(
    behavior = "mcp_event_record",
    verify = "every event a recording sink keeps carries the time it was recorded"
)]
fn a_log_stamps_each_event_with_the_time_it_was_recorded() {
    let before = chrono::Utc::now();
    let mut server = init_server();
    call(&mut server, "tools/list", json!({}));
    call_tool(&mut server, "specforge.stats", json!({}));
    call(&mut server, "ping", json!({}));
    let after = chrono::Utc::now();

    let recorded = server.log().recorded();
    assert!(recorded.len() >= 3, "{recorded:?}");
    for event in &recorded {
        assert!(
            before <= event.timestamp && event.timestamp <= after,
            "{event:?} was not recorded during the session"
        );
        let payload = event.payload();
        let stamp = payload["timestamp"].as_str().unwrap_or_default();
        assert!(
            chrono::DateTime::parse_from_rfc3339(stamp).is_ok(),
            "{}: {payload}",
            event.name()
        );
    }
    assert!(
        recorded
            .windows(2)
            .all(|pair| pair[0].timestamp <= pair[1].timestamp),
        "the stamps go forward: {recorded:?}"
    );
}
