//! The project an MCP server serves is replaced whole, by one install, or
//! not at all (`mcp_served_project_consistency`, plan 01 P7).

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn initialize(server: &mut McpServer, root: &Path) {
    let resp = call(
        server,
        "initialize",
        json!({"projectRoot": root.to_str().unwrap()}),
    );
    assert!(resp["error"].is_null(), "{resp}");
}

/// A project with `specforge.json` listing `extensions` and one spec file.
fn project(extensions: &[&str], spec: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    write_config(dir.path(), extensions);
    fs::write(dir.path().join("main.spec"), spec).unwrap();
    dir
}

fn write_config(root: &Path, extensions: &[&str]) {
    let config = json!({"name": "served", "version": "0.1.0", "extensions": extensions});
    fs::write(root.join("specforge.json"), config.to_string()).unwrap();
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "analyze notifies subscribers when the diagnostics it compiled changed"
)]
fn analyze_notifies_the_diagnostics_it_compiled() {
    let dir = project(&[], "behavior login \"Login\" {\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://diagnostics"}),
    );
    server.take_notifications();

    // A dangling reference: the next compile reports an E003.
    fs::write(
        dir.path().join("main.spec"),
        "behavior login \"Login\" {\n  invariants [session_limit]\n}\n",
    )
    .unwrap();
    let resp = call_tool(&mut server, "specforge.analyze", json!({}));
    assert!(resp["error"].is_null(), "{resp}");

    let sent = server.take_notifications();
    let changed: Vec<&Value> = sent
        .iter()
        .filter(|n| n["method"] == "specforge/diagnosticsChanged")
        .collect();
    assert_eq!(changed.len(), 1, "{sent:?}");
    let added = changed[0]["params"]["added"].as_array().unwrap();
    assert!(added.iter().any(|d| d["code"] == "E003"), "{added:?}");
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "validate with a path to another project leaves the served project in place"
)]
fn validate_on_another_project_keeps_serving_this_one() {
    let served = project(&[], "behavior login \"Login\" {\n}\n");
    let other = within(
        served.path(),
        project(
            &[],
            "behavior other \"Other\" {\n  invariants [missing_one]\n}\n",
        ),
    );
    let mut server = McpServer::new();
    initialize(&mut server, served.path());

    let resp = call_tool(
        &mut server,
        "specforge.validate",
        json!({"path": other.path().to_str().unwrap()}),
    );
    // The other project's diagnostics are the answer...
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let diagnostics: Value = serde_json::from_str(text).unwrap();
    assert!(
        diagnostics
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E003"),
        "{diagnostics}"
    );

    // ...and the server still serves its own project.
    let state = server.state();
    assert_eq!(
        state.project_root().map(|p| fs::canonicalize(p).unwrap()),
        Some(fs::canonicalize(served.path()).unwrap())
    );
    assert!(state.graph().node("login").is_some());
    assert!(state.graph().node("other").is_none());
    assert!(state.diagnostics().iter().all(|d| d.code != "E003"));
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation tool that wrote files leaves the server serving what is on disk"
)]
fn a_format_that_wrote_files_is_served() {
    // Three blank lines inside the block: formatting removes them.
    let dir = project(&[], "behavior login \"Login\" {\n\n\n\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());
    let end_line = |server: &McpServer| {
        server
            .state()
            .graph()
            .node("login")
            .map(|n| n.source_span.end_line)
    };
    let before = end_line(&server);

    let resp = call_tool(&mut server, "specforge.format", json!({}));
    assert!(resp["error"].is_null(), "{resp}");
    let on_disk = fs::read_to_string(dir.path().join("main.spec")).unwrap();
    assert_ne!(on_disk.lines().count(), 5, "format rewrote the file");

    let after = end_line(&server);
    assert_ne!(
        after, before,
        "the server still serves the unformatted file"
    );
    assert_eq!(after, Some(on_disk.lines().count()));
}

/// A request before `initialize` serves the project at the server's root; `initialize` naming
/// another project then replaces it whole (ADR 0109: the served session is reopened at the new
/// root).
#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "initialize naming another project replaces the project a stateless request served"
)]
fn initialize_naming_another_project_replaces_the_one_a_stateless_request_served() {
    let a = project(&[], "behavior a \"A\" {\n");
    let b = project(&[], "behavior b \"B\" {\n}\n");
    let canonical = |dir: &TempDir| fs::canonicalize(dir.path()).unwrap();
    let mut server = McpServer::with_project_root(a.path().to_path_buf());

    let resp = call(
        &mut server,
        "tools/call",
        json!({
            "name": "specforge.validate",
            "arguments": {},
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        }),
    );
    assert!(resp["error"].is_null(), "{resp}");
    let state = server.state();
    assert_eq!(
        state.project_root().map(|p| fs::canonicalize(p).unwrap()),
        Some(canonical(&a))
    );
    assert!(state.diagnostics().iter().any(|d| d.code == "E001"));

    initialize(&mut server, b.path());
    let state = server.state();
    assert_eq!(
        state.project_root().map(|p| fs::canonicalize(p).unwrap()),
        Some(canonical(&b))
    );
    assert!(state.diagnostics().iter().all(|d| d.code != "E001"));
    assert!(state.graph().node("b").is_some());
    assert!(state.graph().node("a").is_none());
}

/// Shutdown stops serving the project whole: its graph, diagnostics,
/// config and spec root go with its session.
#[test]
fn shutdown_serves_nothing_of_the_project() {
    let dir = project(&["@specforge/software"], "behavior login \"Login\" {\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());
    assert_eq!(server.state().config().name.as_deref(), Some("served"));
    assert!(server.state().spec_root().is_some());

    call(&mut server, "shutdown", json!({}));

    let state = server.state();
    assert_eq!(state.graph().node_count(), 0);
    assert!(state.diagnostics().is_empty());
    assert_eq!(state.config().name, None);
    assert!(state.spec_root().is_none());
    assert!(state.registries().rules().is_empty());
    assert!(state.session().runtime().is_none());
}
