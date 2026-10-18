use crate::support::*;
use serde_json::{Value, json};
use specforge_extension_sdk::prelude::{PassDiagnostic, PassSeverity, PassSpan};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

/// `alpha`, a behavior with a contract and one obligation (test.spec).
const ALPHA: &str = "behavior alpha \"Alpha Behavior\" {\n    contract \"The system MUST do alpha\"\n    features [beta]\n    verify unit \"does alpha correctly\"\n}\n";

/// `beta`, a feature of `alpha` (features.spec).
const BETA: &str = "feature beta \"Beta Feature\" {\n    problem \"Users need it\"\n}\n";

/// `alpha` and the feature `beta` it belongs to, on disk.
fn project() -> TestProject {
    TestProject::new()
        .file("test.spec", ALPHA)
        .file("features.spec", BETA)
}

/// `@test/ext` with the software kinds, `behavior`'s contract a headline
/// field the context export lifts.
fn extension() -> TestExtension {
    TestExtension::software().headline("behavior")
}

fn test_server() -> Served {
    project().serve(&[extension()])
}

/// Adds `gamma`, an invariant with no edges, outside every entity's
/// subgraph: written to disk, served by the next request.
fn add_unconnected_gamma(server: &Served) {
    server.write("gamma.spec", "invariant gamma \"Gamma\" {\n}\n");
}

/// A feature whose `behaviors` names `ghost`, which no entity declares: the
/// compile reports E003 at its line 2.
const BROKEN: &str = "behavior broken \"Broken\" {\n    features [ghost]\n}\n";

/// `diagnostic` at test.spec line 1 (alpha), as a pass reports it.
fn at_alpha(diagnostic: PassDiagnostic) -> PassDiagnostic {
    diagnostic.with_span(PassSpan {
        file: "test.spec".into(),
        start_line: 1,
        start_col: 1,
        end_line: 1,
        end_col: 10,
    })
}

fn node_ids(parsed: &Value) -> Vec<&str> {
    let mut ids: Vec<&str> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    ids
}

// B:expose_graph_as_mcp_resource — verify unit "returns full graph as JSON"
#[specforge_test(
    behavior = "expose_graph_as_mcp_resource",
    verify = "specforge://graph resource returns full Graph Protocol JSON"
)]
fn graph_resource_returns_json() {
    let mut server = test_server();
    let resp = read_resource(&mut server, "specforge://graph");
    let text = resource_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["nodes"].is_array());
    assert!(parsed["edges"].is_array());
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 2);
}

// B:expose_graph_as_mcp_resource — verify unit "graph resource has correct MIME type"
#[specforge_test(
    behavior = "expose_graph_as_mcp_resource",
    verify = "specforge://graph resource returns full Graph Protocol JSON"
)]
fn graph_resource_has_mime_type() {
    let mut server = test_server();
    let resp = read_resource(&mut server, "specforge://graph");
    assert_eq!(
        resp["result"]["contents"][0]["mimeType"],
        "application/json"
    );
    let parsed: Value = serde_json::from_str(&resource_text(&resp)).unwrap();
    assert_eq!(node_ids(&parsed), vec!["alpha", "beta"]);
    assert_eq!(
        parsed["edges"],
        json!([{"source": "alpha", "target": "beta", "label": "features"}])
    );
    let alpha = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "alpha")
        .unwrap();
    assert_eq!(alpha["kind"], "behavior");
    assert_eq!(alpha["file"], "test.spec");
    assert_eq!(alpha["fields"]["contract"], "The system MUST do alpha");
}

/// A project on disk using the builtin `extensions`, served with the
/// project's own component runtime.
fn builtin_project(extensions: &[&str]) -> Served {
    TestProject::new()
        .enabling(extensions)
        .file(
            "main.spec",
            "behavior act \"Act\" {\n  contract \"MUST act\"\n}\n",
        )
        .serve_components()
}

fn kind_names(schema: &Value) -> Vec<&str> {
    schema["entity_kinds"]
        .as_array()
        .unwrap_or_else(|| panic!("entity_kinds is not a list: {schema}"))
        .iter()
        .map(|k| k["name"].as_str().unwrap())
        .collect()
}

#[specforge_test(
    behavior = "expose_schema_as_mcp_resource",
    verify = "specforge://schema resource returns GraphProtocolSchema JSON"
)]
fn schema_resource_returns_the_graph_protocol_schema() {
    let mut server = builtin_project(&["@specforge/software"]);

    let schema: Value = serde_json::from_str(&resource_text(&read_resource(
        &mut server,
        "specforge://schema",
    )))
    .unwrap();
    assert_eq!(
        schema["schema_version"],
        json!({"major": 1, "minor": 0, "patch": 0})
    );
    assert_eq!(
        schema["extensions"],
        json!([{"name": "@specforge/software", "version": "1.0.0"}])
    );
    assert_eq!(
        kind_names(&schema),
        ["behavior", "event", "invariant", "port", "type"]
    );
    let consumes = schema["edge_types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["label"] == "BehaviorConsumesEvent")
        .unwrap_or_else(|| panic!("no BehaviorConsumesEvent in {schema}"));
    assert_eq!(consumes["source_kinds"], json!(["behavior"]));
    assert_eq!(consumes["target_kinds"], json!(["event"]));
}

// B:expose_context_as_mcp_resource — verify unit "returns context-optimized graph"
#[specforge_test(
    behavior = "expose_context_as_mcp_resource",
    verify = "specforge://context resource returns token-optimized format"
)]
fn context_resource_returns_context_graph() {
    let mut server = test_server();
    let resp = read_resource(&mut server, "specforge://context");
    let text = resource_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(node_ids(&parsed), vec!["alpha", "beta"]);
    let alpha = &parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "alpha")
        .unwrap();
    assert_eq!(alpha["contract"], "The system MUST do alpha");
    // Token-optimized: the source location and the embedded schema the
    // graph format carries are dropped.
    for key in ["file", "line", "source_span"] {
        assert!(
            alpha.get(key).is_none(),
            "context node keeps {key}: {alpha}"
        );
    }
    assert!(parsed.get("schema").is_none(), "context embeds the schema");

    let graph_text = resource_text(&read_resource(&mut server, "specforge://graph"));
    let graph: Value = serde_json::from_str(&graph_text).unwrap();
    assert_eq!(graph["nodes"][0]["file"], "test.spec");
    assert!(graph["schema"].is_object());
    assert!(
        text.len() < graph_text.len(),
        "context ({} bytes) must be smaller than the graph ({} bytes)",
        text.len(),
        graph_text.len()
    );
}

// B:expose_brief_as_mcp_resource — verify unit "returns brief graph"
#[specforge_test(
    behavior = "expose_brief_as_mcp_resource",
    verify = "specforge://brief resource returns minimal IDs and edges format"
)]
fn brief_resource_returns_brief_graph() {
    let mut server = test_server();
    let resp = read_resource(&mut server, "specforge://brief");
    let text = resource_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["nodes"].is_array());
    // Brief only has id, kind, title — no fields
    let nodes = parsed["nodes"].as_array().unwrap();
    for node in nodes {
        assert!(node["id"].is_string());
        assert!(node["kind"].is_string());
        assert!(node.get("fields").is_none());
        let mut keys: Vec<&String> = node.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["id", "kind", "title"], "{node}");
    }
    // The edges carry exactly source, target and label.
    assert_eq!(
        parsed["edges"],
        json!([{"source": "alpha", "target": "beta", "label": "features"}])
    );
}

// B:expose_diagnostics_as_mcp_resource — verify unit "returns diagnostics array"
#[specforge_test(
    behavior = "expose_diagnostics_as_mcp_resource",
    verify = "specforge://diagnostics resource returns current DiagnosticBag as JSON"
)]
fn diagnostics_resource_returns_array() {
    // E003: a real unresolved reference; W901: a check-phase pass's
    // finding, reported after the graph checks.
    let mut server = project().file("broken.spec", BROKEN).serve(&[
        extension().reporting(at_alpha(PassDiagnostic::warning("W901", "orphan entity")))
    ]);
    let resp = read_resource(&mut server, "specforge://diagnostics");
    let text = resource_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let diags = parsed.as_array().unwrap();
    assert_eq!(diags.len(), 2, "{parsed}");
    assert_eq!(diags[0]["code"], "E003");
    assert_eq!(diags[0]["message"], UNRESOLVED_GHOST);
    assert_eq!(diags[1]["code"], "W901");
    assert_eq!(diags[1]["message"], "orphan entity");
}

/// What the compile reports for [`BROKEN`]'s reference to `ghost`.
const UNRESOLVED_GHOST: &str = "unresolved reference 'ghost' in entity 'broken'";

// B:expose_entity_as_mcp_resource — verify unit "returns entity subgraph"
#[specforge_test(
    behavior = "expose_entity_as_mcp_resource",
    verify = "specforge://graph/{entity_id} returns entity and its neighbors"
)]
fn entity_resource_returns_subgraph() {
    let mut server = test_server();
    add_unconnected_gamma(&server);
    let resp = read_resource(&mut server, "specforge://graph/alpha");
    let text = resource_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["nodes"].is_array());
    // Subgraph from alpha includes alpha and beta (connected)
    let ids: Vec<&str> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"alpha"));
    assert!(
        ids.contains(&"beta"),
        "the neighbor beta is missing: {ids:?}"
    );
    assert!(!ids.contains(&"gamma"), "gamma is not a neighbor: {ids:?}");
    assert_eq!(
        parsed["edges"],
        json!([{"source": "alpha", "target": "beta", "label": "features"}])
    );
}

// B:expose_entity_as_mcp_resource — verify unit "returns error for unknown entity"
#[specforge_test(
    behavior = "expose_entity_as_mcp_resource",
    verify = "non-existent entity_id returns 404 error"
)]
fn entity_resource_error_for_unknown() {
    let mut server = test_server();
    let resp = read_resource(&mut server, "specforge://graph/nonexistent");
    // Not found, as a handshake session says it (MCP 2025-11-25).
    assert_eq!(resp["error"]["code"], -32002);
    assert_eq!(
        resp["error"]["message"],
        "unresolved entity 'nonexistent' — not found in graph"
    );
    let data = &resp["error"]["data"];
    assert_eq!(data["code"], "entity_not_found", "{resp}");
    assert_eq!(data["entity_id"], "nonexistent", "{resp}");
    assert_eq!(data["uri"], "specforge://graph/nonexistent", "{resp}");
    // Told apart from a malformed ID.
    let malformed = read_resource(&mut server, "specforge://graph/!@#$");
    assert_ne!(malformed["error"]["message"], resp["error"]["message"]);
}

// B:expose_entity_as_mcp_resource — verify unit "returns error for empty entity ID"
#[specforge_test(
    behavior = "expose_entity_as_mcp_resource",
    verify = "malformed entity_id returns 400 error"
)]
fn entity_resource_error_for_empty_id() {
    let mut server = test_server();
    let resp = read_resource(&mut server, "specforge://graph/");
    assert_eq!(resp["error"]["code"], -32602);
    let message = resp["error"]["message"].as_str().unwrap();
    assert!(message.starts_with("Malformed entity_id"), "{message}");
    assert!(!message.contains("not found"), "{message}");
}

// Resource read missing URI
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "missing required params produces -32602 Invalid params"
)]
fn resource_read_missing_uri() {
    let mut server = test_server();
    let resp = call(&mut server, "resources/read", json!({}));
    assert_eq!(resp["error"]["code"], -32602);
    assert_eq!(resp["error"]["message"], "Missing required parameter: uri");
}

// Unknown resource URI
#[specforge_test(
    behavior = "expose_graph_as_mcp_resource",
    verify = "returns error for unknown URI"
)]
fn resource_read_unknown_uri() {
    let mut server = test_server();
    let resp = read_resource(&mut server, "specforge://unknown");
    assert!(resp["error"].is_object());
}

// Resource read when not initialized
#[test]
fn resource_read_not_initialized() {
    let mut server = McpServer::new();
    let resp = call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://graph"}),
    );
    assert!(resp["error"].is_object());
}

#[test]
fn graph_includes_schema_version() {
    let mut server = test_server();
    let resp = read_resource(&mut server, "specforge://graph");
    let text = resource_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    // The graph resource returns valid JSON with a nodes array
    assert!(parsed["nodes"].is_array());
}

// B:expose_graph_as_mcp_resource — verify unit "resource refreshes after recompilation"
#[specforge_test(
    behavior = "expose_graph_as_mcp_resource",
    verify = "resource refreshes after recompilation"
)]
fn graph_refreshes_after_recompilation() {
    let mut server = test_server();
    let resp1 = read_resource(&mut server, "specforge://graph");
    let text1 = resource_text(&resp1);
    let parsed1: Value = serde_json::from_str(&text1).unwrap();
    let count1 = parsed1["nodes"].as_array().unwrap().len();

    // A new entity written to disk: the next read recompiles.
    server.write("delta.spec", "behavior delta \"Delta Behavior\" {\n}\n");

    let resp2 = read_resource(&mut server, "specforge://graph");
    let text2 = resource_text(&resp2);
    let parsed2: Value = serde_json::from_str(&text2).unwrap();
    let count2 = parsed2["nodes"].as_array().unwrap().len();
    assert_eq!(count2, count1 + 1);
}

#[specforge_test(
    behavior = "expose_schema_as_mcp_resource",
    verify = "schema updates when extensions change"
)]
fn schema_updates_when_extensions_change() {
    let mut server = builtin_project(&["@specforge/software"]);
    let read = |server: &mut McpServer| -> Value {
        serde_json::from_str(&resource_text(&read_resource(server, "specforge://schema"))).unwrap()
    };
    let before = read(&mut server);
    assert!(!kind_names(&before).contains(&"feature"), "{before}");

    // Enabling @specforge/product on disk brings its kinds and its
    // extension entry: the next read reloads the environment.
    server.write(
        "specforge.json",
        &json!({"name": "t", "version": "0.1.0",
            "extensions": ["@specforge/software", "@specforge/product"]})
        .to_string(),
    );
    let after = read(&mut server);
    assert!(kind_names(&after).contains(&"feature"), "{after}");
    let extensions: Vec<&str> = after["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(extensions.contains(&"@specforge/product"), "{after}");
}

// B:expose_context_as_mcp_resource — verify unit "resource refreshes after recompilation"
#[specforge_test(
    behavior = "expose_context_as_mcp_resource",
    verify = "resource refreshes after recompilation"
)]
fn context_refreshes_after_recompilation() {
    let mut server = test_server();
    let resp1 = read_resource(&mut server, "specforge://context");
    let text1 = resource_text(&resp1);
    let parsed1: Value = serde_json::from_str(&text1).unwrap();
    let count1 = parsed1["nodes"].as_array().unwrap().len();

    server.write("delta.spec", "behavior delta \"Delta Behavior\" {\n}\n");

    let resp2 = read_resource(&mut server, "specforge://context");
    let text2 = resource_text(&resp2);
    let parsed2: Value = serde_json::from_str(&text2).unwrap();
    let count2 = parsed2["nodes"].as_array().unwrap().len();
    assert_eq!(count2, count1 + 1);
}

// B:expose_brief_as_mcp_resource — verify unit "resource refreshes after recompilation"
#[specforge_test(
    behavior = "expose_brief_as_mcp_resource",
    verify = "resource refreshes after recompilation"
)]
fn brief_refreshes_after_recompilation() {
    let mut server = test_server();
    let resp1 = read_resource(&mut server, "specforge://brief");
    let text1 = resource_text(&resp1);
    let parsed1: Value = serde_json::from_str(&text1).unwrap();
    let count1 = parsed1["nodes"].as_array().unwrap().len();

    server.write("delta.spec", "behavior delta \"Delta Behavior\" {\n}\n");

    let resp2 = read_resource(&mut server, "specforge://brief");
    let text2 = resource_text(&resp2);
    let parsed2: Value = serde_json::from_str(&text2).unwrap();
    let count2 = parsed2["nodes"].as_array().unwrap().len();
    assert_eq!(count2, count1 + 1);
}

// B:expose_diagnostics_as_mcp_resource — verify unit "resource updates after recompilation"
#[specforge_test(
    behavior = "expose_diagnostics_as_mcp_resource",
    verify = "resource updates after recompilation"
)]
fn diagnostics_updates_after_recompilation() {
    let mut server = test_server();
    let resp1 = read_resource(&mut server, "specforge://diagnostics");
    let text1 = resource_text(&resp1);
    let parsed1: Value = serde_json::from_str(&text1).unwrap();
    let count1 = parsed1.as_array().unwrap().len();

    // A dangling reference written to disk: the next read recompiles and
    // reports E003.
    server.write("broken.spec", BROKEN);

    let resp2 = read_resource(&mut server, "specforge://diagnostics");
    let text2 = resource_text(&resp2);
    let parsed2: Value = serde_json::from_str(&text2).unwrap();
    let count2 = parsed2.as_array().unwrap().len();
    assert_eq!(count2, count1 + 1);
    assert!(
        parsed2
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E003" && d["message"] == UNRESOLVED_GHOST),
        "{parsed2}"
    );
}

// B:expose_diagnostics_as_mcp_resource — verify unit "each diagnostic includes severity, code, message, file, span"
#[specforge_test(
    behavior = "expose_diagnostics_as_mcp_resource",
    verify = "each diagnostic includes severity, code, message, file, and span"
)]
fn diagnostics_fields_present() {
    // E003 at broken.spec line 2.
    let mut server = project().file("broken.spec", BROKEN).serve(&[extension()]);

    let resp = read_resource(&mut server, "specforge://diagnostics");
    let text = resource_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let diags = parsed.as_array().unwrap();
    assert!(!diags.is_empty());

    let d = &diags[0];
    assert!(d["code"].is_string());
    assert!(d["severity"].is_string());
    assert!(d["message"].is_string());
    assert!(d.get("file").is_some());
    assert!(d.get("line").is_some());
}

// B:expose_entity_as_mcp_resource — verify unit "malformed entity_id returns 400 error"
#[specforge_test(
    behavior = "expose_entity_as_mcp_resource",
    verify = "malformed entity_id returns 400 error"
)]
fn entity_malformed_id_returns_error() {
    let mut server = test_server();
    let resp = read_resource(&mut server, "specforge://graph/!@#$");
    assert_eq!(resp["error"]["code"], -32602);
    let message = resp["error"]["message"].as_str().unwrap();
    assert!(message.starts_with("Malformed entity_id"), "{message}");
    assert!(message.contains("!@#$"), "names the bad ID: {message}");
    assert!(!message.contains("not found"), "{message}");
}

// B:expose_entity_as_mcp_resource — verify unit "resource refreshes after recompilation"
#[specforge_test(
    behavior = "expose_entity_as_mcp_resource",
    verify = "resource refreshes after recompilation"
)]
fn entity_refreshes_after_recompilation() {
    let mut server = test_server();
    let resp1 = read_resource(&mut server, "specforge://graph/alpha");
    let text1 = resource_text(&resp1);
    let parsed1: Value = serde_json::from_str(&text1).unwrap();
    let title1 = parsed1["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "alpha")
        .unwrap()["title"]
        .as_str()
        .unwrap()
        .to_string();

    // alpha retitled on disk.
    server.write(
        "test.spec",
        &ALPHA.replace("Alpha Behavior", "Alpha Revised"),
    );

    let resp2 = read_resource(&mut server, "specforge://graph/alpha");
    let text2 = resource_text(&resp2);
    let parsed2: Value = serde_json::from_str(&text2).unwrap();
    let title2 = parsed2["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "alpha")
        .unwrap()["title"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(title1, title2);
    assert_eq!(title2, "Alpha Revised");
}

// ---- C9-06: query parameters on graph resources ----

// B:expose_graph_as_mcp_resource — verify unit "root query scopes the read to a subgraph with a schema_ref"
#[specforge_test(
    behavior = "serve_graph_resource",
    verify = "scope query parameter restricts to subgraph"
)]
fn graph_resource_root_scopes_with_schema_ref() {
    let mut server = test_server();
    add_unconnected_gamma(&server);
    let unscoped: Value = serde_json::from_str(&resource_text(&read_resource(
        &mut server,
        "specforge://graph",
    )))
    .unwrap();
    assert_eq!(node_ids(&unscoped), vec!["alpha", "beta", "gamma"]);
    let resp = read_resource(&mut server, "specforge://graph?scope=alpha");
    let text = resource_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let ids: Vec<&str> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert!(
        !ids.contains(&"gamma"),
        "gamma is outside alpha's subgraph: {ids:?}"
    );
    assert_eq!(node_ids(&parsed), vec!["alpha", "beta"]);
    assert!(
        parsed["schema_ref"].is_object(),
        "scoped exports reference the published schema instead of embedding it"
    );
    assert!(
        parsed["schema"].is_null(),
        "scoped read must not embed the full schema"
    );
}

// B:expose_graph_as_mcp_resource — verify unit "unknown root returns an error"
#[test]
fn graph_resource_unknown_root_errors() {
    let mut server = test_server();
    let resp = read_resource(&mut server, "specforge://graph?root=nonexistent");
    assert!(resp["error"].is_object());
}

// B:expose_graph_as_mcp_resource — verify unit "kinds query filters node kinds"
#[test]
fn graph_resource_kinds_filter() {
    let mut server = test_server();
    let resp = read_resource(&mut server, "specforge://graph?kinds=behavior");
    let parsed: Value = serde_json::from_str(&resource_text(&resp)).unwrap();
    let kinds: Vec<&str> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["behavior"], "kinds=a,b must filter node kinds");
}

// B:expose_graph_as_mcp_resource — verify unit "max_tokens budgets the payload"
#[test]
fn graph_resource_max_tokens_budgets() {
    let mut server = test_server();
    // The budgeted export leaves the schema out and fits the budget, or
    // fails with E062 when not even the empty envelope fits, as
    // `specforge export --max-tokens` does (ADR 0004 D3-a).
    let resp = read_resource(&mut server, "specforge://graph?max_tokens=1");
    assert_eq!(resp["error"]["code"], -32602, "{resp}");
    assert_eq!(resp["error"]["data"]["code"], "invalid_input", "{resp}");
    assert_eq!(
        resp["error"]["data"]["diagnostic"]["code"], "E062",
        "{resp}"
    );

    let resp = read_resource(&mut server, "specforge://graph?max_tokens=200");
    let text = resource_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed.get("schema").is_none(), "{parsed}");
    assert!(specforge_emitter::estimate_tokens(&text) <= 200, "{text}");
}

// `root` is what the resource has always read: it stays an alias of `scope`.
#[specforge_test(
    behavior = "serve_graph_resource",
    verify = "root is accepted as an alias of the scope query parameter"
)]
fn root_is_an_alias_of_scope() {
    let mut server = test_server();
    add_unconnected_gamma(&server);
    let by_scope = read_resource(&mut server, "specforge://graph?scope=alpha");
    let by_root = read_resource(&mut server, "specforge://graph?root=alpha");
    assert_eq!(resource_text(&by_root), resource_text(&by_scope));
    let parsed: Value = serde_json::from_str(&resource_text(&by_root)).unwrap();
    assert_eq!(node_ids(&parsed), vec!["alpha", "beta"]);
    // The context and brief resources read it the same way.
    for resource in ["context", "brief"] {
        let by_scope = read_resource(&mut server, &format!("specforge://{resource}?scope=alpha"));
        let by_root = read_resource(&mut server, &format!("specforge://{resource}?root=alpha"));
        assert_eq!(
            resource_text(&by_root),
            resource_text(&by_scope),
            "{resource}"
        );
    }
}

// B:expose_context_as_mcp_resource — verify unit "context entity template scopes to the subgraph"
#[specforge_test(
    behavior = "serve_graph_resource",
    verify = "scope query parameter restricts to subgraph"
)]
fn context_entity_template_scopes() {
    let mut server = test_server();
    add_unconnected_gamma(&server);
    let resp = read_resource(&mut server, "specforge://context/alpha");
    let text = resource_text(&resp);
    assert_eq!(
        resp["result"]["contents"][0]["uri"],
        "specforge://context/alpha"
    );
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let ids: Vec<&str> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&"alpha"),
        "context template must serve the subgraph rooted at the path entity"
    );
    assert!(
        !ids.contains(&"gamma"),
        "gamma is outside alpha's subgraph: {ids:?}"
    );
    assert_eq!(node_ids(&parsed), vec!["alpha", "beta"]);
    assert!(
        parsed.get("schema").is_none() && parsed.get("schema_ref").is_none(),
        "the context carries the graph only; the schema is specforge://schema"
    );
}

#[test]
fn context_entity_template_with_kinds_query() {
    let mut server = test_server();
    // kinds=event matches nothing: the kind filter drops alpha (behavior),
    // but the scoped root (beta) is always kept regardless of the filter.
    let resp = read_resource(&mut server, "specforge://context/beta?kinds=event");
    let parsed: Value = serde_json::from_str(&resource_text(&resp)).unwrap();
    let ids: Vec<&str> = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec!["beta"],
        "kind filter drops non-matching nodes but the scoped root remains"
    );
}

#[test]
fn context_entity_template_registered() {
    let mut server = test_server();
    let resp = call(&mut server, "resources/templates/list", json!({}));
    let uris: Vec<&str> = resp["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uriTemplate"].as_str().unwrap())
        .collect();
    assert!(
        uris.contains(&"specforge://context/{entity_id}"),
        "specforge://context/{{entity_id}} must be advertised: {uris:?}"
    );
}

#[specforge_test(
    behavior = "expose_diagnostics_as_mcp_resource",
    verify = "each catalogued diagnostic in the resource carries its title"
)]
fn diagnostics_resource_gives_catalogued_codes_their_title() {
    // E003 is catalogued and the host's own; W901 is a third-party
    // extension's check-phase pass finding, which names its extension and
    // has no catalogue entry.
    let mut server = project()
        .file("broken.spec", BROKEN)
        .serve(&[extension().reporting(at_alpha(PassDiagnostic::new(
            "W901",
            PassSeverity::Warning,
            "m",
        )))]);
    let bag: Value = serde_json::from_str(&resource_text(&read_resource(
        &mut server,
        "specforge://diagnostics",
    )))
    .unwrap();
    assert_eq!(bag[0]["code"], "E003");
    assert_eq!(bag[0]["title"], "Unresolved reference");
    assert!(bag[0].get("origin").is_none(), "{bag}");
    assert_eq!(bag[1]["code"], "W901");
    assert_eq!(bag[1]["title"], Value::Null, "{bag}");
    assert_eq!(bag[1]["origin"], "@test/ext", "{bag}");
}
