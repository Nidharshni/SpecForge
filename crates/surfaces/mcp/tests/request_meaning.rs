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

#[test]
#[ignore = "flipped by T6"]
fn export_refuses_both_schema_flags_on_with_schema() {
    let reply = call_tool(
        &mut served(),
        "specforge.export",
        json!({"format": "graph", "with_schema": true, "no_schema": true}),
    );
    assert_refused(&reply, "with_schema", None);
}
