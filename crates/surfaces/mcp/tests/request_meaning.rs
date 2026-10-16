//! What a request's values mean is the operation's, so each refusal reads the same on the wire (ADR 0101): the
//! code and the argument it is about. The pairing with the CLI is `cli/tests/request_pairs.rs`.

use crate::support::*;
use serde_json::{Value, json};
use specforge_test::prelude::*;

/// A server over a project with one behavior, `login`, and the cargo-test collector.
fn served() -> Served {
    TestProject::new()
        .enabling(&["@specforge/software", "@specforge/cargo-test"])
        .file("app.spec", "behavior login \"Log in\" {\n}\n")
        .serve_components()
}

/// The refused tool call's error: its code, the argument it names, and its diagnostic's code (`None`: not asserted).
fn assert_refused(reply: &Value, argument: &str, diagnostic: Option<&str>) {
    let error = crate::tool_errors::mcp_error(reply);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], argument, "{error}");
    if let Some(diagnostic) = diagnostic {
        assert_eq!(error["diagnostic"]["code"], diagnostic, "{error}");
    }
}

#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "a runner no collector answers to, auto included, is an E058 error on runner"
)]
fn collect_refuses_auto_as_an_unknown_runner() {
    let reply = call_tool(
        &mut served(),
        "specforge.collect",
        json!({"runner": "auto"}),
    );
    assert_refused(&reply, "runner", Some("E058"));
}

#[specforge_test(
    behavior = "provide_mcp_trace_tool",
    verify = "a trace call with both entity_id and plan is invalid input on plan"
)]
fn trace_refuses_an_entity_with_a_plan_on_plan() {
    let reply = call_tool(
        &mut served(),
        "specforge.trace",
        json!({"entity_id": "ghost", "plan": agent_plan(&["login"])}),
    );
    assert_refused(&reply, "plan", None);
}

#[specforge_test(
    behavior = "provide_mcp_trace_prompt",
    verify = "the trace prompt refuses an entity_id given with a plan, as invalid input on plan"
)]
fn trace_prompt_refuses_an_entity_with_a_plan_on_plan() {
    let reply = call(
        &mut served(),
        "prompts/get",
        json!({"name": "specforge://prompts/trace", "arguments": {
            "entity_id": "ghost", "plan": agent_plan(&["login"]).to_string()}}),
    );
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    assert_eq!(reply["error"]["data"]["code"], "invalid_input", "{reply}");
    assert_eq!(reply["error"]["data"]["argument"], "plan", "{reply}");
}

#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "with_schema and no_schema together are invalid input on with_schema"
)]
fn export_refuses_both_schema_flags_on_with_schema() {
    let reply = call_tool(
        &mut served(),
        "specforge.export",
        json!({"format": "graph", "with_schema": true, "no_schema": true}),
    );
    assert_refused(&reply, "with_schema", None);
}

/// A server over an empty workspace, for the calls that create a project.
fn workspace() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    specforge_mcp::McpServer,
) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("workspace");
    std::fs::create_dir_all(&root).unwrap();
    let mut server = specforge_mcp::McpServer::with_project_root(root.clone());
    call(&mut server, "initialize", json!({}));
    (dir, root, server)
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "each string in extensions is one specifier, as add_extension reads each of specifiers"
)]
fn init_reads_a_comma_string_as_add_extension_does() {
    let (_dir, root, mut server) = workspace();
    let reply = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": "created", "extensions": ["@specforge/product,@specforge/governance"]}),
    );
    assert_refused(&reply, "extensions", Some("E054"));
    assert!(!root.join("created").exists());
}

#[specforge_test(
    behavior = "parse_extension_specifier",
    verify = "a string holding commas is one extension specifier, for add and init alike"
)]
fn add_extension_reads_a_comma_string_as_one_specifier() {
    let reply = call_tool(
        &mut served(),
        "specforge.add_extension",
        json!({"specifiers": ["@specforge/product,@specforge/governance"]}),
    );
    assert_refused(&reply, "specifiers", Some("E054"));
}
