use crate::support::*;
use serde_json::json;
use specforge_common::{Diagnostic, Severity, SourceSpan};
use specforge_mcp::McpServer;
use specforge_mcp::events::testing::EventLog;
use specforge_mcp::subscriptions::{Changes, Subscriptions, Watched};
use specforge_project::{GraphDelta, NodeChange};
use specforge_test::prelude::*;
use std::fs;
use tempfile::TempDir;

fn init_server() -> Logged {
    let mut server = logged(McpServer::new());
    let req = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
    server.handle_message(&req.to_string());
    server
}

// B:mcp_subscription_cleanup — verify unit "subscribe adds subscription"
#[specforge_test(
    behavior = "notify_graph_delta_via_mcp",
    verify = "clients can subscribe and unsubscribe from delta notifications"
)]
fn subscribe_adds_subscription() {
    let mut server = init_server();
    let reply = call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert_eq!(reply["result"], json!({}), "{reply}");

    assert_eq!(
        server.state().subscriptions().subscribed(),
        ["specforge://graph"]
    );
}

#[test]
fn subscribing_twice_records_one_subscription() {
    let mut server = init_server();
    for _ in 0..2 {
        let reply = call(
            &mut server,
            "resources/subscribe",
            json!({"uri": "specforge://graph"}),
        );
        assert_eq!(reply["result"], json!({}), "{reply}");
    }
    assert_eq!(events(&server, "mcp_subscription_created").len(), 1);
}

// B:mcp_subscription_cleanup — verify unit "unsubscribe removes subscription"
#[specforge_test(
    behavior = "notify_graph_delta_via_mcp",
    verify = "clients can subscribe and unsubscribe from delta notifications"
)]
fn unsubscribe_removes_subscription() {
    let mut server = init_server();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    let reply = call(
        &mut server,
        "resources/unsubscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert_eq!(reply["result"], json!({}), "{reply}");

    assert!(server.state().subscriptions().subscribed().is_empty());
}

// B:mcp_subscription_cleanup — verify unit "shutdown clears all subscriptions"
#[specforge_test(
    behavior = "mcp_shutdown",
    verify = "shutdown unsubscribes all active subscriptions"
)]
fn shutdown_clears_subscriptions() {
    let mut server = init_server();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );

    let req = json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":{}});
    server.handle_message(&req.to_string());

    assert!(server.state().subscriptions().is_empty());
    assert!(
        events(&server, "mcp_subscription_removed")
            .iter()
            .any(|payload| payload["resourceUri"] == "specforge://graph"),
        "shutdown emits mcp_subscription_removed"
    );
}

// ---- C9-01: subscribe → recompile → notification loop ----

/// Real mini project so the routed refresh path performs an honest recompile.
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    let spec_dir = dir.path().join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
    fs::write(
        spec_dir.join("base.spec"),
        "behavior base \"Base\" {\n    contract \"The system MUST exist\"\n}\n",
    )
    .unwrap();
    dir
}

fn init_with_project(root: &TempDir) -> Logged {
    let mut server = logged(McpServer::new());
    call(
        &mut server,
        "initialize",
        json!({"projectRoot": root.path().to_str().unwrap()}),
    );
    server
}

/// New content on disk: an extra entity, written while the server runs
/// (the next request that reads the project sees it).
fn evolve_project(root: &TempDir, entity: &str) {
    fs::write(
        root.path().join("spec").join(format!("{}.spec", entity)),
        format!(
            "feature {entity} \"{}\" {{\n    behaviors [base]\n}}\n",
            entity
        ),
    )
    .unwrap();
}

/// C9-01 acceptance: subscribe → recompile with changed content →
/// notification delivered to the captured client sink.
#[test]
fn subscribe_recompile_delivers_graph_notification() {
    let dir = project();
    let mut server = init_with_project(&dir);
    assert!(server.state().graph().node_count() > 0);

    let resp = call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert!(resp["result"].is_object(), "subscribe must succeed: {resp}");

    evolve_project(&dir, "fresh_added");
    // Any routed resources/read brings the project up to date.
    call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://diagnostics"}),
    );

    let notifications = server.take_notifications();
    assert_eq!(
        notifications.len(),
        2,
        "the resource's update, then the graph delta: {notifications:?}"
    );
    assert_eq!(notifications[1]["method"], "specforge/graphChanged");
    let added = notifications[1]["params"]["added_nodes"]
        .as_array()
        .unwrap();
    assert!(
        added.iter().any(|n| n["id"] == "fresh_added"),
        "delta must name the new entity: {added:?}"
    );
    assert!(!events(&server, "mcp_delta_notified").is_empty());
}

#[specforge_test(
    behavior = "mcp_shutdown",
    verify = "shutdown flushes pending notifications"
)]
fn shutdown_keeps_pending_notifications_for_the_host() {
    let dir = project();
    let mut server = init_with_project(&dir);
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    evolve_project(&dir, "fresh_added");
    call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://diagnostics"}),
    );

    call(&mut server, "shutdown", json!({}));

    let notifications = server.take_notifications();
    assert_eq!(notifications.len(), 2, "{notifications:?}");
    assert_eq!(notifications[1]["method"], "specforge/graphChanged");
}

#[specforge_test(
    behavior = "mcp_server_shutdown",
    verify = "emits mcp_server_shutdown with correct counts when MCP server shuts down"
)]
fn shutdown_event_counts_what_it_released() {
    let dir = project();
    let mut server = init_with_project(&dir);
    for uri in ["specforge://graph", "specforge://diagnostics"] {
        call(&mut server, "resources/subscribe", json!({"uri": uri}));
    }
    evolve_project(&dir, "fresh_added");
    call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://diagnostics"}),
    );
    let pending = server.state().subscriptions().pending();
    assert!(pending > 0, "the rebuild queued notifications");

    call(&mut server, "shutdown", json!({}));

    let event = events(&server, "mcp_server_shutdown")
        .into_iter()
        .next()
        .expect("mcp_server_shutdown emitted");
    assert_eq!(event["pending_notifications_flushed"], pending);
    assert_eq!(event["subscriptions_released"], 2);
    // The served project's runtime is released with its session.
    assert_eq!(event["wasm_engines_released"], 1);
}

#[specforge_test(
    behavior = "mcp_subscription_cleanup",
    verify = "client disconnect removes all subscriptions for that client"
)]
fn disconnect_ends_every_subscription_and_stream() {
    let mut server = init_server();
    for uri in ["specforge://graph", "specforge://diagnostics"] {
        call(&mut server, "resources/subscribe", json!({"uri": uri}));
    }
    listen(&mut server, json!(7), &["specforge://graph"]);

    server.disconnect();

    assert!(server.state().subscriptions().is_empty());
    assert_eq!(events(&server, "mcp_subscription_removed").len(), 3);
}

#[specforge_test(
    behavior = "mcp_subscription_cleanup",
    verify = "rapid connect/disconnect cycles leave zero subscriptions"
)]
fn rapid_connect_disconnect_cycles_leave_nothing_behind() {
    let mut server = init_server();
    for i in 0..50 {
        for uri in ["specforge://graph", "specforge://diagnostics"] {
            call(&mut server, "resources/subscribe", json!({"uri": uri}));
        }
        listen(&mut server, json!(i), &["specforge://graph"]);
        server.disconnect();
    }
    assert!(
        server.state().subscriptions().is_empty(),
        "{:?}",
        server.state().subscriptions()
    );
    assert_eq!(events(&server, "mcp_subscription_removed").len(), 150);
}

/// C9-01 contract: no subscribers → notification suppressed.
#[test]
fn recompile_without_subscribers_emits_nothing() {
    let dir = project();
    let mut server = init_with_project(&dir);

    evolve_project(&dir, "fresh_added");
    call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://diagnostics"}),
    );

    assert!(
        server.take_notifications().is_empty(),
        "no subscription means no notification"
    );
}

/// C9-01 contract: unsubscribe stops delivery.
#[test]
fn unsubscribe_stops_delivery() {
    let dir = project();
    let mut server = init_with_project(&dir);

    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    evolve_project(&dir, "fresh_one");
    call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://diagnostics"}),
    );
    assert_eq!(server.take_notifications().len(), 2);

    let resp = call(
        &mut server,
        "resources/unsubscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert!(resp["result"].is_object());

    evolve_project(&dir, "fresh_two");
    call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://diagnostics"}),
    );
    assert!(
        server.take_notifications().is_empty(),
        "unsubscribed clients must not receive notifications"
    );
}

/// Subscribing to the diagnostics resource hears the diagnostics, and only
/// them.
#[test]
fn subscribing_to_the_diagnostics_hears_only_their_changes() {
    let dir = project();
    let mut server = init_with_project(&dir);

    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://diagnostics"}),
    );
    assert_eq!(
        server.state().subscriptions().subscribed(),
        ["specforge://diagnostics"]
    );

    // A graph-only change sends nothing.
    evolve_project(&dir, "fresh_added");
    call_tool(&mut server, "specforge.stats", json!({}));
    assert!(server.take_notifications().is_empty());

    // A source that does not parse changes the diagnostics.
    fs::write(dir.path().join("spec").join("broken.spec"), "behavior {\n").unwrap();
    call_tool(&mut server, "specforge.stats", json!({}));
    let sent = server.take_notifications();
    assert_eq!(
        methods(&sent),
        [
            "notifications/resources/updated",
            "specforge/diagnosticsChanged"
        ]
    );
    assert_eq!(sent[0]["params"]["uri"], "specforge://diagnostics");
}

/// Guard: subscribe requires initialization and a uri parameter.
#[test]
fn resources_subscribe_guards() {
    let mut server = McpServer::new();
    let resp = call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert!(resp["error"].is_object(), "must reject before initialize");

    let mut server = init_server();
    let resp = call(&mut server, "resources/subscribe", json!({}));
    assert!(resp["error"].is_object(), "must reject missing uri");
}

// ---- Pins: who hears about what (plan 12, T0) ----

/// What `subscriptions` queued, as the transport writes it.
fn drained(subscriptions: &mut Subscriptions) -> Vec<serde_json::Value> {
    subscriptions
        .drain()
        .iter()
        .map(specforge_mcp::subscriptions::Notification::framed)
        .collect()
}

/// The `method` of each notification.
fn methods(sent: &[serde_json::Value]) -> Vec<String> {
    sent.iter()
        .map(|n| n["method"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// A served project holding the behavior `alpha`.
fn alpha() -> Served {
    TestProject::new()
        .file("main.spec", "behavior alpha \"Alpha\" {\n}\n")
        .serve(&[TestExtension::software()])
}

/// A new behavior on disk, brought in by a request that reads the project.
fn change(server: &mut Served) {
    server.write("beta.spec", "behavior beta \"Beta\" {\n}\n");
    call_tool(server, "specforge.stats", json!({}));
}

/// The extension names of the served `specforge://schema`.
fn schema_extensions(server: &mut McpServer) -> Vec<String> {
    let (_, schema) = resource(server, "specforge://schema");
    schema["extensions"]
        .as_array()
        .unwrap_or_else(|| panic!("no extensions in {schema}"))
        .iter()
        .map(|e| e["name"].as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "listen_for_mcp_resource_updates",
    verify = "an environment reload is heard by the schema, the graph views and the extension resources"
)]
fn an_environment_reload_is_heard_by_the_schema_and_the_graph_views() {
    let mut server = TestProject::new()
        .enabling(&["@specforge/software"])
        .file(
            "main.spec",
            "behavior alpha \"Alpha\" {\n  contract \"MUST work\"\n}\n",
        )
        .serve_components();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://schema"}),
    );
    listen(
        &mut server,
        json!(3),
        &["specforge://schema", "specforge://graph"],
    );
    assert_eq!(schema_extensions(&mut server), ["@specforge/software"]);

    server.write(
        "specforge.json",
        &json!({"name": "t", "version": "0.1.0",
            "extensions": ["@specforge/software", "@specforge/product"]})
        .to_string(),
    );
    call_tool(&mut server, "specforge.stats", json!({}));

    let stream = |uri: &str| {
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/resources/updated",
            "params": {"_meta": {"io.modelcontextprotocol/subscriptionId": 3}, "uri": uri},
        })
    };
    assert_eq!(
        server.take_notifications(),
        [
            stream("specforge://schema"),
            stream("specforge://graph"),
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/resources/updated",
                "params": {"uri": "specforge://schema"},
            }),
        ]
    );
    assert_eq!(
        schema_extensions(&mut server),
        ["@specforge/product", "@specforge/software"]
    );

    // A second request with nothing changed sends nothing.
    call_tool(&mut server, "specforge.stats", json!({}));
    assert!(server.take_notifications().is_empty());
}

#[specforge_test(
    behavior = "notify_graph_delta_via_mcp",
    verify = "unsubscribing one resource keeps the client's other subscriptions"
)]
fn unsubscribing_one_resource_keeps_the_others() {
    let mut server = alpha();
    for uri in ["specforge://graph", "specforge://context"] {
        call(&mut server, "resources/subscribe", json!({"uri": uri}));
    }
    call(
        &mut server,
        "resources/unsubscribe",
        json!({"uri": "specforge://context"}),
    );
    change(&mut server);
    let sent = server.take_notifications();
    assert_eq!(
        methods(&sent),
        ["notifications/resources/updated", "specforge/graphChanged"]
    );
    assert_eq!(sent[0]["params"]["uri"], "specforge://graph");
    assert_eq!(sent[1]["params"]["added_nodes"][0]["id"], "beta");
}

#[specforge_test(
    behavior = "notify_graph_delta_via_mcp",
    verify = "a subscribed resource hears notifications/resources/updated when a rebuild changes it"
)]
fn a_subscribed_resource_hears_notifications_resources_updated() {
    let mut server = alpha();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    change(&mut server);
    let sent = server.take_notifications();
    assert_eq!(
        sent[0],
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/resources/updated",
            "params": {"uri": "specforge://graph"},
        })
    );
    assert_eq!(methods(&sent[1..]), ["specforge/graphChanged"]);
    assert_eq!(sent.len(), 2);
}

#[specforge_test(
    behavior = "mcp_shutdown",
    verify = "shutdown ends every listen stream and records its removal"
)]
fn shutdown_ends_every_listen_stream_and_records_it() {
    let mut server = alpha();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    listen(&mut server, json!(7), &["specforge://diagnostics"]);
    call(&mut server, "shutdown", json!({}));
    assert_eq!(
        events(&server, "mcp_subscription_removed"),
        [
            json!({"resourceUri": "specforge://graph"}),
            json!({"resourceUri": "specforge://diagnostics", "subscriptionId": "7"}),
        ]
    );
    assert_eq!(
        events(&server, "mcp_server_shutdown")[0]["subscriptions_released"],
        2
    );
}

#[specforge_test(
    behavior = "listen_for_mcp_resource_updates",
    verify = "a resource a listen names twice is honoured once"
)]
fn a_resource_a_listen_names_twice_is_honoured_once() {
    let mut server = alpha();
    let ack = listen(
        &mut server,
        json!(3),
        &["specforge://graph", "specforge://graph"],
    );
    assert_eq!(
        ack[0]["params"]["notifications"]["resourceSubscriptions"],
        json!(["specforge://graph"])
    );
    change(&mut server);
    let sent = server.take_notifications();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["method"], "notifications/resources/updated");
    assert_eq!(sent[0]["params"]["uri"], "specforge://graph");
}

#[specforge_test(
    behavior = "listen_for_mcp_resource_updates",
    verify = "a listen stream's subscription events name its request id"
)]
fn a_listen_streams_events_name_its_id() {
    let mut server = alpha();
    listen(&mut server, json!("abc"), &["specforge://graph"]);
    assert_eq!(
        events(&server, "mcp_subscription_created"),
        [json!({"resourceUri": "specforge://graph", "subscriptionId": "abc"})]
    );
}

// ---- Moved from notifications.rs: what a rebuild sends ----

/// A feature naming `ghost`, which no entity declares: the compile reports
/// E003.
const BROKEN: &str = "feature broken \"Broken\" {\n    behaviors [ghost]\n}\n";

/// A server over a project of `files`, `@test/ext` declaring the software
/// kinds.
fn served(files: &[(&str, &str)]) -> Served {
    files
        .iter()
        .fold(TestProject::new(), |project, (path, text)| {
            project.file(path, text)
        })
        .serve(&[TestExtension::software()])
}

/// Subscribe the default client to `uri`'s delta notifications.
fn subscribe(server: &mut McpServer, uri: &str) {
    let reply = call(server, "resources/subscribe", json!({"uri": uri}));
    assert_eq!(reply["result"], json!({}), "{reply}");
}

/// Any request: it brings the served project up to date with disk, and
/// the update's notifications are queued for the subscribers.
fn any_request(server: &mut McpServer) {
    let reply = call_tool(server, "specforge.stats", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
}

/// B:notify_graph_delta_via_mcp — verify unit "enqueue delivers one
/// notification per subscribed channel and suppresses empty channels"
#[test]
fn enqueue_delivers_graph_and_diagnostics_to_subscribers() {
    let mut server = served(&[]);
    subscribe(&mut server, "specforge://graph");
    subscribe(&mut server, "specforge://diagnostics");

    // One file adds an entity and a diagnostic.
    server.write("broken.spec", BROKEN);
    any_request(&mut server);

    assert_eq!(
        methods(&server.take_notifications()),
        [
            "notifications/resources/updated",
            "notifications/resources/updated",
            "specforge/graphChanged",
            "specforge/diagnosticsChanged",
        ]
    );

    // Taking them empties the queue (the captured client sink).
    assert!(server.take_notifications().is_empty());
}

/// B:notify_graph_delta_via_mcp — verify unit "enqueue suppresses
/// channels without subscribers and unchanged graphs"
#[test]
fn enqueue_suppresses_unsubscribed_and_unchanged() {
    let mut server = served(&[]);
    // Only the diagnostics channel is subscribed.
    subscribe(&mut server, "specforge://diagnostics");

    // Graph changed but nobody subscribes; diagnostics unchanged anyway.
    server.write("alpha.spec", "behavior alpha \"Alpha\" {\n}\n");
    any_request(&mut server);
    assert!(
        server.take_notifications().is_empty(),
        "graph delta must be suppressed without subscribers"
    );

    // Diagnostics changed and the channel is subscribed.
    server.write("broken.spec", BROKEN);
    any_request(&mut server);
    assert_eq!(
        methods(&server.take_notifications()),
        [
            "notifications/resources/updated",
            "specforge/diagnosticsChanged"
        ]
    );
}

// B:notify_diagnostics_delta_via_mcp — verify unit "no notification when diagnostics are unchanged"
#[specforge_test(
    behavior = "notify_diagnostics_delta_via_mcp",
    verify = "no notification when diagnostics are unchanged"
)]
fn diagnostics_no_notification_when_unchanged() {
    // A subscribed client gets nothing when a compile leaves the
    // diagnostics as they were (an entity added, broken.spec's E003 kept)
    // ...
    let mut server = served(&[("broken.spec", BROKEN)]);
    subscribe(&mut server, "specforge://diagnostics");
    server.write("gamma.spec", "behavior gamma \"Gamma\" {\n}\n");
    any_request(&mut server);
    let sent = server.take_notifications();
    assert!(sent.is_empty(), "{sent:?}");
    // ... and one notification when they change.
    server.remove("broken.spec");
    any_request(&mut server);
    assert_eq!(
        methods(&server.take_notifications()),
        [
            "notifications/resources/updated",
            "specforge/diagnosticsChanged"
        ]
    );
}

fn node_change(id: &str) -> NodeChange {
    NodeChange {
        id: id.to_string(),
        kind: "behavior".to_string(),
        file: "test.spec".to_string(),
        line: 1,
    }
}

#[specforge_test(
    behavior = "notify_graph_delta_via_mcp",
    verify = "notification includes GraphDelta payload"
)]
fn graph_changed_sends_the_whole_delta() {
    use specforge_project::{EdgeChange, ModifiedNodeChange};
    let mut subscriptions = Subscriptions::new();
    let mut events = EventLog::new();
    subscriptions.subscribe("specforge://graph", &mut events);

    let delta = GraphDelta {
        added_nodes: vec![node_change("alpha")],
        removed_nodes: vec![node_change("beta")],
        modified_nodes: vec![ModifiedNodeChange {
            id: "gamma".to_string(),
            changed_fields: vec!["status".to_string()],
            changed_aspects: vec![specforge_project::NodeAspect::Title],
            file: "test.spec".to_string(),
            line: 1,
        }],
        added_edges: vec![EdgeChange {
            source: "alpha".to_string(),
            target: "gamma".to_string(),
            label: "depends_on".to_string(),
        }],
        removed_edges: vec![EdgeChange {
            source: "beta".to_string(),
            target: "gamma".to_string(),
            label: "depends_on".to_string(),
        }],
        affected_files: vec!["test.spec".to_string()],
    };
    subscriptions.updated(&Changes::new(&delta, false, None), &mut events);
    let sent = drained(&mut subscriptions);
    assert_eq!(
        sent[1],
        json!({
            "jsonrpc": "2.0",
            "method": "specforge/graphChanged",
            "params": serde_json::to_value(&delta).unwrap(),
        })
    );
}

#[specforge_test(
    behavior = "notify_diagnostics_delta_via_mcp",
    verify = "payload includes added and removed diagnostics"
)]
fn diagnostics_changed_sends_whole_diagnostics() {
    let warning = Diagnostic::untyped("W001", Severity::Warning, "test warning")
        .with_span(SourceSpan {
            file: "a.spec".into(),
            start_line: 3,
            start_col: 1,
            end_line: 3,
            end_col: 9,
        })
        .with_suggestion("do the other thing");
    let delta = GraphDelta::default();
    let mut subscriptions = Subscriptions::new();
    let mut events = EventLog::new();
    subscriptions.subscribe("specforge://diagnostics", &mut events);

    subscriptions.updated(
        &Changes::new(&delta, false, Some((&[], std::slice::from_ref(&warning)))),
        &mut events,
    );
    assert_eq!(
        drained(&mut subscriptions)[1],
        json!({
            "jsonrpc": "2.0",
            "method": "specforge/diagnosticsChanged",
            "params": {
                "added": serde_json::to_value(specforge_common::diagnostics_json(
                    std::slice::from_ref(&warning)
                )).unwrap(),
                "removed": [],
            },
        })
    );
}

#[specforge_test(
    behavior = "notify_diagnostics_delta_via_mcp",
    verify = "each added or removed diagnostic is presented as every surface presents diagnostics"
)]
fn a_notified_diagnostic_is_presented_as_check_presents_it() {
    use specforge_common::DiagnosticData;
    let unresolved = Diagnostic::new(
        specforge_common::codes::E003,
        "unresolved reference 'ghost'",
    )
    .with_span(SourceSpan {
        file: "b.spec".into(),
        start_line: 4,
        start_col: 2,
        end_line: 4,
        end_col: 12,
    })
    .with_suggestion("did you mean 'alpha'?")
    .with_data(DiagnosticData::UnresolvedReference {
        target: "ghost".to_string(),
        entity: "beta".to_string(),
        field: "depends_on".to_string(),
        did_you_mean: Some("alpha".to_string()),
    });
    let delta = GraphDelta::default();
    let mut subscriptions = Subscriptions::new();
    let mut events = EventLog::new();
    subscriptions.subscribe("specforge://diagnostics", &mut events);

    for (before, after, key) in [
        (&[][..], std::slice::from_ref(&unresolved), "added"),
        (std::slice::from_ref(&unresolved), &[][..], "removed"),
    ] {
        subscriptions.updated(
            &Changes::new(&delta, false, Some((before, after))),
            &mut events,
        );
        let sent = drained(&mut subscriptions);
        let presented = serde_json::to_value(specforge_common::diagnostics_json(
            std::slice::from_ref(&unresolved),
        ))
        .unwrap();
        assert_eq!(sent[1]["params"][key], presented, "{key}");
        assert!(presented[0]["data"].is_object(), "{presented}");
        assert!(presented[0]["span"].is_object(), "{presented}");
    }
}

#[test]
fn a_diagnostic_is_named_by_code_message_and_file() {
    let graph = GraphDelta::default();
    let error = Diagnostic::new(specforge_common::codes::E001, "test error");
    let at = |line: usize| {
        let span = SourceSpan {
            file: "a.spec".into(),
            start_line: line,
            start_col: 0,
            end_line: line,
            end_col: 0,
        };
        Diagnostic::new(specforge_common::codes::E001, "test error").with_span(span)
    };
    let touched = |before: &[Diagnostic], after: &[Diagnostic]| {
        Changes::new(&graph, false, Some((before, after))).touched(Watched::Diagnostics)
    };

    assert!(touched(&[], std::slice::from_ref(&error)), "added");
    assert!(touched(std::slice::from_ref(&error), &[]), "removed");
    assert!(
        !touched(std::slice::from_ref(&error), std::slice::from_ref(&error)),
        "the same pair"
    );
    assert!(
        !touched(&[at(1)], &[at(9)]),
        "the same code, message and file on another line"
    );
    assert!(
        !Changes::new(&graph, false, None).touched(Watched::Diagnostics),
        "not read"
    );
}

#[test]
fn diagnostics_are_read_only_when_someone_hears_them() {
    let mut events = EventLog::new();
    let mut subscriptions = Subscriptions::new();
    assert!(!subscriptions.hears_diagnostics());
    subscriptions.subscribe("specforge://graph", &mut events);
    assert!(!subscriptions.hears_diagnostics());
    subscriptions.listen(json!(1), vec!["specforge://schema".into()], &mut events);
    assert!(!subscriptions.hears_diagnostics());
    subscriptions.subscribe("specforge://diagnostics", &mut events);
    assert!(subscriptions.hears_diagnostics());

    let mut streamed = Subscriptions::new();
    streamed.listen(
        json!(1),
        vec!["specforge://diagnostics".into()],
        &mut events,
    );
    assert!(streamed.hears_diagnostics());
}

#[test]
fn reopening_a_listen_id_ends_the_old_stream() {
    let mut subscriptions = Subscriptions::new();
    let mut events = EventLog::new();
    subscriptions.listen(json!(1), vec!["specforge://graph".into()], &mut events);
    drained(&mut subscriptions);
    events = EventLog::new();

    subscriptions.listen(
        json!(1),
        vec!["specforge://diagnostics".into()],
        &mut events,
    );
    let names: Vec<(String, serde_json::Value)> = events
        .recorded()
        .iter()
        .map(|recorded| (recorded.name(), recorded.payload()))
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(names[0].0, "mcp_subscription_removed");
    assert_eq!(names[0].1["resourceUri"], "specforge://graph");
    assert_eq!(names[0].1["subscriptionId"], "1");
    assert_eq!(names[1].0, "mcp_subscription_created");
    drained(&mut subscriptions);

    // A graph change reaches nobody: the stream on the graph ended.
    let delta = GraphDelta {
        added_nodes: vec![node_change("alpha")],
        ..GraphDelta::default()
    };
    subscriptions.updated(&Changes::new(&delta, false, None), &mut events);
    assert!(drained(&mut subscriptions).is_empty());
}

#[specforge_test(
    behavior = "listen_for_mcp_resource_updates",
    verify = "both eras decide what a change touches by one rule"
)]
fn one_update_is_heard_in_one_order() {
    let mut subscriptions = Subscriptions::new();
    let mut events = EventLog::new();
    subscriptions.listen(
        json!(1),
        vec!["specforge://diagnostics".into(), "specforge://graph".into()],
        &mut events,
    );
    subscriptions.subscribe("specforge://context", &mut events);
    subscriptions.subscribe("specforge://diagnostics", &mut events);
    drained(&mut subscriptions);

    let delta = GraphDelta {
        added_nodes: vec![node_change("alpha")],
        ..GraphDelta::default()
    };
    let added = Diagnostic::untyped("W001", Severity::Warning, "test warning");
    subscriptions.updated(
        &Changes::new(&delta, false, Some((&[], std::slice::from_ref(&added)))),
        &mut events,
    );

    let sent = drained(&mut subscriptions);
    let heard: Vec<(&str, &str, bool)> = sent
        .iter()
        .map(|n| {
            (
                n["method"].as_str().unwrap(),
                n["params"]["uri"].as_str().unwrap_or_default(),
                n["params"]["_meta"].is_object(),
            )
        })
        .collect();
    assert_eq!(
        heard,
        [
            (
                "notifications/resources/updated",
                "specforge://diagnostics",
                true
            ),
            ("notifications/resources/updated", "specforge://graph", true),
            (
                "notifications/resources/updated",
                "specforge://context",
                false
            ),
            (
                "notifications/resources/updated",
                "specforge://diagnostics",
                false
            ),
            ("specforge/graphChanged", "", false),
            ("specforge/diagnosticsChanged", "", false),
        ]
    );
}

#[test]
fn an_environment_reload_touches_the_graph_views_and_the_schema() {
    let none = GraphDelta::default();
    let reload = Changes::new(&none, true, None);
    assert!(reload.touched(Watched::Graph));
    assert!(reload.touched(Watched::Schema));
    assert!(!reload.touched(Watched::Diagnostics));

    let mut subscriptions = Subscriptions::new();
    let mut events = EventLog::new();
    subscriptions.subscribe("specforge://graph", &mut events);
    subscriptions.updated(&reload, &mut events);
    let sent = drained(&mut subscriptions);
    // The graph delta is empty: the standard update, and no graphChanged.
    assert_eq!(methods(&sent), ["notifications/resources/updated"]);
    assert_eq!(sent[0]["params"]["uri"], "specforge://graph");
}

#[test]
fn a_schema_subscriber_hears_no_graph_delta() {
    let mut subscriptions = Subscriptions::new();
    let mut events = EventLog::new();
    subscriptions.subscribe("specforge://schema", &mut events);
    let delta = GraphDelta {
        added_nodes: vec![node_change("alpha")],
        ..GraphDelta::default()
    };
    subscriptions.updated(&Changes::new(&delta, false, None), &mut events);
    assert!(drained(&mut subscriptions).is_empty());
}
