use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

/// The project the core tools read: `alpha`, a behavior with a contract
/// and one obligation; `beta_feature`, the feature that has it; and
/// `gamma_orphan`, an invariant with no edge and no obligation.
const SOURCES: &[(&str, &str)] = &[
    (
        "test.spec",
        "behavior alpha \"Alpha Behavior\" {\n    contract \"The system MUST do alpha\"\n    features [beta_feature]\n    verify unit \"does alpha correctly\"\n}\n",
    ),
    (
        "features.spec",
        "feature beta_feature \"Beta Feature\" {\n    problem \"Users need it\"\n}\n",
    ),
    (
        "invariants.spec",
        "invariant gamma_orphan \"Gamma Orphan\" {\n}\n",
    ),
];

/// [`SOURCES`] on disk, before a test's own files.
fn project() -> TestProject {
    SOURCES
        .iter()
        .fold(TestProject::new(), |project, (path, text)| {
            project.file(path, text)
        })
}

/// `@test/ext` with the software kinds, as @specforge/testing obligates
/// them: a testable behavior or invariant must declare obligations (W004),
/// so its entities count toward coverage.
fn extension() -> TestExtension {
    TestExtension::software()
        .obligating("behavior")
        .obligating("invariant")
}

fn test_server() -> Served {
    project().serve(&[extension()])
}

/// A server that serves no project: initialize names none. A call that
/// names a project by `path` is then served that project.
fn serving_nothing() -> McpServer {
    let mut server = McpServer::new();
    let reply = call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
    server
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "testable is the kind's testability and declared says whether the entity has obligations"
)]
fn inspect_testable_is_the_kinds_and_declared_is_the_entitys() {
    let mut server = test_server();
    let mut inspect = |id: &str| {
        let resp = call_tool(&mut server, "specforge.inspect", json!({"entity_id": id}));
        let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
        (parsed["testable"].clone(), parsed["declared"].clone())
    };
    // A behavior that declares an obligation.
    assert_eq!(inspect("alpha"), (json!(true), json!(true)));
    // An invariant (a testable kind) that declares none.
    assert_eq!(inspect("gamma_orphan"), (json!(true), json!(false)));
    // A feature: its kind is not testable.
    assert_eq!(inspect("beta_feature"), (json!(false), json!(false)));
}

/// [`test_server`] with a testable kind no rule obligates (`constraint`),
/// a union of one (`type`), and an entity of a kind no extension declares.
fn standings_server() -> Served {
    project()
        .file(
            "standings.spec",
            "constraint free_one \"Free\" {\n}\ntype Choice = yes | no\ngizmo thing \"Thing\" {\n}\n",
        )
        .serve(&[extension().kind("constraint", true).kind("type", true)])
}

const STANDING_IDS: [&str; 6] = [
    "alpha",
    "beta_feature",
    "gamma_orphan",
    "free_one",
    "Choice",
    "thing",
];

fn inspected(server: &mut McpServer, id: &str) -> Value {
    let resp = call_tool(server, "specforge.inspect", json!({"entity_id": id}));
    tool_json(&resp)
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "exempt says the entity does not count toward coverage, as specforge.coverage's row says"
)]
fn inspect_exempt_is_the_coverage_rows() {
    let mut server = standings_server();
    let mut exempt = Vec::new();
    for id in STANDING_IDS {
        let inspect = inspected(&mut server, id);
        let resp = call_tool(&mut server, "specforge.coverage", json!({"entity_id": id}));
        let rows = tool_json(&resp)["entities"].clone();
        let row = &rows.as_array().unwrap_or_else(|| panic!("{rows}"))[0];
        assert_eq!(inspect["exempt"], row["exempt"], "{id}");
        exempt.push((id, inspect["exempt"].as_bool().unwrap()));
    }
    assert_eq!(
        exempt,
        [
            ("alpha", false),
            ("beta_feature", false),
            ("gamma_orphan", false),
            ("free_one", true),
            ("Choice", true),
            ("thing", false),
        ]
    );
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "obligated says whether the entity's kind must declare obligations"
)]
fn inspect_obligated_follows_the_rule_set() {
    let mut server = standings_server();
    // W004 targets behavior and invariant.
    assert_eq!(inspected(&mut server, "alpha")["obligated"], json!(true));
    assert_eq!(
        inspected(&mut server, "gamma_orphan")["obligated"],
        json!(true)
    );
    // A union of a kind no rule obligates: exempt, and why.
    let choice = inspected(&mut server, "Choice");
    assert_eq!(
        (&choice["exempt"], &choice["obligated"]),
        (&json!(true), &json!(false))
    );
    assert_eq!(
        inspected(&mut server, "free_one")["obligated"],
        json!(false)
    );
    assert_eq!(
        inspected(&mut server, "beta_feature")["obligated"],
        json!(false)
    );
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "source_extension names the extension that declares the entity's kind"
)]
fn inspect_names_the_declaring_extension() {
    let mut server = standings_server();
    for id in ["alpha", "beta_feature", "free_one", "Choice"] {
        assert_eq!(
            inspected(&mut server, id)["source_extension"],
            json!("@test/ext"),
            "{id}"
        );
    }
    // A kind no loaded extension declares.
    let thing = inspected(&mut server, "thing");
    assert!(
        !thing.as_object().unwrap().contains_key("source_extension"),
        "absent when no extension declares the kind: {thing}"
    );
}

/// The node ids of a graph-shaped payload, sorted.
fn node_ids(parsed: &Value) -> Vec<String> {
    let mut ids: Vec<String> = parsed["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("no nodes in {parsed}"))
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    ids
}

fn node<'a>(parsed: &'a Value, id: &str) -> &'a Value {
    parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {parsed}"))
}

// --- specforge.query ---

// B:provide_mcp_query_tool — verify unit "returns subgraph for entity"
#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "specforge.query tool returns subgraph for valid entityId"
)]
fn query_returns_subgraph() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    // gamma_orphan is not connected to alpha.
    assert_eq!(node_ids(&parsed), vec!["alpha", "beta_feature"]);
    assert_eq!(
        parsed["edges"],
        json!([{"source": "alpha", "target": "beta_feature", "label": "features"}])
    );
}

// B:provide_mcp_query_tool — verify unit "returns error for unknown entity"
#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "non-existent entityId returns error response"
)]
fn query_error_for_unknown() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "nonexistent"}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "entity_not_found", "{error}");
    assert_eq!(error["entity_id"], "nonexistent", "{error}");
}

// B:provide_mcp_query_tool — verify unit "respects depth parameter"
#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "depth parameter limits traversal depth"
)]
fn query_respects_depth() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha", "depth": 0}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let nodes = parsed["nodes"].as_array().unwrap();
    // Depth 0 = only the root
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["id"], "alpha");
}

// B:provide_mcp_query_tool — verify unit "respects kind filter"
#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "kind filter restricts returned node types"
)]
fn query_respects_kind_filter() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha", "depth": 2, "kinds": ["behavior"]}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let nodes = parsed["nodes"].as_array().unwrap();
    for node in nodes {
        let kind = node["kind"].as_str().unwrap();
        // Root always included, plus kind-filtered nodes
        assert!(kind == "behavior" || node["id"] == "alpha");
    }
}

// B:provide_mcp_query_tool — verify unit "missing entity_id returns error"
#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "a tool that detects invalid arguments returns an isError result, not -32602"
)]
fn query_missing_entity_id() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.query", json!({}));
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], "entity_id", "{error}");
}

// --- specforge.export ---

// B:provide_mcp_export_tool — verify unit "exports graph format"
#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "specforge.export tool returns graph in requested format"
)]
fn export_graph_format() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.export", json!({"format": "graph"}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["nodes"].is_array());
    // Full graph format includes fields
    let alpha = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "alpha")
        .unwrap();
    assert!(alpha["fields"].is_object());
    assert_eq!(parsed["format_version"], "2.0");
    assert!(parsed["schema"].is_object());
    assert_eq!(alpha["file"], "test.spec");
    assert_eq!(alpha["line"], 1);
    assert_eq!(alpha["fields"]["contract"], "The system MUST do alpha");
}

// B:provide_mcp_export_tool — verify unit "exports context format"
#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "all three formats (context, brief, graph) supported"
)]
fn export_context_format() {
    let mut server = project().serve(&[extension().headline("behavior")]);
    let resp = call_tool(
        &mut server,
        "specforge.export",
        json!({"format": "context"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        node_ids(&parsed),
        vec!["alpha", "beta_feature", "gamma_orphan"]
    );
    // The context format lifts the contract to the top level and drops the
    // source location the graph format carries.
    let alpha = node(&parsed, "alpha");
    assert_eq!(alpha["contract"], "The system MUST do alpha");
    assert!(alpha.get("file").is_none(), "{alpha}");
    assert!(alpha.get("line").is_none(), "{alpha}");
    let graph: Value = serde_json::from_str(&tool_text(&call_tool(
        &mut server,
        "specforge.export",
        json!({"format": "graph"}),
    )))
    .unwrap();
    assert_eq!(node(&graph, "alpha")["file"], "test.spec");
    assert!(node(&graph, "alpha").get("contract").is_none());
}

// B:provide_mcp_export_tool — verify unit "exports brief format"
#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "all three formats (context, brief, graph) supported"
)]
fn export_brief_format() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.export", json!({"format": "brief"}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        node_ids(&parsed),
        vec!["alpha", "beta_feature", "gamma_orphan"]
    );
    // Brief nodes carry no fields and no contract.
    for n in parsed["nodes"].as_array().unwrap() {
        assert!(n.get("fields").is_none(), "{n}");
        assert!(n.get("contract").is_none(), "{n}");
        assert!(n["kind"].is_string(), "{n}");
    }
    assert_eq!(node(&parsed, "alpha")["kind"], "behavior");
}

// B:provide_mcp_export_tool — verify unit "exports scoped subgraph"
#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "scope parameter restricts to subgraph"
)]
fn export_scoped() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.export",
        json!({"format": "graph", "scope": "alpha"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    // gamma_orphan lies outside alpha's subgraph.
    assert_eq!(node_ids(&parsed), vec!["alpha", "beta_feature"]);
}

// B:provide_mcp_export_tool — verify unit "unknown format returns error"
#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "unknown format returns error"
)]
fn export_unknown_format() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.export", json!({"format": "yaml"}));
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], "format", "{error}");
}

// --- specforge.trace ---

// B:provide_mcp_trace_tool — verify unit "returns trace chain"
#[specforge_test(
    behavior = "provide_mcp_trace_tool",
    verify = "specforge.trace tool returns traceability chain for valid entityId"
)]
fn trace_returns_chain() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.trace",
        json!({"entity_id": "alpha"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["entity_id"], "alpha");
    // alpha -> beta_feature, so beta_feature is downstream of alpha.
    let downstream = parsed["downstream"].as_array().unwrap();
    assert_eq!(downstream.len(), 1, "{parsed}");
    assert_eq!(downstream[0]["entity_id"], "beta_feature");
    assert_eq!(downstream[0]["edge_label"], "features");
    assert_eq!(parsed["upstream"], json!([]));
}

// B:provide_mcp_trace_tool — verify unit "unknown entity returns error"
#[specforge_test(
    behavior = "provide_mcp_trace_tool",
    verify = "non-existent entityId returns error response"
)]
fn trace_unknown_entity() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.trace",
        json!({"entity_id": "nonexistent"}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "entity_not_found", "{error}");
    assert_eq!(error["entity_id"], "nonexistent", "{error}");
}

// --- specforge.search ---

// B:provide_mcp_search_tool — verify unit "respects kind filter"
#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "kind filter restricts results to matching entity kinds"
)]
fn search_respects_kind_filter() {
    let mut server = test_server();
    let mut hits = |arguments: Value| -> Vec<(String, String)> {
        let resp = call_tool(&mut server, "specforge.search", arguments);
        let parsed: Value =
            serde_json::from_str::<Value>(&tool_text(&resp)).unwrap()["results"].clone();
        parsed
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["entity_id"].as_str().unwrap().to_string(),
                    r["kind"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    // Unfiltered, the query matches all three kinds.
    let kinds: std::collections::BTreeSet<String> = hits(json!({"query": "a"}))
        .into_iter()
        .map(|(_, kind)| kind)
        .collect();
    assert_eq!(
        kinds.iter().map(String::as_str).collect::<Vec<_>>(),
        ["behavior", "feature", "invariant"]
    );
    // The filter keeps what matches ...
    assert_eq!(
        hits(json!({"query": "a", "kinds": ["behavior"]})),
        [("alpha".to_string(), "behavior".to_string())]
    );
    // ... and drops what does not: alpha is a behavior, not a feature.
    assert!(
        hits(json!({"query": "alpha", "kinds": ["feature"]}))
            .iter()
            .all(|(id, _)| id != "alpha")
    );
}

// B:provide_mcp_search_tool — verify unit "respects limit"
#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "limit caps the number of returned results"
)]
fn search_respects_limit() {
    let mut server = test_server();
    let mut ids = |arguments: Value| -> Vec<String> {
        let resp = call_tool(&mut server, "specforge.search", arguments);
        let parsed: Value =
            serde_json::from_str::<Value>(&tool_text(&resp)).unwrap()["results"].clone();
        parsed
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["entity_id"].as_str().unwrap().to_string())
            .collect()
    };
    let unlimited = ids(json!({"query": "a"}));
    assert_eq!(unlimited.len(), 3, "{unlimited:?}");
    assert_eq!(unlimited[0], "alpha", "the prefix match comes first");
    assert_eq!(ids(json!({"query": "a", "limit": 1})), ["alpha"]);
}

// B:provide_mcp_search_tool — verify unit "missing query returns error"
#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "missing query returns error"
)]
fn search_missing_query() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.search", json!({}));
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], "query", "{error}");
}

#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "field without value, or value without field, is an invalid-input error"
)]
fn search_refuses_a_lone_field_or_value() {
    let mut server = test_server();
    for (arguments, missing) in [
        (json!({"query": "alpha", "field": "contract"}), "value"),
        (json!({"query": "alpha", "value": "MUST"}), "field"),
    ] {
        let resp = call_tool(&mut server, "specforge.search", arguments);
        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["code"], "invalid_input", "{error}");
        assert_eq!(error["argument"], missing, "{error}");
    }
    // Together they filter: the contract of alpha contains "must", any case.
    let resp = call_tool(
        &mut server,
        "specforge.search",
        json!({"query": "", "field": "contract", "value": "must"}),
    );
    assert!(
        !tool_json(&resp)["results"]
            .clone()
            .as_array()
            .unwrap()
            .is_empty(),
        "{resp}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "specforge.list reports an unknown kind with I020"
)]
fn list_reports_an_unknown_kind() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.list", json!({"kind": "behaviour"}));
    assert_eq!(tool_json(&resp)["entities"].clone(), json!([]), "{resp}");
    let notices = &resp["result"]["_meta"]["diagnostics"];
    assert_eq!(notices[0]["code"], "I020", "{resp}");
    assert_eq!(
        notices[0]["suggestion"], "did you mean 'behavior'?",
        "{resp}"
    );
    // A known kind reports nothing.
    let resp = call_tool(&mut server, "specforge.list", json!({"kind": "behavior"}));
    assert!(resp["result"]["_meta"].is_null(), "{resp}");
}

#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "the default query is the document specforge://graph/{entityId} serves"
)]
fn query_is_the_graph_resource() {
    let mut server = test_server();
    for id in ["alpha", "gamma_orphan"] {
        let queried = tool_json(&call_tool(
            &mut server,
            "specforge.query",
            json!({"entity_id": id}),
        ));
        let (_, served) = resource(&mut server, &format!("specforge://graph/{id}"));
        assert_eq!(queried, served, "{id}");
        assert_eq!(queried["format_version"], "2.0", "{id}");
    }
}

// --- specforge.schema ---

/// A server that compiled `project_with_errors_and_warnings` (which loads
/// `@specforge/software`), and its `specforge.schema` reply for `args`:
/// serving nothing, the server is served the project `validate` names.
fn compiled_schema(server: &mut McpServer, project: &tempfile::TempDir, args: Value) -> Value {
    call_tool(
        server,
        "specforge.validate",
        json!({"path": project.path().to_str().unwrap()}),
    );
    serde_json::from_str(&tool_text(&call_tool(server, "specforge.schema", args))).unwrap()
}

fn names(list: &Value, key: &str) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|item| item[key].as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "provide_mcp_schema_tool",
    verify = "specforge.schema returns full GraphProtocolSchema"
)]
fn schema_tool_returns_the_graph_protocol_schema() {
    let project = project_with_errors_and_warnings();
    let mut server = serving_nothing();
    let schema = compiled_schema(&mut server, &project, json!({}));

    assert_eq!(
        schema["extensions"],
        json!([{"name": "@specforge/software", "version": "1.0.0"}])
    );
    assert_eq!(
        names(&schema["entity_kinds"], "name"),
        ["behavior", "event", "invariant", "port", "type"]
    );
    // Typed fields, not just names: behavior's contract is a required string.
    let behavior = &schema["entity_kinds"][0];
    let contract = behavior["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "contract")
        .unwrap_or_else(|| panic!("no contract field in {behavior}"));
    assert_eq!(contract["field_type"], "string");
    assert_eq!(contract["required"], true);
    let enforces = schema["edge_types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["label"] == "BehaviorEnforcesInvariant")
        .unwrap_or_else(|| panic!("no BehaviorEnforcesInvariant in {schema}"));
    assert_eq!(enforces["source_kinds"], json!(["behavior"]));
    assert_eq!(enforces["target_kinds"], json!(["invariant"]));

    // The document a full graph export embeds.
    let export: Value = serde_json::from_str(&tool_text(&call_tool(
        &mut server,
        "specforge.export",
        json!({"format": "graph"}),
    )))
    .unwrap();
    assert_eq!(schema, export["schema"]);
}

#[specforge_test(
    behavior = "provide_mcp_schema_tool",
    verify = "kind filter restricts schema to single entity kind"
)]
fn schema_tool_kind_filter() {
    let project = project_with_errors_and_warnings();
    let mut server = serving_nothing();
    let schema = compiled_schema(&mut server, &project, json!({"kind": "invariant"}));
    assert_eq!(names(&schema["entity_kinds"], "name"), ["invariant"]);
    // The edge types that can end at an invariant, and the open ones.
    assert_eq!(
        names(&schema["edge_types"], "label"),
        ["BehaviorEnforcesInvariant", "ExternalRef", "References"]
    );
}

// --- specforge.coverage ---

/// The server with a behavior `two` declaring obligations "a" and "b", and
/// a `specforge-report.json` recording `tests` (verify text, status) for it.
fn server_with_report(tests: &[(&str, &str)]) -> Served {
    let tests: Vec<Value> = tests
        .iter()
        .map(|(verify, status)| json!({"name": verify, "verify": verify, "status": status}))
        .collect();
    project()
        .file(
            "two.spec",
            "behavior two \"Two\" {\n    verify unit \"a\"\n    verify unit \"b\"\n}\n",
        )
        .file(
            "specforge-report.json",
            json!({"results": {"two": {"tests": tests}}}).to_string(),
        )
        .serve(&[extension()])
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "analyze reads the project's specforge-report.json by default"
)]
fn analyze_reads_the_project_report_by_default() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software","@specforge/testing"]}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("app.spec"),
        "behavior two \"Two\" {\n  verify unit \"a\"\n  verify unit \"b\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("specforge-report.json"),
        r#"{"results":{"two":{"tests":[{"name":"t","verify":"a","status":"passed"}]}}}"#,
    )
    .unwrap();

    let mut server = serving_nothing();
    let resp = call_tool(
        &mut server,
        "specforge.analyze",
        json!({"path": root.to_str().unwrap(), "pass": "coverage"}),
    );
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    let coverage = parsed["passes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pass"] == "@specforge/testing:coverage")
        .unwrap_or_else(|| panic!("no coverage pass: {parsed}"))
        .clone();
    assert_eq!(
        coverage["summary"]["test_results"]["obligations_proven"], 1,
        "{coverage}"
    );
    let a015: Vec<&Value> = coverage["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "A015")
        .collect();
    assert_eq!(a015.len(), 1, "{coverage}");
    assert!(a015[0]["message"].as_str().unwrap().contains("\"b\""));
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "stray test records come back as an optional stray_records field"
)]
fn analyze_returns_stray_records_only_when_records_are_stray() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software"]}"#,
    )
    .unwrap();
    std::fs::write(root.join("app.spec"), "behavior widget \"Widget\" {\n}\n").unwrap();
    let report = root.join("specforge-report.json");
    let mut server = serving_nothing();
    let mut run = |strict: bool| -> Value {
        let resp = call_tool(
            &mut server,
            "specforge.analyze",
            json!({"path": root.to_str().unwrap(), "pass": "contracts", "strict": strict}),
        );
        serde_json::from_str(&tool_text(&resp)).unwrap()
    };

    std::fs::write(&report, r#"{"results":{"widget":{"tests":[]}}}"#).unwrap();
    assert!(run(false).get("stray_records").is_none());

    std::fs::write(&report, r#"{"results":{"wodget":{"tests":[]}}}"#).unwrap();
    let expected = json!([{"entity_id": "wodget", "near": "widget"}]);
    let lax = run(false);
    assert_eq!(lax["stray_records"], expected, "{lax}");
    let strict = run(true);
    assert_eq!(strict["stray_records"], expected, "{strict}");
    assert_eq!(strict["ok"], lax["ok"]);
}

/// The `McpError` an `isError` tool result carries.
fn mcp_error(resp: &Value) -> Value {
    assert!(resp["error"].is_null(), "not a JSON-RPC error: {resp}");
    assert_eq!(resp["result"]["isError"], true, "an isError result: {resp}");
    serde_json::from_str(&tool_text(resp)).unwrap_or_else(|e| panic!("McpError JSON ({e}): {resp}"))
}

#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "a malformed specforge-report.json is an error result, not an empty report"
)]
fn coverage_refuses_a_malformed_report() {
    let mut server = test_server();
    server.write(
        "specforge-report.json",
        r#"{"results": {"alpha": {"tests": ["#,
    );

    let error = mcp_error(&call_tool(&mut server, "specforge.coverage", json!({})));
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("specforge-report.json"),
        "names the file: {error}"
    );
    assert_eq!(error["diagnostic"]["code"], "E045", "{error}");

    // Without a report, nothing is recorded: not an error.
    server.remove("specforge-report.json");
    let resp = call_tool(&mut server, "specforge.coverage", json!({}));
    assert_ne!(resp["result"]["isError"], true, "{resp}");
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "a malformed test report is an error result"
)]
fn analyze_refuses_a_malformed_report() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software","@specforge/testing"]}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("app.spec"),
        "behavior two \"Two\" {\n  verify unit \"a\"\n}\n",
    )
    .unwrap();
    std::fs::write(root.join("specforge-report.json"), "{not json").unwrap();

    let mut server = serving_nothing();
    // The project's own report.
    let error = mcp_error(&call_tool(
        &mut server,
        "specforge.analyze",
        json!({"path": root.to_str().unwrap(), "pass": "coverage"}),
    ));
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert_eq!(error["diagnostic"]["code"], "E045", "{error}");

    // A report the caller names.
    let error = mcp_error(&call_tool(
        &mut server,
        "specforge.analyze",
        json!({
            "path": root.to_str().unwrap(),
            "pass": "coverage",
            "test_results": root.join("specforge-report.json").to_str().unwrap(),
        }),
    ));
    assert_eq!(error["code"], "schema_mismatch", "{error}");
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "extension passes receive no proved claims, as specforge analyze without --prove"
)]
fn analyze_hands_extension_passes_no_proved_claims() {
    use crate::fake_extension::{self, EXT, FakeExtension};

    // `@test/cmds` declares one compiler pass and records what it is given.
    let ext = FakeExtension::new()
        .with_passes(&["audit"])
        .with_output("__pass_audit", json!({"diagnostics": []}));
    let (mut server, ext, _project) = fake_extension::initialized(ext);

    let resp = call_tool(&mut server, "specforge.analyze", json!({}));

    assert_ne!(resp["result"]["isError"], true, "{resp}");
    let calls = ext.calls();
    let (_, _, input) = calls
        .iter()
        .find(|(name, export, _)| name == EXT && export == "__pass_audit")
        .unwrap_or_else(|| panic!("the pass never ran: {calls:?}"));
    // Prove did not run: no claims, not an empty set of them.
    assert_eq!(input["proved_claims"], Value::Null, "{input}");
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "an unknown or undeclared pass is an invalid-input error listing the available passes"
)]
fn analyze_refuses_an_undeclared_pass() {
    use crate::fake_extension::{self, FakeExtension};

    let ext = FakeExtension::new()
        .with_passes(&["audit"])
        .with_output("__pass_audit", json!({"diagnostics": []}));
    let (mut server, _ext, _project) = fake_extension::initialized(ext);

    for requested in ["foo:bar", "@test/cmds:typo", "nope"] {
        let error = mcp_error(&call_tool(
            &mut server,
            "specforge.analyze",
            json!({"pass": requested}),
        ));
        assert_eq!(error["code"], "invalid_input", "{error}");
        assert_eq!(error["argument"], "pass", "{error}");
        let message = error["message"].as_str().unwrap();
        for available in ["all", "coverage", "contracts", "@test/cmds:audit"] {
            assert!(message.contains(available), "{available} in {message}");
        }
    }

    // A manifest-declared pass and the `coverage` alias are accepted.
    for requested in ["@test/cmds:audit", "coverage", "contracts", "all"] {
        let resp = call_tool(&mut server, "specforge.analyze", json!({"pass": requested}));
        assert_ne!(resp["result"]["isError"], true, "{requested}: {resp}");
    }
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "strict promotes warnings and clears ok"
)]
fn analyze_strict_promotes_warnings() {
    use crate::fake_extension::{self, FakeExtension};

    let ext = FakeExtension::new().with_passes(&["audit"]).with_output(
        "__pass_audit",
        json!({"diagnostics": [
            {"code": "W900", "severity": "Warning", "message": "careful"}
        ]}),
    );
    let (mut server, _ext, _project) = fake_extension::initialized(ext);
    let run = |server: &mut McpServer, args: Value| -> Value {
        let resp = call_tool(server, "specforge.analyze", args);
        serde_json::from_str(&tool_text(&resp)).unwrap()
    };

    let lenient = run(&mut server, json!({"pass": "@test/cmds:audit"}));
    assert_eq!(lenient["ok"], true, "{lenient}");
    assert_eq!(lenient["passes"][0]["findings"][0]["severity"], "Warning");

    let strict = run(
        &mut server,
        json!({"pass": "@test/cmds:audit", "strict": true}),
    );
    assert_eq!(strict["ok"], false, "{strict}");
    assert_eq!(strict["passes"][0]["findings"][0]["severity"], "Error");
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "analyzing another project leaves the served project untouched"
)]
fn analyze_of_another_project_leaves_the_served_one() {
    use crate::fake_extension::{self, FakeExtension};

    let (mut server, _ext, served) = fake_extension::initialized(FakeExtension::new());
    let served_root = server
        .state()
        .project_root()
        .map(std::path::Path::to_path_buf);
    assert!(served_root.is_some());
    let served_nodes = server.state().graph().node_count();

    let other = within(served.path(), tempfile::tempdir().unwrap());
    std::fs::write(
        other.path().join("specforge.json"),
        r#"{"name":"o","version":"0.1.0","extensions":["@specforge/software"]}"#,
    )
    .unwrap();
    std::fs::write(
        other.path().join("app.spec"),
        "behavior solo \"Solo\" {\n}\n",
    )
    .unwrap();
    let resp = call_tool(
        &mut server,
        "specforge.analyze",
        json!({"path": other.path().to_str().unwrap(), "pass": "contracts"}),
    );
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(parsed["passes"][0]["pass"], "contracts", "{parsed}");

    assert_eq!(
        server
            .state()
            .project_root()
            .map(std::path::Path::to_path_buf),
        served_root
    );
    assert_eq!(server.state().graph().node_count(), served_nodes);
}

// --- specforge.stats ---

// Tool call when not initialized
#[specforge_test(
    behavior = "mcp_initialize",
    verify = "initialization rejects tool calls before completion"
)]
fn tool_call_not_initialized() {
    let mut server = McpServer::new();
    let resp = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha"}),
    );
    assert!(resp["error"].is_object());
}

// Unknown tool name
#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "unknown tool returns error"
)]
fn unknown_tool_returns_error() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.nonexistent", json!({}));
    assert!(resp["error"].is_object());
}

// B:provide_mcp_validate_tool — verify unit "response includes all diagnostics"
#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "specforge.validate tool triggers compilation"
)]
fn validate_returns_all_diagnostics() {
    let project = project_with_errors_and_warnings();
    let mut server = serving_nothing();
    assert_eq!(server.state().project_root(), None);
    assert_eq!(server.state().graph().node_count(), 0);
    assert!(server.state().diagnostics().is_empty());

    // The project the path names, while no project is served: the call
    // compiles and serves it.
    let resp = call_tool(
        &mut server,
        "specforge.validate",
        json!({"path": project.path().to_str().unwrap()}),
    );
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();

    // The project was compiled and is served: its entities are the graph,
    // and its diagnostics are the ones returned.
    assert!(server.state().project_root().is_some());
    let graph = &server.state().graph();
    assert!(graph.node("lonely").is_some());
    assert!(graph.node("act").is_some());
    assert_eq!(graph.node_count(), 2, "only the project's entities");
    let codes: Vec<&str> = parsed
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"E003"), "{codes:?}");
    assert!(codes.contains(&"W003"), "{codes:?}");
    assert_eq!(codes.len(), server.state().diagnostics().len());
}

/// A project whose check yields errors (E003, E006) and warnings (W003, W006).
fn project_with_errors_and_warnings() -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software"]}"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("app.spec"),
        "invariant lonely \"Lonely\" {\n  guarantee \"g\"\n}\n\nbehavior act \"Act\" {\n  invariants [missing]\n}\n",
    )
    .unwrap();
    project
}

/// `specforge.validate`'s diagnostics as (code, severity).
fn validate(args: Value) -> Vec<(String, String)> {
    let mut server = serving_nothing();
    let resp = call_tool(&mut server, "specforge.validate", args);
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    parsed
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["code"].as_str().unwrap().to_string(),
                d["severity"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// The format version header is a compile diagnostic, so the tool shows it
/// with the rest: I007 for an older file, E019 for a newer one.
#[test]
fn validate_reports_the_format_version_headers() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software"]}"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("old.spec"),
        "// specforge-format: 0.9\nbehavior old_one \"Old\" {\n  verify unit \"x\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("new.spec"),
        "// specforge-format: 9.0\nbehavior new_one \"New\" {\n  verify unit \"x\"\n}\n",
    )
    .unwrap();

    let found = validate(json!({"path": project.path().to_str().unwrap()}));

    assert!(
        found.contains(&("I007".to_string(), "Info".to_string())),
        "{found:?}"
    );
    assert!(
        found.contains(&("E019".to_string(), "Error".to_string())),
        "{found:?}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "severity_filter restricts returned diagnostics"
)]
fn validate_severity_filter_restricts_diagnostics() {
    let project = project_with_errors_and_warnings();
    let path = project.path().to_str().unwrap();
    let all = validate(json!({"path": path}));
    assert!(all.iter().any(|(_, s)| s == "Error"), "{all:?}");
    assert!(all.iter().any(|(_, s)| s == "Warning"), "{all:?}");
    for severity in ["Error", "Warning"] {
        let filtered = validate(json!({"path": path, "severity_filter": severity.to_lowercase()}));
        let expected: Vec<_> = all.iter().filter(|(_, s)| s == severity).cloned().collect();
        assert_eq!(filtered, expected);
    }
}

#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "strict mode promotes warnings to errors"
)]
fn validate_strict_promotes_warnings_to_errors() {
    let project = project_with_errors_and_warnings();
    let path = project.path().to_str().unwrap();
    let all = validate(json!({"path": path}));
    let strict = validate(json!({"path": path, "strict": true}));
    let codes = |d: &[(String, String)]| d.iter().map(|(c, _)| c.clone()).collect::<Vec<_>>();
    assert_eq!(codes(&strict), codes(&all), "the same diagnostics");
    assert!(strict.iter().all(|(_, s)| s == "Error"), "{strict:?}");
    assert!(strict.iter().any(|(c, _)| c == "W003"));
}

/// `specforge.validate` on the project at `path` with `args` added: the
/// whole response.
fn validate_response(path: &str, mut args: Value) -> Value {
    args["path"] = json!(path);
    let mut server = serving_nothing();
    call_tool(&mut server, "specforge.validate", args)
}

#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "an unknown severity_filter or lint profile is invalid input"
)]
fn validate_refuses_an_unknown_severity_filter_or_lint_profile() {
    let project = project_with_errors_and_warnings();
    let path = project.path().to_str().unwrap();
    for (args, argument, suggestion) in [
        (
            json!({"severity_filter": "errors"}),
            "severity_filter",
            Some("did you mean 'error'?"),
        ),
        (json!({"severity_filter": "fatal"}), "severity_filter", None),
        (json!({"lint": ["nonsense"]}), "lint", None),
        (
            json!({"lint": ["inferred", "pedantik"]}),
            "lint",
            Some("did you mean 'pedantic'?"),
        ),
    ] {
        let resp = validate_response(path, args.clone());
        assert_eq!(resp["result"]["isError"], true, "{args}: {resp}");
        let error: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
        assert_eq!(error["code"], "invalid_input", "{args}: {error}");
        assert_eq!(error["argument"], argument, "{args}: {error}");
        assert_eq!(
            error["data"]["suggestion"].as_str(),
            suggestion,
            "{args}: {error}"
        );
        let message = error["message"].as_str().unwrap();
        assert!(message.starts_with("Unknown "), "{message}");
    }

    // The payload's own spelling of a severity filters like the lowercase
    // name, and the documented profiles are accepted.
    let path = project.path().to_str().unwrap();
    for spelling in ["Error", "ERROR"] {
        assert_eq!(
            validate(json!({"path": path, "severity_filter": spelling})),
            validate(json!({"path": path, "severity_filter": "error"})),
            "{spelling}"
        );
    }
    assert_eq!(
        validate(json!({"path": path, "lint": ["pedantic"]})),
        validate(json!({"path": path}))
    );
}

#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "the verdict on every reported diagnostic rides in _meta, whatever severity_filter shows"
)]
fn validate_verdict_in_meta_counts_everything_reported() {
    let project = project_with_errors_and_warnings();
    let path = project.path().to_str().unwrap();
    let all = validate(json!({"path": path}));
    let count = |severity: &str| all.iter().filter(|(_, s)| s == severity).count();
    let (errors, warnings, infos) = (count("Error"), count("Warning"), count("Info"));
    assert!(errors > 0 && warnings > 0, "{all:?}");

    let resp = validate_response(path, json!({"severity_filter": "warning"}));
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    let shown: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(shown.as_array().unwrap().len(), warnings);
    assert_eq!(
        resp["result"]["_meta"]["specforge/check"],
        json!({
            "ok": false,
            "errors": errors,
            "warnings": warnings,
            "infos": infos,
            "shown": warnings,
        }),
        "{resp}"
    );

    // Strict promotes first: the verdict counts the promoted warnings as
    // errors, and a warning filter then shows nothing.
    let strict = validate_response(path, json!({"strict": true, "severity_filter": "warning"}));
    assert_eq!(
        strict["result"]["_meta"]["specforge/check"],
        json!({
            "ok": false,
            "errors": errors + warnings,
            "warnings": 0,
            "infos": infos,
            "shown": 0,
        }),
        "{strict}"
    );

    // A clean project passes.
    let clean = tempfile::tempdir().unwrap();
    std::fs::write(
        clean.path().join("specforge.json"),
        r#"{"name":"c","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    let resp = validate_response(clean.path().to_str().unwrap(), json!({}));
    let verdict = &resp["result"]["_meta"]["specforge/check"];
    assert_eq!(verdict["ok"], true, "{resp}");
    assert_eq!(verdict["errors"], 0, "{resp}");
}

// B:provide_mcp_validate_tool — verify unit "validate reports files changed on disk since the last call"
#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "validate reports files changed on disk since the last call"
)]
fn validate_reports_files_changed_since_the_last_call() {
    let project = project_with_errors_and_warnings();
    let mut server = serving_nothing();
    // The first call serves the project its path names.
    let first = codes_of(&call_tool(
        &mut server,
        "specforge.validate",
        json!({"path": project.path().to_str().unwrap()}),
    ));
    assert!(first.contains(&"E003".to_string()), "{first:?}");

    // Fix the unresolved reference on disk.
    fix_unresolved_reference(project.path());
    let second = codes_of(&call_tool(&mut server, "specforge.validate", json!({})));
    assert!(
        !second.contains(&"E003".to_string()),
        "validate must report the files on disk: {second:?}"
    );
    assert!(server.state().graph().node("fixed").is_some());
}

/// The diagnostic codes of a `specforge.validate` response.
fn codes_of(resp: &Value) -> Vec<String> {
    let parsed: Value = serde_json::from_str(&tool_text(resp)).unwrap();
    parsed
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap().to_string())
        .collect()
}

/// Rewrites `project_with_errors_and_warnings`' spec so `act` no longer
/// names the missing invariant (no E003) and a `fixed` behavior appears.
fn fix_unresolved_reference(root: &std::path::Path) {
    std::fs::write(
        root.join("app.spec"),
        "invariant lonely \"Lonely\" {\n  guarantee \"g\"\n}\n\nbehavior act \"Act\" {\n  invariants [lonely]\n}\n\nbehavior fixed \"Fixed\" {\n}\n",
    )
    .unwrap();
}

/// A project of two testable behaviors and an invariant they lean on, served
/// after one `specforge.validate`: the plan tests' graph.
fn plan_project() -> (tempfile::TempDir, McpServer) {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software","@specforge/testing"]}"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("app.spec"),
        "invariant inv_a \"A\" {\n  guarantee \"g\"\n  verify unit \"z\"\n}\n\n\
         behavior act_one \"One\" {\n  invariants [inv_a]\n  verify unit \"x\"\n}\n\n\
         behavior act_two \"Two\" {\n  verify unit \"y\"\n}\n",
    )
    .unwrap();
    let mut server = serving_nothing();
    call_tool(
        &mut server,
        "specforge.validate",
        json!({"path": project.path().to_str().unwrap()}),
    );
    (project, server)
}

#[specforge_test(
    behavior = "provide_mcp_trace_tool",
    verify = "plan parameter triggers gap analysis"
)]
fn trace_plan_gap_analysis() {
    let (_project, mut server) = plan_project();

    // act_one depends on inv_a but comes first; ghost doesn't exist;
    // act_two is testable and missing.
    let plan = agent_plan(&["act_one", "inv_a", "ghost"]);
    let resp = call_tool(&mut server, "specforge.trace", json!({"plan": plan}));
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(parsed["resolved_entries"], json!(["act_one", "inv_a"]));
    let gaps: Vec<(&str, &str, &str)> = parsed["gaps"]
        .as_array()
        .unwrap_or_else(|| panic!("no gaps in {parsed}"))
        .iter()
        .map(|g| {
            (
                g["source_entity"].as_str().unwrap(),
                g["target_entity"].as_str().unwrap(),
                g["missing_link_type"].as_str().unwrap(),
            )
        })
        .collect();
    assert!(
        gaps.contains(&("plan", "ghost", "unresolved_entity")),
        "{gaps:?}"
    );
    assert!(
        gaps.contains(&("plan", "act_two", "missing_plan_entry")),
        "{gaps:?}"
    );
    assert!(gaps.contains(&("act_one", "inv_a", "ordering")), "{gaps:?}");
    assert_eq!(gaps.len(), 3, "{gaps:?}");
    assert_eq!(parsed["ok"], false, "{parsed}");
}

#[specforge_test(behavior = "validate_agent_plan", verify = "output is structured JSON")]
fn trace_plan_reports_its_gaps_as_structured_json() {
    let (_project, mut server) = plan_project();

    let plan = agent_plan(&["act_one", "ghost"]);
    let resp = call_tool(&mut server, "specforge.trace", json!({"plan": plan}));
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();

    // One object: the entries that name an entity, and the gaps.
    let mut keys: Vec<&str> = parsed
        .as_object()
        .unwrap_or_else(|| panic!("not an object: {parsed}"))
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(keys, ["gaps", "ok", "resolved_entries"], "{parsed}");
    assert_eq!(parsed["ok"], false, "{parsed}");
    assert_eq!(parsed["resolved_entries"], json!(["act_one"]));
    for gap in parsed["gaps"].as_array().unwrap() {
        let mut keys: Vec<&str> = gap
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "gap_context",
                "missing_link_type",
                "source_entity",
                "target_entity"
            ],
            "{gap}"
        );
        assert!(
            gap.as_object().unwrap().values().all(Value::is_string),
            "{gap}"
        );
    }
    let ghost = parsed["gaps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|gap| gap["target_entity"] == "ghost")
        .unwrap_or_else(|| panic!("no gap for ghost in {parsed}"));
    assert!(
        ghost["gap_context"].as_str().unwrap().starts_with("E003"),
        "{ghost}"
    );
}

#[specforge_test(
    behavior = "validate_agent_plan",
    verify = "a plan that is not an AgentPlan is refused, naming every place it departs from the type"
)]
fn trace_refuses_a_plan_that_is_not_an_agent_plan() {
    let (_project, mut server) = plan_project();
    let mut refused = |plan: Value| {
        let error = mcp_error(&call_tool(
            &mut server,
            "specforge.trace",
            json!({"plan": plan}),
        ));
        assert_eq!(error["code"], "invalid_input", "{error}");
        assert_eq!(error["argument"], "plan", "{error}");
        error["message"].as_str().unwrap().to_string()
    };
    assert_eq!(
        refused(json!({"entries": [{"entity_id": "act_one"}]})),
        "plan must be an AgentPlan: plan: missing required plan_id; plan.entries[0]: missing required action"
    );
    assert_eq!(
        refused(json!([1, 2])),
        "plan must be an AgentPlan: plan: expected object, got array"
    );
    assert_eq!(
        refused(
            json!({"plan_id": "p", "entries": [{"entity_id": "Task", "action": "implement", "dependencies": ["a"]}], "bogus": 1})
        ),
        "plan must be an AgentPlan: plan.entries[0].dependencies: undeclared key; plan.bogus: undeclared key"
    );
    assert_eq!(
        refused(json!({"entries": [{"entity_id": "act_one", "action": 7, "weird": true}]})),
        "plan must be an AgentPlan: plan: missing required plan_id; plan.entries[0].action: expected string, got integer; plan.entries[0].weird: undeclared key"
    );
    assert_eq!(
        refused(json!({"plan_id": "p", "entries": [["act_one", "implement"]]})),
        "plan must be an AgentPlan: plan.entries[0]: expected object, got array"
    );
    // The JSON text of a plan is read as the plan.
    let resp = call_tool(
        &mut server,
        "specforge.trace",
        json!({"plan": "{\"plan_id\":\"p\",\"entries\":[]}"}),
    );
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(parsed["resolved_entries"], json!([]), "{parsed}");
}

#[specforge_test(
    behavior = "provide_mcp_trace_tool",
    verify = "trace without entity_id or plan returns error"
)]
fn trace_without_entity_or_plan_errors() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.trace", json!({}));
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("entity_id or plan"), "{message}");
}

#[specforge_test(
    behavior = "provide_mcp_schema_tool",
    verify = "include_edges false omits edge type definitions"
)]
fn schema_include_edges_false_omits_edges() {
    let project = project_with_errors_and_warnings();
    let mut server = serving_nothing();
    let full = compiled_schema(&mut server, &project, json!({}));
    assert_eq!(
        full["edge_types"].as_array().unwrap().len(),
        15,
        "edges by default"
    );
    let without = compiled_schema(&mut server, &project, json!({"include_edges": false}));
    assert!(without.get("edge_types").is_none(), "{without}");
    assert_eq!(without["entity_kinds"], full["entity_kinds"]);
}

// B:provide_mcp_schema_tool — verify unit "include_validation_rules true includes rules"
#[specforge_test(
    behavior = "provide_mcp_schema_tool",
    verify = "include_validation_rules true includes validation rules"
)]
fn schema_include_validation_rules_lists_extension_rules() {
    let project = project_with_errors_and_warnings();
    let mut server = serving_nothing();
    // Compiling the project loads @specforge/software's manifest.
    call_tool(
        &mut server,
        "specforge.validate",
        json!({"path": project.path().to_str().unwrap()}),
    );
    let schema = |server: &mut McpServer, args: Value| -> Value {
        serde_json::from_str(&tool_text(&call_tool(server, "specforge.schema", args))).unwrap()
    };
    assert!(
        schema(&mut server, json!({}))
            .get("validation_rules")
            .is_none()
    );
    let with = schema(&mut server, json!({"include_validation_rules": true}));
    let rules = with["validation_rules"].as_array().unwrap();
    let w003 = rules
        .iter()
        .find(|r| r["code"] == "W003")
        .unwrap_or_else(|| panic!("no W003 in {with}"));
    assert_eq!(w003["extension"], "@specforge/software");
    assert_eq!(w003["severity"], "warning");
}

// B:provide_mcp_coverage_tool — verify unit "kind filter restricts to matching entity kinds"
#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "kind filter restricts to matching entity kinds"
)]
fn coverage_kind_filter() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.coverage",
        json!({"kind": "behavior"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str::<Value>(&text).unwrap()["entities"].clone();
    let results = parsed.as_array().unwrap();
    for r in results {
        assert_eq!(r["kind"], "behavior");
    }
}

// B:provide_mcp_coverage_tool — verify unit "status_filter restricts to matching status"
#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "status_filter restricts to matching coverage status"
)]
fn coverage_status_filter_restricts_status() {
    // `two` has one of its two obligations proven; alpha has none.
    let mut server = server_with_report(&[("a", "passed")]);
    let ids = |server: &mut McpServer, status: &str| {
        let resp = call_tool(
            server,
            "specforge.coverage",
            json!({"status_filter": status}),
        );
        let parsed: Value =
            serde_json::from_str::<Value>(&tool_text(&resp)).unwrap()["entities"].clone();
        parsed
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["entity_id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&mut server, "partial"), vec!["two"]);
    assert!(ids(&mut server, "uncovered").contains(&"alpha".to_string()));
    assert!(!ids(&mut server, "uncovered").contains(&"two".to_string()));
    assert!(ids(&mut server, "covered").is_empty());
}

#[specforge_test(
    behavior = "provide_mcp_schema_tool",
    verify = "an unknown kind is an invalid-input error naming the closest kind"
)]
fn schema_unknown_kind_is_invalid_input() {
    let mut server = test_server();
    for (kind, suggestion) in [
        ("behaviour", json!("did you mean 'behavior'?")),
        ("nosuch", Value::Null),
    ] {
        let resp = call_tool(&mut server, "specforge.schema", json!({"kind": kind}));
        assert_eq!(resp["result"]["isError"], true, "{resp}");
        let error: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
        assert_eq!(error["code"], "invalid_input", "{error}");
        assert_eq!(error["argument"], "kind", "{error}");
        assert_eq!(
            error["message"],
            format!("unknown entity kind '{kind}'"),
            "{error}"
        );
        assert_eq!(error["data"]["suggestion"], suggestion, "{error}");
    }
    // A known kind still answers its part of the schema.
    let resp = call_tool(&mut server, "specforge.schema", json!({"kind": "behavior"}));
    let schema: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(schema["entity_kinds"][0]["name"], "behavior", "{schema}");
}

#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "an unknown status_filter is an invalid-input error naming the closest status"
)]
fn coverage_refuses_an_unknown_status_filter() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.coverage",
        json!({"status_filter": "coverd"}),
    );
    assert_eq!(resp["result"]["isError"], true, "{resp}");
    let error: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], "status_filter", "{error}");
    assert_eq!(
        error["data"]["suggestion"], "did you mean 'covered'?",
        "{error}"
    );
}

// B:provide_mcp_query_tool — verify unit "format parameter selects emitter format"
#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "format parameter changes output serialization"
)]
fn query_format_parameter() {
    let mut server = project().serve(&[extension().headline("behavior")]);
    // context format
    let resp = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha", "format": "context"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        node(&parsed, "alpha")["contract"],
        "The system MUST do alpha"
    );

    // brief format
    let resp2 = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha", "format": "brief"}),
    );
    let text2 = tool_text(&resp2);
    let parsed2: Value = serde_json::from_str(&text2).unwrap();
    let brief_alpha = node(&parsed2, "alpha");
    assert!(brief_alpha.get("contract").is_none(), "{brief_alpha}");
    assert!(brief_alpha.get("fields").is_none(), "{brief_alpha}");
    assert_ne!(text, text2, "the two formats serialize differently");
    // Both formats return the same subgraph.
    assert_eq!(node_ids(&parsed), node_ids(&parsed2));
}

// B:provide_mcp_query_tool — verify unit "include_coverage annotates nodes with coverage status"
#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "include_coverage parameter includes coverage status in response"
)]
fn query_include_coverage() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha", "include_coverage": true}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let nodes = parsed["nodes"].as_array().unwrap();
    let alpha = nodes.iter().find(|n| n["id"] == "alpha").unwrap();
    // Declared obligations with no recorded test: as specforge.coverage says.
    assert_eq!(alpha["coverage_status"], "uncovered");

    let mut server = server_with_report(&[("a", "passed"), ("b", "passed")]);
    let resp = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "two", "include_coverage": true}),
    );
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    let two = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "two")
        .unwrap()
        .clone();
    assert_eq!(two["coverage_status"], "covered");
}

// B:provide_mcp_search_tool — verify unit "field and value filter matches entity fields"
#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "field and value filter matches entity fields"
)]
fn search_field_value_filter() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.search",
        json!({"query": "alpha", "field": "contract", "value": "MUST"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str::<Value>(&text).unwrap()["results"].clone();
    let results = parsed.as_array().unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0]["entity_id"], "alpha");

    // The value is matched against the field's text, not its Rust
    // representation (`String("…")`).
    let resp = call_tool(
        &mut server,
        "specforge.search",
        json!({"query": "", "field": "contract", "value": "string"}),
    );
    let parsed: Value =
        serde_json::from_str::<Value>(&tool_text(&resp)).unwrap()["results"].clone();
    assert_eq!(parsed, json!([]));
}

// B:provide_mcp_search_tool — verify unit "references filter returns entities referencing target"
#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "references filter returns entities referencing target"
)]
fn search_references_filter() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.search",
        json!({"query": "", "references": "beta_feature"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str::<Value>(&text).unwrap()["results"].clone();
    let results = parsed.as_array().unwrap();
    // alpha has an edge to beta_feature
    assert!(results.iter().any(|r| r["entity_id"] == "alpha"));
    assert!(
        results.iter().all(|r| r["entity_id"] != "beta_feature"),
        "{parsed}"
    );
}

// B:provide_mcp_validate_tool — verify unit "validate recompiles and updates graph"
#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "response includes all diagnostics as Graph Protocol diagnostics"
)]
fn validate_updates_graph() {
    let project = project_with_errors_and_warnings();
    let mut server = serving_nothing();
    let resp = call_tool(
        &mut server,
        "specforge.validate",
        json!({"path": project.path().to_str().unwrap()}),
    );
    // A validation run that finds errors is a successful call (ADR 0004
    // D4-a): the findings are its result.
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    let diagnostics = parsed.as_array().unwrap();
    // All of them: as many as the compile produced.
    assert_eq!(diagnostics.len(), server.state().diagnostics().len());
    for d in diagnostics {
        for key in ["code", "severity", "message"] {
            assert!(d[key].is_string(), "{key} missing in {d}");
        }
    }
    for (code, severity) in [("E003", "Error"), ("W003", "Warning")] {
        let d = diagnostics
            .iter()
            .find(|d| d["code"] == code)
            .unwrap_or_else(|| panic!("no {code} in {parsed}"));
        assert_eq!(d["severity"], severity);
        assert!(d["file"].as_str().unwrap().ends_with("app.spec"), "{d}");
        assert!(d["line"].as_u64().unwrap() >= 1, "{d}");
    }
}

// --- C9-02/C9-03: token budget enforcement ---

// B:provide_mcp_export_tool — verify unit "max_tokens truncates output within budget with metadata"
#[test]
fn export_with_max_tokens_truncates_and_reports_budget() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.export",
        json!({"format": "graph", "max_tokens": 120}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();

    // metadata: the budget result rides in the payload
    assert!(
        parsed["token_budget"].is_object(),
        "budget metadata must be present: {}",
        text.len()
    );
    assert_eq!(parsed["token_budget"]["budget_tokens"], 120);

    // the estimate must honor the budget
    let estimated = parsed["token_budget"]["estimated_tokens"].as_u64().unwrap();
    assert!(
        estimated <= 120,
        "estimated tokens {estimated} exceed the 120-token budget"
    );

    // and truncation happened (a full-graph export is much larger)
    assert!(
        !parsed["token_budget"]["truncated_entities"]
            .as_array()
            .unwrap()
            .is_empty(),
        "entities must have been dropped to fit the budget"
    );
}

// B:mcp_resource_reads — verify unit "context resource honors ?max_tokens budget"
#[test]
fn context_resource_honors_max_tokens_budget() {
    let mut server = test_server();
    let req = json!({
        "jsonrpc": "2.0", "id": 7,
        "method": "resources/read",
        "params": { "uri": "specforge://context?max_tokens=150" }
    });
    let resp: Value =
        serde_json::from_str(&server.handle_message(&req.to_string()).unwrap()).unwrap();
    let text = resp["result"]["contents"][0]["text"].as_str().unwrap_or("");
    assert!(!text.is_empty(), "context payload must be present");

    // rough token estimate (words + structural chars) must fit the budget
    let words = text.split_whitespace().count();
    assert!(
        words <= 150,
        "budgeted context must be trimmed to ~150 tokens, got ~{words} words"
    );
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "a diagnostic code behind a failed tool call is in its McpError diagnostic"
)]
fn query_and_trace_report_e003_in_the_diagnostic_not_the_message() {
    let mut server = test_server();
    for tool in ["specforge.query", "specforge.trace"] {
        let resp = call_tool(&mut server, tool, json!({"entity_id": "nonexistent"}));
        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["code"], "entity_not_found", "{tool}: {error}");
        assert_eq!(error["diagnostic"]["code"], "E003", "{tool}: {error}");
        let message = error["message"].as_str().unwrap();
        assert!(!message.starts_with("E003"), "{tool}: {message}");
        assert!(message.contains("nonexistent"), "{tool}: {message}");
    }
}

// --- specforge.explain ---

#[specforge_test(
    behavior = "provide_mcp_explain_tool",
    verify = "specforge.explain returns the catalogued title, owner, level, explanation and docs link"
)]
fn explain_returns_the_catalogued_entry() {
    let mut server = test_server();
    let explain = |server: &mut McpServer, code: &str| -> Value {
        serde_json::from_str(&tool_text(&call_tool(
            server,
            "specforge.explain",
            json!({"code": code}),
        )))
        .unwrap()
    };
    // Any case: the same entry `specforge explain W018` prints.
    let entry = explain(&mut server, "w018");
    assert_eq!(entry["code"], "W018");
    assert_eq!(entry["title"], "Duplicate edge type");
    assert_eq!(entry["owner"], "core");
    assert_eq!(entry["level"], "warning");
    assert_eq!(entry["retired"], false);
    assert!(
        entry["explanation"]
            .as_str()
            .unwrap()
            .starts_with("Two extensions register an edge type with the same label"),
        "{entry}"
    );
    assert_eq!(
        entry["docs"],
        "https://github.com/leaderiop/SpecForge/blob/main/docs/diagnostics.md#w018"
    );
    // An extension's code names that extension.
    let formal = explain(&mut server, "E031");
    assert_eq!(formal["owner"], "@specforge/formal", "{formal}");
}

#[specforge_test(
    behavior = "provide_mcp_explain_tool",
    verify = "a retired code names the code that replaced it"
)]
fn explain_a_retired_code_names_its_replacement() {
    let mut server = test_server();
    let retired: Value = serde_json::from_str(&tool_text(&call_tool(
        &mut server,
        "specforge.explain",
        json!({"code": "E047"}),
    )))
    .unwrap();
    assert_eq!(retired["code"], "E047");
    assert_eq!(retired["retired"], true);
    assert_eq!(retired["replaced_by"]["code"], "W139", "{retired}");
    assert_eq!(retired["replaced_by"]["level"], "warning");

    // Retired with no successor.
    let gone: Value = serde_json::from_str(&tool_text(&call_tool(
        &mut server,
        "specforge.explain",
        json!({"code": "W024"}),
    )))
    .unwrap();
    assert_eq!(gone, json!({"code": "W024", "retired": true}));
}

#[specforge_test(
    behavior = "provide_mcp_explain_tool",
    verify = "an uncatalogued code is an invalid_input error"
)]
fn explain_an_uncatalogued_code_is_invalid_input() {
    let mut server = test_server();
    // E901 belongs to a third-party extension; Z123 is no code at all.
    for code in ["E901", "Z123"] {
        let resp = call_tool(&mut server, "specforge.explain", json!({"code": code}));
        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["code"], "invalid_input", "{code}: {error}");
        assert_eq!(error["argument"], "code", "{code}: {error}");
        assert_eq!(
            error["message"],
            format!("unknown diagnostic code: {code}"),
            "{error}"
        );
        assert!(
            error["data"]["suggestion"]
                .as_str()
                .unwrap()
                .starts_with("codes are E###"),
            "{error}"
        );
    }
}

#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "each catalogued diagnostic carries its title"
)]
fn validate_gives_each_catalogued_code_its_title() {
    let project = project_with_errors_and_warnings();
    let mut server = serving_nothing();
    let resp = call_tool(
        &mut server,
        "specforge.validate",
        json!({"path": project.path().to_str().unwrap()}),
    );
    let found: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    let title = |code: &str| -> Value {
        found
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["code"] == code)
            .unwrap_or_else(|| panic!("no {code} in {found}"))["title"]
            .clone()
    };
    assert_eq!(title("E003"), "Unresolved reference");
    assert_eq!(title("E006"), "Missing required field");
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "specforge.analyze takes min and returns the run verdict and where the gate landed"
)]
fn analyze_gates_on_min() {
    // Software and testing, a widget with an obligation, and a recorded
    // report that proves nothing: 0 of 1 testable entities proven.
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("spec")).unwrap();
    std::fs::write(
        root.path().join("specforge.json"),
        r#"{"name":"cov","spec_root":"spec","extensions":["@specforge/software","@specforge/testing"]}"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("spec/a.spec"),
        "type widget \"Widget\" {\n  id string @unique\n  verify unit \"widget valid\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("specforge-report.json"),
        r#"{"runner":"r","results":{}}"#,
    )
    .unwrap();
    let path = root.path().to_str().unwrap();
    let mut server = serving_nothing();

    // A number, or a string holding one, is read the same way; the gate is
    // below its minimum, which is a verdict, not a refusal.
    for min in [json!(50), json!("50")] {
        let resp = call_tool(
            &mut server,
            "specforge.analyze",
            json!({"pass": "coverage", "min": min, "path": path}),
        );
        assert_eq!(resp["result"]["isError"], false, "{resp}");
        let doc: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
        assert_eq!(doc["ok"], false, "{doc}");
        assert_eq!(doc["gate"]["status"], "below", "{doc}");
        assert_eq!(doc["gate"]["min"], 50.0, "{doc}");
    }

    let resp = call_tool(
        &mut server,
        "specforge.analyze",
        json!({"pass": "coverage", "min": true, "path": path}),
    );
    let error = mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(
        error["message"], "min must be a number, got true",
        "{error}"
    );

    let resp = call_tool(
        &mut server,
        "specforge.analyze",
        json!({"pass": "coverage", "min": 150, "path": path}),
    );
    let error = mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(
        error["message"], "min must be a percentage between 0 and 100, got 150",
        "{error}"
    );

    // A minimum the coverage pass will not answer is refused as E068.
    let resp = call_tool(
        &mut server,
        "specforge.analyze",
        json!({"pass": "contracts", "min": 50, "path": path}),
    );
    let error = mcp_error(&resp);
    assert_eq!(error["diagnostic"]["code"], "E068", "{error}");
}

// --- specforge.validate never writes the build cache ---

/// Product declares `status` the lifecycle field of feature and milestone; software's behavior has a `status` too,
/// but no lifecycle field.
fn build_cache_project() -> TestProject {
    TestProject::new()
        .enabling(&["@specforge/software", "@specforge/product"])
        .files(&[
            (
                "b.spec",
                "feature zeta \"Z\" {\n  problem \"p\"\n  status in_progress\n}\n\nbehavior beta \"B\" {\n  contract \"a status, but no lifecycle\"\n  status draft\n}\n",
            ),
            (
                "a.spec",
                "feature alpha \"A\" {\n  problem \"p\"\n  status done\n}\n\nmilestone mid \"M\" {\n  status \"planned\"\n}\n",
            ),
        ])
}

#[specforge_test(
    behavior = "write_build_cache",
    verify = "MCP validate never writes the build cache"
)]
fn mcp_validate_never_writes_the_build_cache() {
    const CACHE: &str = "specforge-cache.json";
    let mut served = Served::rooted(build_cache_project());
    // Without strict the project passes the check, which is when `check
    // --cache` writes; under strict its warnings fail it. validate takes no
    // cache argument, and an unknown one is refused (ADR 0033).
    let runs = [json!({}), json!({"strict": true})];
    let validate = |served: &mut Served, arguments: &Value| {
        let resp = served.call_tool("specforge.validate", arguments.clone());
        assert_eq!(resp["result"]["isError"], false, "{resp}");
        let passes = arguments["strict"] != true;
        assert_eq!(
            resp["result"]["_meta"]["specforge/check"]["ok"], passes,
            "{arguments}: {resp}"
        );
    };
    for arguments in &runs {
        validate(&mut served, arguments);
        assert!(!served.root().join(CACHE).exists(), "{arguments}");
    }
    let refused = served.call_tool("specforge.validate", json!({"cache": true}));
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    assert_eq!(refused["result"]["structuredContent"]["argument"], "cache");
    assert!(!served.root().join(CACHE).exists());

    // An existing cache is read, never rewritten.
    let previous = "{\"format\": 1, \"statuses\": {}}\n";
    served.write(CACHE, previous);
    for arguments in &runs {
        validate(&mut served, arguments);
        assert_eq!(
            std::fs::read_to_string(served.root().join(CACHE)).unwrap(),
            previous,
            "{arguments}"
        );
    }
}
