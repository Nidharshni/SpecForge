//! A core tool's refusal names the argument it is about (ADR 0033, plan 11).

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

/// One refused call: the tool, its arguments, the refusal's code and the
/// argument it is about (`None`: the refusal names none).
type Refusal = (&'static str, Value, &'static str, Option<&'static str>);

/// A server over a project with the software extensions, one behavior, a type
/// and a file that is no extension.
fn served() -> Served {
    TestProject::new()
        .enabling(&["@specforge/software"])
        .file(
            "app.spec",
            "behavior login \"Log in\" {\n  contract \"Given credentials, returns a token\"\n}\n\ntype account \"Account\" {\n}\n",
        )
        .file("notwasm.txt", "not wasm")
        .serve_components()
}

/// A server that serves nothing (`initialize` names no project), for init.
fn nothing_served() -> McpServer {
    let mut server = McpServer::new();
    let reply = call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
    server
}

/// The McpError of a refused tool call, asserting its `code`, the `argument`
/// it names (`None`: none) and the code of its diagnostic (`None`: not asserted).
fn assert_refused(reply: &Value, code: &str, argument: Option<&str>, diagnostic: Option<&str>) {
    let error = crate::tool_errors::mcp_error(reply);
    assert_eq!(error["code"], code, "{error}");
    assert_eq!(error["argument"].as_str(), argument, "{error}");
    if let Some(diagnostic) = diagnostic {
        assert_eq!(error["diagnostic"]["code"], diagnostic, "{error}");
    }
}

/// The JSON-RPC error of a refused prompt or resource read: its rpc code, then
/// its `data` (the McpError) by `code` and `argument`.
fn assert_rpc_refused(reply: &Value, rpc: i64, code: &str, argument: Option<&str>) {
    assert_eq!(reply["error"]["code"], rpc, "{reply}");
    let data = &reply["error"]["data"];
    assert_eq!(data["code"], code, "{reply}");
    assert_eq!(data["argument"].as_str(), argument, "{reply}");
}

/// One call of `tool` over a fresh server over [`served`].
fn call_served(tool: &str, arguments: Value) -> Value {
    call_tool(&mut served(), tool, arguments)
}

/// The refusals of §3.R5 whose argument is named today, as (tool, arguments,
/// code, argument).
fn named_today() -> Vec<Refusal> {
    vec![
        (
            "specforge.model",
            json!({"root": "nope"}),
            "invalid_input",
            Some("root"),
        ),
        (
            "specforge.model",
            json!({"extension": "@nope/x"}),
            "extension_not_found",
            Some("extension"),
        ),
        (
            "specforge.infer_session",
            json!({"action": "start", "source_roots": ["../x"]}),
            "invalid_input",
            Some("source_roots"),
        ),
        (
            "specforge.infer_session",
            json!({"action": "mark_analyzed", "source_file": "../x.rs"}),
            "invalid_input",
            Some("source_file"),
        ),
        (
            "specforge.infer_session",
            json!({"action": "end", "session_id": "nope"}),
            "invalid_input",
            Some("session_id"),
        ),
        (
            "specforge.rename",
            json!({"entity_id": "login", "new_name": "Bad Name"}),
            "invalid_input",
            Some("new_name"),
        ),
        (
            "specforge.analyze",
            json!({"pass": "nope"}),
            "invalid_input",
            Some("pass"),
        ),
        (
            "specforge.validate",
            json!({"severity_filter": "nope"}),
            "invalid_input",
            Some("severity_filter"),
        ),
        (
            "specforge.validate",
            json!({"lint": ["nope"]}),
            "invalid_input",
            Some("lint"),
        ),
        (
            "specforge.explain",
            json!({"code": "Z999"}),
            "invalid_input",
            Some("code"),
        ),
        (
            "specforge.schema",
            json!({"kind": "nope"}),
            "invalid_input",
            Some("kind"),
        ),
    ]
}

/// The refusals that name no argument today.
fn unnamed_today() -> Vec<Refusal> {
    vec![
        (
            "specforge.analyze",
            json!({"min": 150}),
            "invalid_input",
            Some("min"),
        ),
        (
            "specforge.export",
            json!({"depth": 2}),
            "invalid_input",
            Some("depth"),
        ),
        (
            "specforge.export",
            json!({"max_tokens": 5}),
            "invalid_input",
            Some("max_tokens"),
        ),
        (
            "specforge.migrate",
            json!({"target_version": "x.y"}),
            "invalid_input",
            Some("target_version"),
        ),
        (
            "specforge.add_extension",
            json!({"specifiers": ["!!bad"]}),
            "file_not_found",
            Some("specifiers"),
        ),
        (
            "specforge.add_extension",
            json!({"specifiers": ["@scope/name@not.a.version"]}),
            "invalid_input",
            Some("specifiers"),
        ),
    ]
}

/// Call each refusal and return the error body of the reply.
fn refused(tool: &str, arguments: &Value) -> Value {
    let mut server = served();
    let reply = call_tool(&mut server, tool, arguments.clone());
    assert_eq!(
        reply["result"]["isError"], true,
        "{tool} {arguments}: {reply}"
    );
    serde_json::from_str(&tool_text(&reply)).unwrap()
}

#[specforge_test(
    behavior = "read_mcp_arguments_as_declared",
    verify = "an operation's refusal of a core tool's call names the argument it is about"
)]
fn refusals_name_their_argument() {
    for (tool, arguments, code, argument) in named_today() {
        let error = refused(tool, &arguments);
        assert_eq!(error["code"], code, "{tool} {arguments}: {error}");
        assert_eq!(
            error["argument"].as_str(),
            argument,
            "{tool} {arguments}: {error}"
        );
    }
}

#[specforge_test(
    behavior = "read_mcp_arguments_as_declared",
    verify = "an operation's refusal of a core tool's call names the argument it is about"
)]
fn every_invalid_input_refusal_names_its_argument() {
    for (tool, arguments, code, argument) in named_today().into_iter().chain(unnamed_today()) {
        let error = refused(tool, &arguments);
        assert_eq!(error["code"], code, "{tool} {arguments}: {error}");
        assert_eq!(
            error["argument"].as_str(),
            argument,
            "{tool} {arguments}: {error}"
        );
        assert!(
            error["data"].get("argument").is_none(),
            "{tool} {arguments}: the argument is on the wire once: {error}"
        );
    }
}

#[specforge_test(
    behavior = "read_mcp_arguments_as_declared",
    verify = "a refusal a value the caller gave earns names that argument, whatever its kind"
)]
fn extension_refusals_name_their_argument() {
    let git = "git+https://example.com/x";
    let add = |specifiers: Value| {
        call_served("specforge.add_extension", json!({"specifiers": specifiers}))
    };
    assert_refused(
        &add(json!([git])),
        "invalid_input",
        Some("specifiers"),
        Some("E064"),
    );
    assert_refused(
        &add(json!(["./notwasm.txt"])),
        "invalid_input",
        Some("specifiers"),
        Some("E028"),
    );
    let mixed = add(json!(["@specforge/software", git]));
    assert_refused(&mixed, "invalid_input", Some("specifiers"), Some("E064"));
    let message = crate::tool_errors::mcp_error(&mixed)["message"].to_string();
    assert!(
        message.starts_with(&format!("\"adding '{git}': ")),
        "{message}"
    );

    let remove = |name: &str| call_served("specforge.remove_extension", json!({"name": name}));
    assert_refused(
        &remove("@@bad"),
        "invalid_input",
        Some("name"),
        Some("E072"),
    );
    assert_refused(
        &remove("@nope/x"),
        "extension_not_found",
        Some("name"),
        None,
    );

    let scratch = tempfile::TempDir::new().unwrap();
    let notwasm = scratch.path().join("notwasm.txt");
    std::fs::write(&notwasm, "not wasm").unwrap();
    let init = |dir: &str, extensions: Value| {
        let path = scratch.path().join(dir);
        call_tool(
            &mut nothing_served(),
            "specforge.init",
            json!({"path": path.to_str().unwrap(), "extensions": extensions}),
        )
    };
    assert_refused(
        &init("s1", json!([git])),
        "invalid_input",
        Some("extensions"),
        Some("E064"),
    );
    assert_refused(
        &init("s2", json!(["@a/b@!!"])),
        "invalid_input",
        Some("extensions"),
        Some("R-RES-003"),
    );
    assert_refused(
        &init("s3", json!([notwasm.to_str().unwrap()])),
        "invalid_input",
        Some("extensions"),
        Some("E028"),
    );

    let existing = served();
    let reply = call_tool(
        &mut nothing_served(),
        "specforge.init",
        json!({"path": existing.root().to_str().unwrap()}),
    );
    assert_refused(&reply, "conflict", Some("path"), None);
}

#[specforge_test(
    behavior = "read_mcp_arguments_as_declared",
    verify = "a refusal a value the caller gave earns names that argument, whatever its kind"
)]
#[ignore = "fixed by T4"]
fn read_view_refusals_name_their_argument() {
    assert_refused(
        &call_served("specforge.export", json!({"schema_version": "9.0.0"})),
        "conflict",
        Some("schema_version"),
        None,
    );
    assert_refused(
        &call_served("specforge.export", json!({"scope": "ghost"})),
        "entity_not_found",
        Some("scope"),
        None,
    );
    assert_refused(
        &call_served(
            "specforge.render",
            json!({"format": "graph", "scope": "ghost"}),
        ),
        "entity_not_found",
        Some("scope"),
        None,
    );
    assert_refused(
        &call_served("specforge.query", json!({"entity_id": "ghost"})),
        "entity_not_found",
        Some("entity_id"),
        None,
    );
    assert_refused(
        &call_served("specforge.analyze", json!({"test_results": "missing.json"})),
        "file_not_found",
        Some("test_results"),
        None,
    );

    let mut server = served();
    let reply = read_resource(&mut server, "specforge://graph?max_tokens=5");
    assert_rpc_refused(&reply, -32602, "invalid_input", Some("max_tokens"));
    let reply = read_resource(&mut server, "specforge://graph?root=ghost");
    assert_eq!(
        reply["error"]["data"]["code"], "entity_not_found",
        "{reply}"
    );
    assert_eq!(reply["error"]["data"]["argument"], "root", "{reply}");
    let reply = read_resource(&mut server, "specforge://graph/ghost");
    assert_rpc_refused(&reply, -32002, "entity_not_found", Some("entity_id"));
}

#[specforge_test(
    behavior = "read_mcp_arguments_as_declared",
    verify = "a prompt's or a resource read's refusal names the argument or query key it is about"
)]
#[ignore = "fixed by T5"]
fn entity_refusals_name_their_argument() {
    for tool in [
        "specforge.inspect",
        "specforge.find_definition",
        "specforge.find_references",
        "specforge.trace",
        "specforge.suggest_fixes",
    ] {
        assert_refused(
            &call_served(tool, json!({"entity_id": "ghost"})),
            "entity_not_found",
            Some("entity_id"),
            None,
        );
    }
    let mut server = served();
    for prompt in ["context", "explore", "review", "trace"] {
        let reply = get_prompt(
            &mut server,
            &format!("specforge://prompts/{prompt}"),
            json!({"entity_id": "ghost"}),
        );
        assert_rpc_refused(&reply, -32602, "entity_not_found", Some("entity_id"));
    }
    let reply = get_prompt(
        &mut server,
        "specforge://prompts/infer",
        json!({"scope": "kind:nope"}),
    );
    assert_rpc_refused(&reply, -32602, "invalid_input", Some("scope"));
}

#[specforge_test(
    behavior = "read_mcp_arguments_as_declared",
    verify = "a refusal a value the caller gave earns names that argument, whatever its kind"
)]
#[ignore = "fixed by T6"]
fn mutation_refusals_name_their_argument() {
    assert_refused(
        &call_served(
            "specforge.rename",
            json!({"entity_id": "ghost", "new_name": "good_name"}),
        ),
        "entity_not_found",
        Some("entity_id"),
        None,
    );
    assert_refused(
        &call_served(
            "specforge.rename",
            json!({"entity_id": "login", "new_name": "account"}),
        ),
        "conflict",
        Some("new_name"),
        None,
    );
    let mut collecting = TestProject::new()
        .enabling(&["@specforge/software", "@specforge/cargo-test"])
        .file("app.spec", "behavior login \"Log in\" {\n}\n")
        .serve_components();
    let reply = call_tool(
        &mut collecting,
        "specforge.collect",
        json!({"runner": "nope"}),
    );
    assert_refused(&reply, "invalid_input", Some("runner"), Some("E058"));
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
)]
fn a_refusal_of_the_project_names_no_argument() {
    // No registry is configured: a refusal of the project.
    assert_refused(
        &call_served(
            "specforge.add_extension",
            json!({"specifiers": ["@acme/x@^9"]}),
        ),
        "precondition_failed",
        None,
        Some("E063"),
    );

    // An unusable recorded report is a refusal of the project, and a prompt's
    // refusal no argument fixes is -32603 with the operation's message.
    let mut server = TestProject::new()
        .enabling(&["@specforge/software"])
        .file("app.spec", "behavior login \"Log in\" {\n}\n")
        .file("specforge-report.json", "{\"not\":\"a report\"}")
        .serve_components();
    let reply = call_tool(&mut server, "specforge.coverage", json!({}));
    assert_refused(&reply, "schema_mismatch", None, Some("E045"));
    let reply = get_prompt(&mut server, "specforge://prompts/review", json!({}));
    assert_rpc_refused(&reply, -32603, "schema_mismatch", None);
    assert_eq!(
        reply["error"]["data"]["diagnostic"]["code"], "E045",
        "{reply}"
    );
    assert_eq!(
        reply["error"]["message"], reply["error"]["data"]["message"],
        "{reply}"
    );
    assert_ne!(reply["error"]["message"], "Internal error", "{reply}");
}
