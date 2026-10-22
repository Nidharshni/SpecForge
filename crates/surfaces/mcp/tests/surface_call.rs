//! What one request that invokes a named tool, resource or prompt answers,
//! per request kind (ADR 0024): its failures, its events, its lookup, its
//! subscriptions. Every test serves a project on disk with the builtin
//! extensions, as `specforge mcp <root>` does.
//!
//! A pin marked `PIN` asserts today's behaviour and is flipped, with its
//! spec link, by the ticket it names.

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_ops::read::export::{self, Format, Request};
use specforge_ops::view::ProjectView;
use specforge_test::prelude::*;

const SOFTWARE: &str = "@specforge/software";
const PRODUCT: &str = "@specforge/product";

/// `alpha` then `zeta`, two behaviors declared in that order.
const SOURCE: &str = "behavior zeta \"Zeta\" {\n  contract \"The system MUST rest.\"\n  verify unit \"rests\"\n}\n\nbehavior alpha \"Alpha\" {\n  contract \"The system MUST work.\"\n  verify unit \"works\"\n}\n";

fn project() -> TestProject {
    TestProject::new()
        .enabling(&[SOFTWARE])
        .file("main.spec", SOURCE)
}

fn served() -> Served {
    project().serve_components()
}

/// `specforge.json` enabling `extensions`.
fn config(extensions: &[&str]) -> String {
    json!({"name": "t", "version": "0.1.0", "extensions": extensions}).to_string()
}

/// The `_meta` a stateless (2026-07-28) request carries.
fn modern(mut params: Value) -> Value {
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    params
}

/// A resource read's JSON-RPC error.
fn read_error(server: &mut McpServer, uri: &str) -> Value {
    let reply = read_resource(server, uri);
    assert!(reply["result"].is_null(), "{uri} is served: {reply}");
    reply["error"].clone()
}

/// A tool call's `isError` result, parsed.
fn tool_error(response: &Value) -> Value {
    assert_eq!(response["result"]["isError"], true, "{response}");
    tool_json(response)
}

/// The names of every event recorded so far.
fn event_names(server: &impl Logs) -> Vec<String> {
    server.log().names()
}

/// The names of the events `act` records, `mcp_protocol_error_handled`
/// (the report of any JSON-RPC error) left out.
fn recorded(server: &mut Served, act: impl FnOnce(&mut McpServer)) -> Vec<String> {
    let before = server.log().len();
    act(server);
    event_names(server)
        .split_off(before)
        .into_iter()
        .filter(|name| name != "mcp_protocol_error_handled")
        .collect()
}

// --- P1: a path on resources/read is not an argument ---

#[test]
fn a_resource_read_ignores_a_path_param() {
    let mut server = served();
    let reply = call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://graph", "path": "/nope"}),
    );
    let graph: Value = serde_json::from_str(&resource_text(&reply)).unwrap();
    assert_eq!(graph["format_version"], "2.0", "{reply}");
    assert!(graph["nodes"].as_array().is_some_and(|n| n.len() >= 2));
}

// --- P2: graph/{id} and graph?scope= name one subgraph ---

#[specforge_test(
    behavior = "expose_entity_as_mcp_resource",
    verify = "the entity resource is the scoped graph export with its schema_ref"
)]
fn the_entity_resource_is_the_scoped_graph_export() {
    let mut server = served();
    let (content, entity) = resource(&mut server, "specforge://graph/alpha");
    assert_eq!(entity["format_version"], "2.0");
    assert!(entity["schema_ref"].is_object(), "{entity}");
    assert_eq!(content["uri"], "specforge://graph/alpha");

    // The same document, byte for byte, as the scoped read at depth 1 and as
    // `specforge export --format graph --scope alpha --depth 1`.
    let by_query = resource_text(&read_resource(
        &mut server,
        "specforge://graph?scope=alpha&depth=1",
    ));
    let by_entity = resource_text(&read_resource(&mut server, "specforge://graph/alpha"));
    assert_eq!(by_entity, by_query);
    let root = server.root().to_path_buf();
    let request = Request {
        format: Some(Format::Graph),
        scope: Some("alpha"),
        depth: Some(1),
        ..Request::default()
    };
    assert_eq!(by_entity, exported(&root, &request));

    // The entity's own query is read, as `--depth` is: one that cannot be is
    // refused, not ignored.
    let error = read_error(&mut server, "specforge://graph/alpha?depth=wide");
    assert_eq!(error["data"]["argument"], "depth", "{error}");
}

// --- P3: a core resource's refusal ---

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "a failed resources/read carries its McpError as the error's data"
)]
fn a_failed_resource_read_carries_its_mcp_error() {
    let mut server = served();
    // A resource that does not exist is not found, in a handshake session
    // -32002; its data is its McpError, naming the URI read.
    for uri in [
        "specforge://graph?root=ghost",
        "specforge://context/ghost",
        "specforge://graph/ghost",
    ] {
        let error = read_error(&mut server, uri);
        assert_eq!(error["code"], -32002, "{uri}: {error}");
        assert_eq!(error["data"]["code"], "entity_not_found", "{uri}: {error}");
        assert_eq!(error["data"]["entity_id"], "ghost", "{uri}: {error}");
        assert_eq!(
            error["data"]["diagnostic"]["code"], "E003",
            "{uri}: {error}"
        );
        assert_eq!(error["data"]["uri"], uri, "{uri}: {error}");
        assert!(error["data"].get("resource").is_none(), "{uri}: {error}");
        assert!(
            !error["message"].as_str().unwrap().starts_with("E003"),
            "the code is in `diagnostic`: {error}"
        );
    }
    // The entity resource says it as the export does.
    let error = read_error(&mut server, "specforge://graph/ghost");
    assert_eq!(
        error["message"],
        "unresolved entity 'ghost' — not found in graph"
    );
}

#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "a resource that does not exist is -32002 in a handshake session and -32602 in a 2026-07-28 request, its data naming the uri"
)]
fn a_missing_resource_is_not_found_in_the_revision_it_was_asked_in() {
    let mut server = served();
    for uri in ["specforge://nope", "specforge://graph/ghost"] {
        // A handshake session (2025-11-25) says -32002.
        let error = read_error(&mut server, uri);
        assert_eq!(error["code"], -32002, "{uri}: {error}");
        assert_eq!(error["data"]["uri"], uri, "{uri}: {error}");

        // A stateless request (2026-07-28) says -32602, naming the same URI.
        let reply = call(&mut server, "resources/read", modern(json!({"uri": uri})));
        assert_eq!(reply["error"]["code"], -32602, "{uri}: {reply}");
        assert_eq!(reply["error"]["data"]["uri"], uri, "{uri}: {reply}");
    }
    // Other refusals are not "not found": invalid input stays -32602 in a
    // handshake session.
    let error = read_error(&mut server, "specforge://graph?max_tokens=1");
    assert_ne!(error["code"], -32002, "{error}");
}

// --- P4, P5: a prompt's refusal ---

#[specforge_test(
    behavior = "serve_mcp_prompt",
    verify = "a prompt refusal is a JSON-RPC error whose data is an McpError naming the prompt"
)]
fn a_prompt_refusal_carries_its_mcp_error() {
    let mut server = served();
    let reply = get_prompt(
        &mut server,
        "specforge://prompts/context",
        json!({"entity_id": "ghost"}),
    );
    let error = &reply["error"];
    assert_eq!(error["code"], -32602, "{reply}");
    assert_eq!(error["data"]["code"], "entity_not_found", "{reply}");
    assert_eq!(error["data"]["diagnostic"]["code"], "E003", "{reply}");
    assert_eq!(
        error["data"]["prompt"], "specforge://prompts/context",
        "{reply}"
    );
}

#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "a refusal naming an argument is -32602 for a prompt or a resource read"
)]
fn a_prompt_path_refusal_is_invalid_params() {
    let mut server = served();
    let reply = get_prompt(
        &mut server,
        "specforge://prompts/context",
        json!({"entity_id": "alpha", "path": "nope"}),
    );
    // An argument the client named: -32602, not a server fault.
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    assert_eq!(reply["error"]["data"]["code"], "file_not_found", "{reply}");
    assert_eq!(reply["error"]["data"]["argument"], "path", "{reply}");
}

// --- P6: a tool's path refusal ---

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "a path that does not exist is a file_not_found error on path"
)]
fn a_tool_path_refusal_is_file_not_found() {
    let mut server = served();
    let reply = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha", "path": "nope"}),
    );
    let error = tool_error(&reply);
    assert_eq!(error["code"], "file_not_found", "{error}");
    assert_eq!(error["argument"], "path", "{error}");
    assert_eq!(error["tool"], "specforge.query", "{error}");
}

// --- P7, P8: an operation's failure code ---

#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "a token budget too small for the export is invalid_input carrying E062"
)]
fn a_budget_too_small_is_invalid_input() {
    let mut server = served();
    let reply = call_tool(&mut server, "specforge.export", json!({"max_tokens": 1}));
    let error = tool_error(&reply);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["diagnostic"]["code"], "E062", "{error}");
    assert!(
        !error["message"].as_str().unwrap().starts_with("E062"),
        "the code is in `diagnostic`: {error}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "specforge.export takes the options specforge export takes"
)]
fn export_takes_a_schema_version() {
    let mut server = served();
    // A version of another major is refused as `specforge export
    // --schema-version` refuses it: the operation's one error.
    let root = server.root().to_path_buf();
    let expected = {
        let runtime = specforge_project::testing::component_runtime();
        let project = specforge_project::testing::compile_at(&root, Some(runtime));
        specforge_ops::read::export::export(
            &ProjectView::of(&project),
            &Request {
                format: Some(Format::Graph),
                schema_version: Some("99.0.0"),
                ..Request::default()
            },
        )
        .unwrap_err()
    };
    let reply = call_tool(
        &mut server,
        "specforge.export",
        json!({"format": "graph", "schema_version": "99.0.0"}),
    );
    let error = tool_error(&reply);
    // An operation's own code (a slug) does not ride in the tool error: the
    // kind picks `code`, the message names the refusal.
    assert_eq!(expected.error.code.as_ref(), "incompatible_schema_version");
    assert_eq!(error["code"], "conflict", "{error}");
    assert_eq!(error["message"], expected.error.message.as_str(), "{error}");

    // A version of this major is served at that version.
    let reply = call_tool(
        &mut server,
        "specforge.export",
        json!({"format": "graph", "schema_version": "1.0.0"}),
    );
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    let document: Value = serde_json::from_str(text).unwrap();
    assert_eq!(document["schema_version"], "1.0.0", "{document}");

    // `depth` needs a `scope`, as on every surface.
    let reply = call_tool(
        &mut server,
        "specforge.export",
        json!({"format": "graph", "depth": 1}),
    );
    let error = tool_error(&reply);
    assert_eq!(error["code"], "invalid_input", "{error}");
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "--schema-version and the export tool's schema_version are checked whatever the format"
)]
fn the_export_tools_schema_version_is_checked_whatever_the_format() {
    let mut server = served();
    for format in ["graph", "brief", "context"] {
        let reply = call_tool(
            &mut server,
            "specforge.export",
            json!({"format": format, "schema_version": "2.0.0"}),
        );
        let error = tool_error(&reply);
        assert_eq!(error["code"], "conflict", "{format}: {error}");
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .starts_with("requested schema version 2.0.0 has another major version"),
            "{format}: {error}"
        );
    }
}

#[test]
fn a_budget_too_small_for_a_resource_is_invalid_input() {
    let mut server = served();
    // Invalid input, not "not found": -32602, its McpError carrying E062.
    let error = read_error(&mut server, "specforge://graph?max_tokens=1");
    assert_eq!(error["code"], -32602, "{error}");
    assert_eq!(error["data"]["code"], "invalid_input", "{error}");
    assert_eq!(error["data"]["diagnostic"]["code"], "E062", "{error}");
    assert_eq!(error["data"]["uri"], "specforge://graph?max_tokens=1");
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "an operation's failure kind decides its McpError code, never its code text"
)]
fn an_unknown_scope_is_entity_not_found_with_its_code_in_diagnostic() {
    let mut server = served();
    let reply = call_tool(&mut server, "specforge.export", json!({"scope": "ghost"}));
    let error = tool_error(&reply);
    assert_eq!(error["code"], "entity_not_found", "{error}");
    assert_eq!(error["entity_id"], "ghost", "{error}");
    assert_eq!(error["diagnostic"]["code"], "E003", "{error}");
    assert!(
        !error["message"].as_str().unwrap().starts_with("E003"),
        "the code is in `diagnostic`, not repeated in the message: {error}"
    );
}

// --- P9: subscribe ---

#[specforge_test(
    behavior = "notify_graph_delta_via_mcp",
    verify = "resources/subscribe to a URI the server does not serve is refused as not found, as resources/read refuses it"
)]
fn subscribing_to_an_unserved_uri_is_refused_as_not_found() {
    let mut server = served();
    for uri in ["specforge://nope", "not a uri at all"] {
        // A handshake session says not found as -32002, with the URI.
        let reply = call(&mut server, "resources/subscribe", json!({"uri": uri}));
        assert_eq!(reply["error"]["code"], -32002, "{reply}");
        assert_eq!(
            reply["error"]["message"],
            format!("Unknown resource URI: {uri}")
        );
        assert_eq!(reply["error"]["data"], json!({ "uri": uri }), "{reply}");
        // The same URI a read refuses, refused the same way.
        let read = read_error(&mut server, uri);
        assert_eq!(read["code"], reply["error"]["code"]);
        assert_eq!(read["message"], reply["error"]["message"]);
    }
    assert!(server.state().subscriptions().is_empty());

    // What the server serves is subscribed to, and unsubscribing never fails.
    for uri in [
        "specforge://graph",
        "specforge://graph/alpha",
        "specforge://diagnostics",
    ] {
        let reply = call(&mut server, "resources/subscribe", json!({"uri": uri}));
        assert_eq!(reply["result"], json!({}), "{uri}: {reply}");
    }
    assert_eq!(
        server.state().subscriptions().subscribed(),
        [
            "specforge://graph",
            "specforge://graph/alpha",
            "specforge://diagnostics"
        ]
    );
    for uri in [
        "specforge://nope",
        "specforge://graph",
        "specforge://never-subscribed",
    ] {
        let reply = call(&mut server, "resources/unsubscribe", json!({"uri": uri}));
        assert_eq!(reply["result"], json!({}), "{uri}: {reply}");
    }
    assert_eq!(
        server.state().subscriptions().subscribed(),
        ["specforge://graph/alpha", "specforge://diagnostics"]
    );
}

// --- P10: an extension enabled on disk ---

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "an extension tool enabled on disk since the last request is callable by name"
)]
fn an_extension_enabled_on_disk_is_callable_by_the_next_request() {
    let mut server = served();
    let features =
        |server: &mut McpServer| call_tool(server, "specforge.product.features", json!({}));
    let unknown = features(&mut server);
    assert_eq!(unknown["error"]["code"], -32602, "{unknown}");
    assert_eq!(
        unknown["error"]["message"],
        "Unknown tool: specforge.product.features"
    );

    // The very next request after the edit finds the tool: the lookup of an
    // extension name brings the served project up to date first.
    server.write("specforge.json", &config(&[SOFTWARE, PRODUCT]));
    let known = features(&mut server);
    assert!(known["error"].is_null(), "{known}");
    assert_eq!(known["result"]["isError"], false, "{known}");
    assert_eq!(tool_json(&known)["has_more"], false, "{known}");

    // And an extension taken off the list is unknown at once.
    server.write("specforge.json", &config(&[SOFTWARE]));
    let gone = features(&mut server);
    assert_eq!(gone["error"]["code"], -32602, "{gone}");
}

/// A server over a project that enables no extension, its runtime serving
/// `@test/cmds` (declaring a tool, a plain resource and a templated one),
/// then `specforge.json` edited on disk to enable it: what the very next
/// request of each kind sees is the one freshness decision of ADR 0024.
fn extension_enabled_after_serving()
-> (Served, std::sync::Arc<crate::fake_extension::FakeExtension>) {
    use crate::fake_extension::{EXT, FakeExtension};
    let ext = std::sync::Arc::new(FakeExtension::new().with_resource(json!({
        "uri_template": "specforge://ext/cmds/{id}",
        "name": "cmds-by-id",
        "export": "mcp__by_id",
        "mime_type": "application/json"
    })));
    let server = TestProject::new()
        .enabling(&[])
        .file("main.spec", "")
        .serve_in(ext.runtime() as std::sync::Arc<dyn specforge_wasm::runtime::WasmRuntime>);
    // The project installs what its runtime serves, then enables it.
    specforge_installed::testing::install(server.root(), &*ext.runtime(), &[EXT]);
    server.write("specforge.json", &config(&[EXT]));
    (server, ext)
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "an extension enabled on disk since the last request is listed by the next listing of every kind"
)]
fn a_listing_brings_the_served_project_up_to_date() {
    let names = |reply: &Value, list: &str, key: &str| -> Vec<String> {
        reply["result"][list]
            .as_array()
            .unwrap_or_else(|| panic!("{reply}"))
            .iter()
            .map(|entry| entry[key].as_str().unwrap().to_string())
            .collect()
    };
    for (method, list, key, listed) in [
        ("tools/list", "tools", "name", "specforge.cmds.check"),
        (
            "resources/list",
            "resources",
            "uri",
            "specforge://ext/cmds/summary",
        ),
        (
            "resources/templates/list",
            "resourceTemplates",
            "uriTemplate",
            "specforge://ext/cmds/{id}",
        ),
    ] {
        let (mut server, _ext) = extension_enabled_after_serving();
        let reply = call(&mut server, method, json!({}));
        assert!(
            names(&reply, list, key).iter().any(|name| name == listed),
            "{method} names {listed} once it is enabled on disk: {reply}"
        );
    }
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a subscription to an extension resource enabled on disk since the last request is accepted"
)]
fn a_subscription_finds_an_extension_resource_enabled_on_disk() {
    let (mut server, _ext) = extension_enabled_after_serving();
    let uri = "specforge://ext/cmds/summary";
    let reply = call(&mut server, "resources/subscribe", json!({"uri": uri}));
    assert_eq!(reply["result"], json!({}), "{reply}");
    assert_eq!(server.state().subscriptions().subscribed(), [uri]);

    // Taken off the list again, the same lookup refuses it as `read` does.
    server.write("specforge.json", &config(&[]));
    let unsubscribed = call(&mut server, "resources/subscribe", json!({"uri": uri}));
    assert_eq!(unsubscribed["error"]["code"], -32002, "{unsubscribed}");
}

// --- P11, P12: the scope query ---

#[specforge_test(
    behavior = "serve_graph_resource",
    verify = "scope query parameter restricts to subgraph"
)]
fn scope_restricts_the_graph_to_a_subgraph() {
    let mut server = served();
    let nodes = |graph: &Value| graph["nodes"].as_array().unwrap().len();
    let (_, whole) = resource(&mut server, "specforge://graph");
    assert!(
        whole.get("schema").is_some(),
        "the full graph embeds its schema"
    );
    let (_, by_scope) = resource(&mut server, "specforge://graph?scope=alpha");
    assert!(nodes(&by_scope) < nodes(&whole));
    assert!(by_scope["schema_ref"].is_object(), "{by_scope}");
    // `root` stays its alias.
    let (_, by_root) = resource(&mut server, "specforge://graph?root=alpha");
    assert_eq!(by_root, by_scope);
}

#[test]
fn a_read_names_the_uri_the_client_read() {
    let mut server = served();
    for uri in [
        "specforge://graph",
        "specforge://graph?root=alpha",
        "specforge://graph/alpha",
        "specforge://context/alpha?kinds=behavior",
        "specforge://entities/behavior",
        "specforge://diagnostics",
    ] {
        let (content, _) = resource(&mut server, uri);
        assert_eq!(content["uri"], uri);
    }
}

// --- P13, P14: the graph views are the exports ---

/// What `specforge export` writes for `request`, over the project at `root`.
fn exported(root: &std::path::Path, request: &Request) -> String {
    let runtime = specforge_project::testing::component_runtime();
    let project = specforge_project::testing::compile_at(root, Some(runtime));
    let exported = export::export(&ProjectView::of(&project), request);
    exported.unwrap().into_value_and_notices().0
}

#[specforge_test(
    behavior = "expose_context_as_mcp_resource",
    verify = "the context resource is the context export of the same request"
)]
fn context_resource_is_its_export() {
    let mut server = served();
    let root = server.root().to_path_buf();
    for (uri, scope, kinds) in [
        ("specforge://context", None, vec![]),
        ("specforge://context/alpha", Some("alpha"), vec![]),
        ("specforge://context?root=alpha", Some("alpha"), vec![]),
    ] {
        let request = Request {
            format: Some(Format::Context),
            scope,
            kinds,
            ..Request::default()
        };
        let text = resource_text(&read_resource(&mut server, uri));
        assert_eq!(text, exported(&root, &request), "{uri}");
    }
}

#[specforge_test(
    behavior = "expose_brief_as_mcp_resource",
    verify = "the brief resource is the brief export of the same request"
)]
fn the_brief_resource_is_its_export() {
    let mut server = served();
    let root = server.root().to_path_buf();
    for (uri, kinds) in [
        ("specforge://brief", vec![]),
        ("specforge://brief?kinds=behavior", vec!["behavior"]),
    ] {
        let request = Request {
            format: Some(Format::Brief),
            kinds,
            ..Request::default()
        };
        let text = resource_text(&read_resource(&mut server, uri));
        assert_eq!(text, exported(&root, &request), "{uri}");
    }
}

#[specforge_test(
    behavior = "expose_graph_as_mcp_resource",
    verify = "specforge://graph resource returns full Graph Protocol JSON"
)]
fn the_graph_resource_is_the_export_text() {
    let mut server = served();
    let root = server.root().to_path_buf();
    let request = Request {
        format: Some(Format::Graph),
        ..Request::default()
    };
    let exported = exported(&root, &request);
    let text = resource_text(&read_resource(&mut server, "specforge://graph"));
    // Byte for byte what `specforge export --format graph` writes: a
    // client that hashes the text has one hash across the CLI and MCP.
    assert_eq!(text, exported);
}

#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "the entities resource lists what specforge.list lists for the kind"
)]
fn the_entities_resource_lists_what_list_lists() {
    let mut server = served();
    // `zeta` is declared before `alpha`: both answer them sorted by id.
    let (_, resource_rows) = resource(&mut server, "specforge://entities/behavior");
    let listed =
        tool(&mut server, "specforge.list", json!({"kind": "behavior"}))["entities"].clone();
    assert_eq!(resource_rows, listed);
    let ids: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["alpha", "zeta"]);
    let (_, none) = resource(&mut server, "specforge://entities/nothing");
    assert_eq!(none, json!([]));
}

#[specforge_test(
    behavior = "serve_graph_resource",
    verify = "an unknown query key, a repeated key or a malformed value is invalid_input naming the key"
)]
fn a_query_the_resource_cannot_read_is_refused_naming_its_key() {
    let mut server = served();
    for (uri, key) in [
        ("specforge://graph?scop=alpha", "scop"),
        ("specforge://graph?depth=two", "depth"),
        ("specforge://graph?depth=1&depth=2", "depth"),
        ("specforge://graph?scope=alpha&root=alpha", "root"),
        ("specforge://graph?max_tokens=-1", "max_tokens"),
        ("specforge://graph?kinds=behavior,", "kinds"),
        ("specforge://graph?scope=", "scope"),
        ("specforge://graph?depth", "depth"),
        ("specforge://context/alpha?root=zeta", "root"),
        ("specforge://context/alpha?scope=zeta", "scope"),
        ("specforge://brief?kind=behavior", "kind"),
        ("specforge://schema?scope=alpha", "scope"),
        ("specforge://diagnostics?verbose=1", "verbose"),
        ("specforge://entities/behavior?limit=1", "limit"),
    ] {
        let error = read_error(&mut server, uri);
        assert_eq!(error["code"], -32602, "{uri}: {error}");
        assert_eq!(error["data"]["code"], "invalid_input", "{uri}: {error}");
        assert_eq!(error["data"]["argument"], key, "{uri}: {error}");
        assert_eq!(error["data"]["uri"], uri, "{uri}: {error}");
    }
    // A percent-escape is decoded: two kinds, not one unknown kind.
    let (_, graph) = resource(&mut server, "specforge://graph?kinds=behavior%2Cfeature");
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 2);
    let (_, escaped) = resource(&mut server, "specforge://graph?sc%6Fpe=alpha");
    let (_, scoped) = resource(&mut server, "specforge://graph?scope=alpha");
    assert_eq!(escaped, scoped);
    // An escape that is no UTF-8 is refused, naming the key.
    let error = read_error(&mut server, "specforge://graph?scope=%ff");
    assert_eq!(error["data"]["argument"], "scope", "{error}");
}

#[specforge_test(
    behavior = "serve_graph_resource",
    verify = "a count in a resource query is read as a tool's count argument is"
)]
fn a_resource_count_is_read_as_a_tool_reads_one() {
    let mut server = served();
    let error = read_error(&mut server, "specforge://brief?depth=x");
    assert_eq!(
        error["message"], "depth must be a non-negative integer, got 'x'",
        "{error}"
    );
    assert_eq!(error["data"]["argument"], "depth", "{error}");
    // A string holding a count, after decoding, is that count.
    let (_, spaced) = resource(&mut server, "specforge://graph?scope=alpha&depth=%32");
    let (_, plain) = resource(&mut server, "specforge://graph?scope=alpha&depth=2");
    assert_eq!(spaced, plain);
}

#[specforge_test(
    behavior = "serve_graph_resource",
    verify = "an unknown query key, a repeated key or a malformed value is invalid_input naming the key"
)]
fn a_depth_without_a_scope_is_refused() {
    let mut server = served();
    let error = read_error(&mut server, "specforge://graph?depth=2");
    assert_eq!(error["code"], -32602, "{error}");
    assert_eq!(error["data"]["code"], "invalid_input", "{error}");
    assert_eq!(error["data"]["argument"], "scope", "{error}");
}

// --- P15, P16: the request's own schema ---

#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "each request method that needs a session refuses before initialize with -32600"
)]
fn every_session_method_needs_initialize() {
    let mut server = McpServer::new();
    for method in [
        "tools/list",
        "resources/list",
        "resources/templates/list",
        "prompts/list",
        "tools/call",
        "resources/read",
        "resources/subscribe",
        "resources/unsubscribe",
        "prompts/get",
    ] {
        let reply = call(&mut server, method, json!({}));
        assert_eq!(reply["error"]["code"], -32600, "{method}: {reply}");
        assert_eq!(
            reply["error"]["message"], "Server not initialized",
            "{method}"
        );
    }
    let reply = call(&mut server, "ping", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
    let reply = call(&mut server, "nope/method", json!({}));
    assert_eq!(reply["error"]["code"], -32601, "{reply}");
}

#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "missing required params produces -32602 Invalid params"
)]
fn arguments_must_be_an_object() {
    let mut server = served();
    let reply = call(
        &mut server,
        "tools/call",
        json!({"name": "specforge.stats", "arguments": []}),
    );
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    assert_eq!(
        reply["error"]["message"],
        "Invalid params: arguments must be an object"
    );
    let reply = call(
        &mut server,
        "prompts/get",
        json!({"name": "specforge://prompts/context", "arguments": []}),
    );
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    assert_eq!(
        reply["error"]["message"],
        "Invalid params: arguments must be an object"
    );
    let reply = call(&mut server, "resources/read", json!({}));
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    assert_eq!(reply["error"]["message"], "Missing required parameter: uri");
    for method in ["tools/call", "prompts/get"] {
        let reply = call(&mut server, method, json!({}));
        assert_eq!(reply["error"]["code"], -32602, "{reply}");
        assert_eq!(
            reply["error"]["message"],
            "Missing required parameter: name"
        );
    }
}

// --- P17: the arguments a target declares ---

#[test]
fn target_argument_descriptions() {
    let tools = core_tools();
    let described = |argument: &str| -> Vec<(String, String)> {
        tools
            .iter()
            .filter_map(|tool| {
                let property = &tool.input_schema["properties"][argument];
                Some((tool.name.clone(), property["description"].as_str()?.into()))
            })
            .collect()
    };

    // The target declares `path` only: no tool lists use_cached.
    assert!(described("use_cached").is_empty());

    let paths = described("path");
    let (init, others): (Vec<_>, Vec<_>) =
        paths.iter().partition(|(name, _)| name == "specforge.init");
    assert_eq!(others.len(), 9, "{paths:?}");
    for (name, text) in &others {
        assert_eq!(
            text, "Project root path (the served project if omitted)",
            "{name}"
        );
    }
    assert_eq!(
        init[0].1,
        "Directory for the new project, outside the current one"
    );
    let init_tool = tools.iter().find(|t| t.name == "specforge.init").unwrap();
    assert_eq!(init_tool.input_schema["required"], json!(["path"]));
}

// --- P18: the events of each request kind ---

#[specforge_test(
    behavior = "serve_mcp_prompt",
    verify = "an unknown prompt records no mcp_prompt_invoked event"
)]
fn events_per_request_kind() {
    let mut server = served();
    let base = event_names(&server).len();

    // A tool: the invocation first, even when its target refuses.
    let names = recorded(&mut server, |s| {
        call_tool(s, "specforge.stats", json!({}));
    });
    assert_eq!(
        names.first().map(String::as_str),
        Some("mcp_tool_invoked"),
        "{names:?}"
    );
    let names = recorded(&mut server, |s| {
        let refused = call_tool(s, "specforge.stats", json!({"path": "nope"}));
        assert_eq!(tool_error(&refused)["code"], "file_not_found");
    });
    assert_eq!(names, ["mcp_tool_invoked"]);
    let names = recorded(&mut server, |s| {
        call_tool(s, "specforge.no_such_tool", json!({}));
    });
    assert!(
        !names.iter().any(|n| n == "mcp_tool_invoked"),
        "an unknown tool is no invocation: {names:?}"
    );

    // A resource: one read event, only for a read that returned content.
    let names = recorded(&mut server, |s| {
        read_resource(s, "specforge://graph");
    });
    assert_eq!(names, ["mcp_resource_read"]);
    let read = events(&server, "mcp_resource_read");
    assert_eq!(
        read.last().unwrap(),
        &json!({"resourceUri": "specforge://graph", "format": "application/json"})
    );
    let names = recorded(&mut server, |s| {
        read_resource(s, "specforge://graph?root=ghost");
        read_resource(s, "specforge://nope");
    });
    assert!(names.is_empty(), "{names:?}");

    // A prompt: the invocation, none for an unknown one.
    let names = recorded(&mut server, |s| {
        get_prompt(
            s,
            "specforge://prompts/context",
            json!({"entity_id": "alpha"}),
        );
    });
    assert_eq!(names, ["mcp_prompt_invoked"]);
    let names = recorded(&mut server, |s| {
        get_prompt(s, "specforge://prompts/nope", json!({}));
    });
    assert!(names.is_empty(), "{names:?}");
    assert!(event_names(&server).len() > base);
}

// --- P19: one rule says what a change touches ---

#[specforge_test(
    behavior = "listen_for_mcp_resource_updates",
    verify = "a recompile that changes a listened resource sends resources/updated with the subscription id"
)]
fn listen_and_subscribe_watch_by_one_rule() {
    let mut server = served();
    let (context, diagnostics) = ("specforge://context", "specforge://diagnostics");
    for uri in [context, diagnostics] {
        let reply = call(&mut server, "resources/subscribe", json!({"uri": uri}));
        assert_eq!(reply["result"], json!({}), "{reply}");
    }
    // A listen stream has no response; its acknowledgement is queued.
    let listen = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "subscriptions/listen",
        "params": modern(json!({"notifications": {"resourceSubscriptions": [context, diagnostics]}})),
    });
    assert!(server.handle_message(&listen.to_string()).is_none());
    let acknowledged = server.take_notifications();
    assert_eq!(
        acknowledged[0]["method"], "notifications/subscriptions/acknowledged",
        "{acknowledged:?}"
    );

    let methods = |notifications: &[Value]| -> Vec<(String, String)> {
        notifications
            .iter()
            .map(|n| {
                (
                    n["method"].as_str().unwrap().to_string(),
                    n["params"]["uri"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect()
    };

    // A renamed title changes the graph, not what is reported about it.
    server.write(
        "main.spec",
        &SOURCE.replace("\"Alpha\"", "\"Alpha, renamed\""),
    );
    call_tool(&mut server, "specforge.stats", json!({}));
    let sent = server.take_notifications();
    assert_eq!(
        methods(&sent),
        [
            (
                "notifications/resources/updated".to_string(),
                context.to_string()
            ),
            (
                "notifications/resources/updated".to_string(),
                context.to_string()
            ),
            ("specforge/graphChanged".to_string(), String::new()),
        ],
        "{sent:?}"
    );

    // A source that does not parse changes the diagnostics, not the graph.
    server.write("broken.spec", "behavior {\n");
    call_tool(&mut server, "specforge.stats", json!({}));
    let sent = server.take_notifications();
    assert_eq!(
        methods(&sent),
        [
            (
                "notifications/resources/updated".to_string(),
                diagnostics.to_string()
            ),
            (
                "notifications/resources/updated".to_string(),
                diagnostics.to_string()
            ),
            ("specforge/diagnosticsChanged".to_string(), String::new()),
        ],
        "{sent:?}"
    );
    assert_eq!(
        sent[0]["params"]["_meta"]["io.modelcontextprotocol/subscriptionId"],
        1
    );
    assert_eq!(
        server.state().subscriptions().subscribed(),
        ["specforge://context", "specforge://diagnostics"]
    );
}
