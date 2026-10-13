//! A core tool's refusal names the argument it is about (ADR 0033, plan 11).

use crate::support::*;
use serde_json::{Value, json};
use specforge_test::prelude::*;

/// One refused call: the tool, its arguments, the refusal's code and the
/// argument it is about (`None`: the refusal names none).
type Refusal = (&'static str, Value, &'static str, Option<&'static str>);

/// A server over a project with one behavior.
fn served() -> Served {
    TestProject::new()
        .enabling(&["@specforge/software"])
        .file(
            "app.spec",
            "behavior login \"Log in\" {\n  contract \"Given credentials, returns a token\"\n}\n",
        )
        .serve_components()
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
