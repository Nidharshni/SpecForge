//! What a core resource template makes of its placeholder: one non-empty,
//! percent-decoded path segment, or a refusal naming the placeholder
//! (ADR 0103 D2-D3).

use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

use crate::support::*;

const SOFTWARE: &str = "@specforge/software";

const SOURCE: &str = "behavior alpha \"Alpha\" {\n  contract \"The system MUST work.\"\n  verify unit \"works\"\n}\n";

fn served() -> Served {
    TestProject::new()
        .enabling(&[SOFTWARE])
        .file("main.spec", SOURCE)
        .serve_components()
}

/// A resource read's JSON-RPC error.
fn read_error(server: &mut McpServer, uri: &str) -> Value {
    let reply = read_resource(server, uri);
    assert!(reply["result"].is_null(), "{uri} is served: {reply}");
    reply["error"].clone()
}

/// `uri` is refused as invalid input naming `argument`.
fn assert_refused(server: &mut McpServer, uri: &str, argument: &str) {
    let error = read_error(server, uri);
    assert_eq!(error["code"], json!(-32602), "{uri}: {error}");
    assert_eq!(error["data"]["code"], "invalid_input", "{uri}: {error}");
    assert_eq!(error["data"]["argument"], argument, "{uri}: {error}");
    assert_eq!(error["data"]["uri"], uri, "{uri}: {error}");
}

#[specforge_test(
    behavior = "expose_entity_as_mcp_resource",
    verify = "an empty entity_id or one of more than one path segment is invalid input naming entity_id, in both entity templates"
)]
fn an_entity_placeholder_is_one_non_empty_segment() {
    let mut server = served();
    for uri in [
        "specforge://graph/",
        "specforge://context/",
        "specforge://graph/alpha/x",
        "specforge://context/alpha/x",
    ] {
        assert_refused(&mut server, uri, "entity_id");
    }
}

#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "the entities resource refuses an empty kind or one of more than one path segment as invalid input naming kind"
)]
fn a_kind_placeholder_is_one_non_empty_segment() {
    let mut server = served();
    for uri in [
        "specforge://entities/",
        "specforge://entities/behavior/extra",
    ] {
        assert_refused(&mut server, uri, "kind");
    }
}

/// A server whose project holds two ref entities with characters the old charset refused.
fn served_with_refs() -> Served {
    TestProject::new()
        .enabling(&[SOFTWARE])
        .file("main.spec", SOURCE)
        .file(
            "refs.spec",
            "ref doc.page:v1+2 \"A page\"\nref gh.pr:org/repo#12 \"A pull request\"\n",
        )
        .serve_components()
}

// Linked to `expose_entity_as_mcp_resource` by T8, with its verify line.
#[test]
#[ignore = "flipped by T8"]
fn a_scheme_ref_id_is_an_entity_id_in_both_entity_templates() {
    let mut server = served_with_refs();
    for (uri, id) in [
        ("specforge://graph/doc.page:v1+2", "doc.page:v1+2"),
        (
            "specforge://graph/gh.pr:org%2Frepo%2312",
            "gh.pr:org/repo#12",
        ),
        ("specforge://context/doc.page:v1+2", "doc.page:v1+2"),
    ] {
        let reply = read_resource(&mut server, uri);
        assert!(reply["error"].is_null(), "{uri}: {reply}");
        assert!(reply["result"].to_string().contains(id), "{uri}: {reply}");
    }
    for uri in [
        "specforge://graph/a.b",
        "specforge://graph/a.b-c:d",
        "specforge://context/1abc",
    ] {
        assert_refused(&mut server, uri, "entity_id");
    }
}
