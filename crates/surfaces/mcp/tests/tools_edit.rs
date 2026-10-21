//! `specforge.edit`: a list of typed edits, validated and written all or nothing (ADR 0151).

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

const TOKENS: &str =
    "invariant token_unique \"Tokens are unique\" {\n  guarantee \"Token ids MUST be unique\"\n}\n";
const LOGIN: &str = "behavior login \"Log in\" {\n  invariants [token_unique]\n  contract \"The system MUST issue a token\"\n}\n";

fn served() -> (Served, std::path::PathBuf) {
    let server = TestProject::new()
        .enabling(&["@specforge/software"])
        .file("spec/tokens.spec", TOKENS)
        .file("spec/login.spec", LOGIN)
        .serve_components();
    let root = server.root().to_path_buf();
    (server, root)
}

fn edit(server: &mut McpServer, args: Value) -> Value {
    let resp = call_tool(server, "specforge.edit", args);
    serde_json::from_str(&tool_text(&resp)).unwrap_or_else(|_| panic!("{resp}"))
}

fn read(root: &std::path::Path, file: &str) -> String {
    std::fs::read_to_string(root.join(file)).unwrap()
}

#[specforge_test(
    behavior = "provide_mcp_edit_tool",
    verify = "specforge.edit applies a list of edits all or nothing"
)]
fn edit_applies_a_list_all_or_nothing() {
    let (mut server, root) = served();
    // A new invariant and a link to it, in one call: neither could be made alone.
    let reply = edit(
        &mut server,
        json!({"edits": [
            {"op": "create", "kind": "invariant", "id": "token_fresh", "title": "Tokens are fresh",
             "file": "spec/tokens.spec", "fields": {"guarantee": "Tokens MUST expire"}},
            {"op": "link", "entity_id": "login", "field": "invariants", "target": "token_fresh"},
        ]}),
    );
    assert_eq!(
        reply["entities"],
        json!(["login", "token_fresh"]),
        "{reply}"
    );
    assert_eq!(
        reply["affected_files"],
        json!(["spec/login.spec", "spec/tokens.spec"]),
        "{reply}"
    );
    assert!(read(&root, "spec/login.spec").contains("invariants [token_unique, token_fresh]"));
    assert!(read(&root, "spec/tokens.spec").contains("invariant token_fresh"));
    assert!(server.state().graph().node("token_fresh").is_some());

    // The second edit adds an error: the first is not written either.
    let before = (
        read(&root, "spec/login.spec"),
        read(&root, "spec/tokens.spec"),
    );
    let resp = call_tool(
        &mut server,
        "specforge.edit",
        json!({"edits": [
            {"op": "set", "entity_id": "login", "field": "contract", "value": "The system MUST do more"},
            {"op": "link", "entity_id": "login", "field": "invariants", "target": "nope_rule"},
        ]}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["diagnostic"]["code"], "E080", "{error}");
    assert_eq!(error["data"]["diagnostics"][0]["code"], "E003", "{error}");
    assert_eq!(
        before,
        (
            read(&root, "spec/login.spec"),
            read(&root, "spec/tokens.spec")
        )
    );
}

#[specforge_test(
    behavior = "provide_mcp_edit_tool",
    verify = "specforge.edit with dry_run writes nothing and returns the diff"
)]
fn edit_with_dry_run_writes_nothing_and_returns_the_diff() {
    let (mut server, root) = served();
    let reply = edit(
        &mut server,
        json!({"edits": [{"op": "retitle", "entity_id": "login", "title": "Sign in"}], "dry_run": true}),
    );
    assert_eq!(reply["dry_run"], true, "{reply}");
    assert!(
        reply["diff"]
            .as_str()
            .unwrap()
            .contains("+behavior login \"Sign in\""),
        "{reply}"
    );
    assert!(reply.get("files_written").is_none(), "{reply}");
    assert_eq!(read(&root, "spec/login.spec"), LOGIN);
}

#[test]
fn edit_refusal_names_the_edits_argument() {
    let (mut server, _root) = served();
    let resp = call_tool(
        &mut server,
        "specforge.edit",
        json!({"edits": [{"op": "set", "entity_id": "login", "field": "categroy", "value": "x"}]}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], "edits", "{error}");

    let resp = call_tool(
        &mut server,
        "specforge.edit",
        json!({"edits": [{"op": "frobnicate"}]}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["argument"], "edits", "{error}");
    let resp = call_tool(&mut server, "specforge.edit", json!({}));
    crate::tool_errors::mcp_error(&resp);
}

#[test]
fn edit_reply_lists_files_written_and_fresh_diagnostics() {
    let (mut server, root) = served();
    let reply = edit(
        &mut server,
        json!({"edits": [{"op": "unset", "entity_id": "login", "field": "invariants"}]}),
    );
    assert_eq!(
        reply["files_written"],
        json!(["spec/login.spec"]),
        "{reply}"
    );
    assert!(reply["diagnostics"].is_array(), "{reply}");
    assert!(!read(&root, "spec/login.spec").contains("invariants"));
}

#[test]
fn edits_may_be_given_as_a_string_holding_their_json() {
    let (mut server, root) = served();
    let edits = json!([{"op": "retitle", "entity_id": "login", "title": "Sign in"}]).to_string();
    let reply = edit(&mut server, json!({"edits": edits}));
    assert_eq!(reply["entities"], json!(["login"]), "{reply}");
    assert!(read(&root, "spec/login.spec").contains("behavior login \"Sign in\""));
}

#[test]
fn edit_on_another_project_by_path_leaves_the_served_one() {
    let (mut server, root) = served();
    let other = within(
        &root,
        TestProject::new()
            .enabling(&["@specforge/software"])
            .file("spec/login.spec", LOGIN.replace("[token_unique]", "[]"))
            .file("spec/tokens.spec", TOKENS)
            .into_dir(),
    );
    let reply = edit(
        &mut server,
        json!({"path": other.path().to_str().unwrap(),
               "edits": [{"op": "link", "entity_id": "login", "field": "invariants", "target": "token_unique"}]}),
    );
    assert_eq!(
        reply["files_written"].as_array().unwrap().len(),
        1,
        "{reply}"
    );
    assert!(read(other.path(), "spec/login.spec").contains("invariants [token_unique]"));
    assert_eq!(read(&root, "spec/login.spec"), LOGIN);
}

#[specforge_test(
    behavior = "provide_mcp_edit_tool",
    verify = "specforge.edit of a list that cancels out writes nothing and names no entity"
)]
fn specforge_edit_of_a_list_that_cancels_out_writes_and_names_nothing() {
    let (mut server, root) = served();
    let reply = edit(
        &mut server,
        json!({"edits": [
            {"op": "create", "kind": "invariant", "id": "token_new", "title": "Tokens are new",
             "file": "spec/new.spec", "fields": {"guarantee": "Tokens MUST be new"}},
            {"op": "delete", "entity_id": "token_new"},
        ]}),
    );
    assert_eq!(reply["files_written"], json!([]), "{reply}");
    assert_eq!(reply["entities"], json!([]), "{reply}");
    assert_eq!(reply["affected_files"], json!([]), "{reply}");
    assert!(!root.join("spec/new.spec").exists());
    assert_eq!(
        events(&server, "mcp_mutation_completed"),
        [completed("specforge.edit", 0, 0, true)]
    );
}
