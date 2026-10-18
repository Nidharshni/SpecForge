use crate::support::*;
use serde_json::{Value, json};
use specforge_extension_sdk::prelude::{CheckKind, PassDiagnostic, PassSpan, ValidationSeverity};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;
use specforge_wasm::WasmRuntime as _;
use std::path::{Path, PathBuf};

/// test.spec: behavior `alpha` on lines 1–5, contract "MUST work", verify
/// unit "works".
const ALPHA_SPEC: &str = concat!(
    "behavior alpha \"Alpha\" {\n",
    "    contract \"MUST work\"\n",
    "    verify unit \"works\"\n",
    "    // the one behavior\n",
    "}\n",
);

/// feat.spec: feature `beta` on lines 1–3, its `behaviors [alpha]` on line
/// 2 (`alpha` at columns 16–21).
const BETA_SPEC: &str = "feature beta \"Beta\" {\n    behaviors [alpha]\n}\n";

/// feat.spec: `beta` with `alpha` and the invariant `gamma`.
const BETA_WITH_GAMMA: &str =
    "feature beta \"Beta\" {\n    behaviors [alpha]\n    invariants [gamma]\n}\n";

/// The two-entity project the contracts read: behavior `alpha` (test.spec)
/// and feature `beta` (feat.spec) with a `behaviors` edge beta -> alpha.
fn contracts_project() -> TestProject {
    TestProject::new()
        .file("test.spec", ALPHA_SPEC)
        .file("feat.spec", BETA_SPEC)
}

/// `@test/ext` with the software kinds: behaviors are testable, features
/// are not; `behavior`'s contract is a headline field.
fn extension() -> TestExtension {
    TestExtension::software().headline("behavior")
}

/// An initialized server over [`contracts_project`].
fn test_server() -> Served {
    contracts_project().serve(&[extension()])
}

fn assert_resource_read(server: &impl Logs, uri: &str) {
    let reads = events(server, "mcp_resource_read");
    assert!(
        reads
            .iter()
            .any(|p| p["resourceUri"] == uri && p["format"] == "application/json"),
        "no mcp_resource_read for {uri}: {reads:?}"
    );
}

/// The `id` of each node in a graph payload, sorted.
fn node_ids(payload: &Value) -> Vec<String> {
    let mut ids: Vec<String> = payload["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("no nodes in {payload}"))
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    ids
}

fn canonical_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn find<'a>(items: &'a Value, key: &str, value: &str) -> &'a Value {
    items
        .as_array()
        .unwrap_or_else(|| panic!("not an array: {items}"))
        .iter()
        .find(|item| item[key] == value)
        .unwrap_or_else(|| panic!("no item with {key} = {value} in {items}"))
}

/// The vendored product extension blob, copied into the project at `root`.
fn product_wasm(root: &Path) -> PathBuf {
    blob_in(root, specforge_component::committed::GREET.path())
}

/// A project on disk with `config` as its specforge.json and one spec file.
fn project_dir(config: Value, spec: &str) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    std::fs::write(dir.path().join("main.spec"), spec).unwrap();
    dir
}

#[specforge_test(
    behavior = "mcp_initialize",
    verify = "MCP Initialize: MCP initialization holds — compiler_api_available, wasm_runtime_available, capabilities_returned, surface_contributions_merged, mcp_initialized_emitted"
)]
fn contract_initialize() {
    use crate::fake_extension::{self, EXT, FakeExtension};
    // wasm_runtime_available: the project's extension, @test/cmds, runs in
    // this runtime and contributes an MCP tool, a resource and two CLI
    // commands (one promoted, one shadowed by the explicit tool).
    let ext = std::sync::Arc::new(FakeExtension::new());
    let dir = fake_extension::project();
    let mut server = fake_extension::server_with(&ext);

    // No tool call is accepted before initialization completes.
    let early = call_tool(&mut server, "specforge.stats", json!({}));
    assert_eq!(early["error"]["code"], -32600, "{early}");

    let resp = call(
        &mut server,
        "initialize",
        json!({"projectRoot": dir.path().to_str().unwrap()}),
    );
    let result = &resp["result"];
    // No version asked for: the latest the server speaks.
    assert_eq!(result["protocolVersion"], "2025-11-25");
    assert_eq!(result["serverInfo"]["name"], "specforge-mcp");
    assert_eq!(result["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(result["capabilities"]["resources"]["subscribe"], true);

    // capabilities_returned: initialize lists nothing; the list methods
    // list every registered tool, resource and prompt.
    for key in ["tools", "resources", "prompts"] {
        assert!(
            result.get(key).is_none(),
            "initialize lists {key}: {result}"
        );
    }
    assert!(result["instructions"].is_string(), "{result}");
    let mut names = |method: &str, key: &str, field: &str| -> Vec<String> {
        call(&mut server, method, json!({}))["result"][key]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d[field].as_str().unwrap().to_string())
            .collect()
    };
    let tools = names("tools/list", "tools", "name");
    for core in [
        "specforge.query",
        "specforge.export",
        "specforge.trace",
        "specforge.inspect",
        "specforge.format",
        "specforge.doctor",
    ] {
        assert!(
            tools.contains(&core.to_string()),
            "{core} missing: {tools:?}"
        );
    }
    let resources = names("resources/list", "resources", "uri");
    assert!(resources.contains(&"specforge://graph".to_string()));
    assert!(resources.contains(&"specforge://diagnostics".to_string()));
    let prompts = names("prompts/list", "prompts", "name");
    assert!(prompts.contains(&"specforge://prompts/context".to_string()));

    // surface_contributions_merged: after the core tools and resources come
    // the extension's explicit tool, its promoted command, and its resource.
    let core_tools = crate::support::core_tools().len();
    assert_eq!(
        tools[core_tools..],
        ["specforge.cmds.check", "specforge.cmds.report"]
    );
    let core_resources = specforge_mcp::resources::CORE_RESOURCES.len();
    assert_eq!(
        resources.last().map(String::as_str),
        Some("specforge://ext/cmds/summary")
    );

    // compiler_api_available: the project root was located and used.
    // The project at the given root is served as its canonical path
    // (`ProjectRoot::of`, canonicalized).
    assert_eq!(
        server.state().project_root().map(canonical_path),
        Some(canonical_path(dir.path())),
        "initialize must adopt the projectRoot it was given"
    );
    assert_eq!(
        server
            .state()
            .registries()
            .extension_info()
            .collect::<Vec<_>>(),
        [(EXT, "0.1.0")]
    );

    // mcp_initialized_emitted, with the advertised counts.
    let initialized = events(&server, "mcp_initialized");
    assert_eq!(
        initialized,
        [json!({
            "tools_registered": core_tools + 2,
            "resources_registered": core_resources + 1,
            "prompts_registered": prompts.len(),
            "extensions_loaded": 1,
            "surface_tools_registered": 2,
            "surface_resources_registered": 1,
            "auto_promoted_tools": 1,
        })]
    );
    assert_eq!(tools.len(), core_tools + 2);
    // resources/list lists the five plain core resources (the three
    // templates are listed by resources/templates/list) and the extension's.
    assert_eq!(resources.len(), 5 + 1);

    // Tool calls are accepted once initialized.
    let after = call_tool(&mut server, "specforge.stats", json!({}));
    assert!(after["error"].is_null(), "{after}");
}

#[specforge_test(
    behavior = "mcp_shutdown",
    verify = "MCP Shutdown: MCP shutdown holds — server_initialized, notifications_flushed, subscriptions_removed, wasm_engines_released, shutdown_emitted"
)]
fn contract_shutdown() {
    let mut server = test_server();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://diagnostics"}),
    );
    // A compile left a graph notification pending for the subscriber: a
    // behavior written to disk, served by the next call.
    server.write("more.spec", "behavior gamma \"Gamma\" {\n}\n");
    call_tool(&mut server, "specforge.stats", json!({}));
    assert_eq!(server.state().subscriptions().pending(), 2);

    let resp = call(&mut server, "shutdown", json!({}));
    assert_eq!(resp["result"], json!({}), "{resp}");

    // notifications_flushed: the pending notification still reaches the
    // client after the shutdown response.
    let delivered = server.take_notifications();
    assert_eq!(delivered.len(), 2, "{delivered:?}");
    let mut added: Vec<&str> = delivered[1]["params"]["added_nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap())
        .collect();
    added.sort();
    assert_eq!(added, ["gamma"]);

    // subscriptions_removed: none left, and each removal was announced.
    assert!(server.state().subscriptions().is_empty());
    let mut removed = events(&server, "mcp_subscription_removed");
    removed.sort_by_key(|p| p["resourceUri"].to_string());
    assert_eq!(
        removed,
        [
            json!({"resourceUri": "specforge://diagnostics"}),
            json!({"resourceUri": "specforge://graph"}),
        ]
    );

    // wasm_engines_released: nothing compiled survives shutdown.
    let state = server.state();
    assert_eq!(state.graph().node_count(), 0);
    assert!(state.registries().declarations().is_empty());
    assert!(state.surfaces().tools().is_empty());
    assert!(state.surfaces().resources().is_empty());
    assert!(state.project_root().is_none());

    // shutdown_emitted, with what it released.
    let shutdown = events(&server, "mcp_server_shutdown");
    assert_eq!(shutdown.len(), 1, "{shutdown:?}");
    assert_eq!(shutdown[0]["pending_notifications_flushed"], 2);
    assert_eq!(shutdown[0]["subscriptions_released"], 2);
    // The served session's runtime went with it.
    assert_eq!(shutdown[0]["wasm_engines_released"], 1);

    // No new tool calls during teardown.
    let late = call_tool(&mut server, "specforge.stats", json!({}));
    assert_eq!(late["error"]["code"], -32600, "{late}");
}

/// The I020 report for an unknown kind in a `kinds` filter.
fn unknown_kind(kind: &str, suggestion: Option<&str>) -> Value {
    json!({
        "code": "I020",
        "title": "Unknown entity kind in a filter",
        "severity": "Info",
        "message": format!("unknown entity kind '{kind}'"),
        "span": null,
        "suggestion": suggestion.map(|s| format!("did you mean '{s}'?")),
        "file": null,
        "line": null,
        "column": null,
    })
}

#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "Provide MCP Query Tool: MCP query tool holds — graph_available, subgraph_returned, unknown_kinds_reported, tool_invoked_emitted"
)]
fn contract_query() {
    let mut server = test_server();
    // graph_available: the tool reads the server's compiled graph.
    assert_eq!(server.state().graph().node_count(), 2);
    let resp = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "alpha"}),
    );
    let parsed = tool_json(&resp);
    // subgraph_returned: alpha and its neighbor beta, with the edge.
    assert_eq!(node_ids(&parsed), ["alpha", "beta"]);
    assert_eq!(
        parsed["edges"],
        json!([{"source": "beta", "target": "alpha", "label": "behaviors"}])
    );
    assert_eq!(
        find(&parsed["nodes"], "id", "alpha")["fields"]["contract"],
        "MUST work"
    );
    // Only known kinds asked for: nothing to report.
    assert!(resp["result"]["_meta"].is_null(), "{resp}");

    // unknown_kinds_reported: the kind filter keeps only matching nodes,
    // unknown kinds are dropped, and each one is reported with I020 in the
    // response metadata, with a suggestion when a known kind is close.
    let filtered_args =
        json!({"entity_id": "alpha", "kinds": ["behavior", "behaviour", "nonexistent"]});
    let filtered = call_tool(&mut server, "specforge.query", filtered_args.clone());
    assert_ne!(filtered["result"]["isError"], true, "{filtered}");
    assert_eq!(node_ids(&tool_json(&filtered)), ["alpha"]);
    assert_eq!(
        filtered["result"]["_meta"]["diagnostics"],
        json!([
            unknown_kind("behaviour", Some("behavior")),
            unknown_kind("nonexistent", None),
        ])
    );

    // tool_invoked_emitted: one event per call, naming the entity.
    assert_eq!(
        events(&server, "mcp_tool_invoked"),
        [
            json!({
                "toolName": "specforge.query",
                "category": "core",
                "params": json!({"entity_id": "alpha"}).to_string(),
                "entityId": "alpha",
            }),
            json!({
                "toolName": "specforge.query",
                "category": "core",
                "params": filtered_args.to_string(),
                "entityId": "alpha",
            }),
        ]
    );
}

#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "Provide MCP Export Tool: MCP export tool holds — graph_available, format_produced, token_budget_enforced, tool_invoked_emitted"
)]
fn contract_export() {
    let mut server = test_server();

    // format_produced: each format carries the graph in its own shape.
    let graph = tool(&mut server, "specforge.export", json!({"format": "graph"}));
    assert_eq!(node_ids(&graph), ["alpha", "beta"]);
    assert_eq!(
        find(&graph["nodes"], "id", "alpha")["fields"]["contract"],
        "MUST work"
    );
    assert_eq!(find(&graph["nodes"], "id", "alpha")["file"], "test.spec");
    assert_eq!(
        graph["edges"],
        json!([{"source": "beta", "target": "alpha", "label": "behaviors"}])
    );

    let context = tool(
        &mut server,
        "specforge.export",
        json!({"format": "context"}),
    );
    let alpha = find(&context["nodes"], "id", "alpha");
    assert_eq!(alpha["contract"], "MUST work");
    assert_eq!(
        alpha["verify"],
        json!([{"kind": "unit", "description": "works"}])
    );
    assert!(
        alpha.get("fields").is_none(),
        "context flattens fields: {alpha}"
    );

    let brief = tool(&mut server, "specforge.export", json!({"format": "brief"}));
    assert_eq!(node_ids(&brief), ["alpha", "beta"]);
    let alpha = find(&brief["nodes"], "id", "alpha");
    assert_eq!(alpha["title"], "Alpha");
    assert!(alpha.get("contract").is_none() && alpha.get("fields").is_none());
    assert_eq!(brief["edges"].as_array().unwrap().len(), 1);

    // token_budget_enforced: an orphan with a long contract is the first
    // thing a tight budget drops; the connected pair stays.
    // gamma, written on test.spec lines 10–12.
    let long = "MUST ".repeat(200);
    server.write(
        "test.spec",
        &format!("{ALPHA_SPEC}\n\n\n\nbehavior gamma \"GAMMA\" {{\n    contract \"{long}\"\n}}\n"),
    );
    let budgeted = tool(
        &mut server,
        "specforge.export",
        json!({"format": "graph", "max_tokens": 200}),
    );
    assert_eq!(node_ids(&budgeted), ["alpha", "beta"], "{budgeted}");
    assert_eq!(budgeted["token_budget"]["budget_tokens"], 200);
    assert_eq!(
        budgeted["token_budget"]["truncated_entities"],
        json!(["gamma"])
    );

    assert_tool_invoked(&server, "specforge.export");
}

#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "max_tokens truncates output to fit token budget"
)]
fn contract_export_max_tokens() {
    let mut server = test_server();

    // Descriptor advertises max_tokens
    let resp = call(&mut server, "tools/list", json!({}));
    let export_tool = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "specforge.export")
        .expect("specforge.export descriptor must be registered");
    assert!(
        export_tool["inputSchema"]["properties"]["max_tokens"].is_object(),
        "export descriptor must declare max_tokens"
    );

    // Tool truncates graph output when max_tokens is set. The budget holds
    // the envelope and its truncation marker but not both entities; below
    // the envelope the export fails with E062.
    let resp = call_tool(
        &mut server,
        "specforge.export",
        json!({"format": "graph", "max_tokens": 50}),
    );
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(text).unwrap();
    assert!(
        parsed["token_budget"].is_object(),
        "graph export with max_tokens must include token_budget metadata"
    );
    assert!(
        parsed["nodes"].as_array().unwrap().len() < 2,
        "tiny budget must truncate the 2-node graph"
    );
}

#[specforge_test(
    behavior = "provide_mcp_trace_tool",
    verify = "Provide MCP Trace Tool: MCP trace tool holds — graph_available, trace_result_returned, gaps_identified, tool_invoked_emitted"
)]
fn contract_trace() {
    let mut server = test_server();

    // trace_result_returned: a TraceChain for an entity.
    let chain = tool(
        &mut server,
        "specforge.trace",
        json!({"entity_id": "alpha"}),
    );
    assert_eq!(chain["entity_id"], "alpha");
    assert_eq!(chain["entity_kind"], "behavior");
    assert_eq!(
        chain["upstream"],
        json!([{"entity_id": "beta", "entity_kind": "feature", "edge_label": "behaviors", "depth": 1, "status": "resolved"}])
    );
    assert_eq!(chain["downstream"], json!([]));
    // gaps_identified: the missing links, the expected edges an entity
    // lacks; isolation on a side is an empty list, not a gap.
    assert!(chain["missing"].is_array());
    assert!(chain.get("gaps").is_none(), "{chain}");
    let beta = tool(&mut server, "specforge.trace", json!({"entity_id": "beta"}));
    assert_eq!(beta["upstream"], json!([]));

    // ...or a McpTracePlanResult for a plan, flagging what the graph lacks.
    let plan = tool(
        &mut server,
        "specforge.trace",
        json!({"plan": agent_plan(&["alpha", "ghost"])}),
    );
    assert_eq!(plan["resolved_entries"], json!(["alpha"]));
    let gaps = plan["gaps"].as_array().unwrap();
    assert!(
        gaps.iter()
            .any(|g| g["target_entity"] == "ghost" && g["missing_link_type"] == "unresolved_entity"),
        "the unresolved plan entry is a gap: {gaps:?}"
    );

    assert_tool_invoked(&server, "specforge.trace");
}

#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "Provide MCP Search Tool: MCP search tool holds — graph_available, filtered_results_returned, unknown_kinds_reported, tool_invoked_emitted"
)]
fn contract_search() {
    let mut server = test_server();
    let ids = |resp: &Value| -> Vec<String> {
        tool_json(resp)["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["entity_id"].as_str().unwrap().to_string())
            .collect()
    };

    // graph_available: results come from the server's compiled graph.
    let by_text = call_tool(&mut server, "specforge.search", json!({"query": "alpha"}));
    assert_eq!(ids(&by_text), ["alpha"]);
    let hit = &tool_json(&by_text)["results"][0];
    assert_eq!(hit["kind"], "behavior");
    assert_eq!(hit["title"], "Alpha");
    assert_eq!(hit["file_path"], "test.spec");
    assert_eq!(hit["line"], 1);
    assert!(by_text["result"]["_meta"].is_null(), "{by_text}");

    // filtered_results_returned: filters combine with AND — kind feature
    // and an empty query is beta; kind feature and text alpha is nothing.
    let by_kind = call_tool(
        &mut server,
        "specforge.search",
        json!({"query": "", "kinds": ["feature"]}),
    );
    assert_eq!(ids(&by_kind), ["beta"]);
    let both = call_tool(
        &mut server,
        "specforge.search",
        json!({"query": "alpha", "kinds": ["feature"]}),
    );
    assert_eq!(ids(&both), Vec::<String>::new());

    // unknown_kinds_reported: unknown kinds match nothing, are not an
    // error, and each one is reported with I020 in the response metadata.
    let unknown = call_tool(
        &mut server,
        "specforge.search",
        json!({"query": "", "kinds": ["featur", "nonexistent"]}),
    );
    assert_ne!(unknown["result"]["isError"], true, "{unknown}");
    assert_eq!(ids(&unknown), Vec::<String>::new());
    assert_eq!(
        unknown["result"]["_meta"]["diagnostics"],
        json!([
            unknown_kind("featur", Some("feature")),
            unknown_kind("nonexistent", None),
        ])
    );
    // A known kind beside an unknown one still filters.
    let mixed = call_tool(
        &mut server,
        "specforge.search",
        json!({"query": "", "kinds": ["behavior", "behaviour"]}),
    );
    assert_eq!(ids(&mixed), ["alpha"]);
    assert_eq!(
        mixed["result"]["_meta"]["diagnostics"],
        json!([unknown_kind("behaviour", Some("behavior"))])
    );

    // tool_invoked_emitted: one event per call, carrying its arguments.
    let invoked = events(&server, "mcp_tool_invoked");
    let params: Vec<&str> = invoked
        .iter()
        .map(|p| {
            assert_eq!(p["toolName"], "specforge.search", "{p}");
            assert_eq!(p["category"], "core", "{p}");
            p["params"].as_str().unwrap()
        })
        .collect();
    assert_eq!(
        params,
        [
            json!({"query": "alpha"}).to_string(),
            json!({"query": "", "kinds": ["feature"]}).to_string(),
            json!({"query": "alpha", "kinds": ["feature"]}).to_string(),
            json!({"query": "", "kinds": ["featur", "nonexistent"]}).to_string(),
            json!({"query": "", "kinds": ["behavior", "behaviour"]}).to_string(),
        ]
    );
}

#[specforge_test(
    behavior = "provide_mcp_stats_tool",
    verify = "Provide MCP Stats Tool: MCP stats tool holds — graph_available, stats_returned, latest_state_reflected, tool_invoked_emitted"
)]
fn contract_stats() {
    // An orphan behavior, one nothing references, is a warning: a rule the
    // extension declares (W901, NoIncomingEdges on `behavior`).
    let mut server = contracts_project().serve(&[extension().declaring(|c| {
        c.rule("W901", |r| {
            r.check(CheckKind::NoIncomingEdges)
                .target_kind("behavior")
                .severity(ValidationSeverity::Warning)
                .message_template("behavior '{id}' is an orphan");
        });
    })]);
    let stats = tool(&mut server, "specforge.stats", json!({}));
    assert_eq!(
        stats["entity_counts"],
        json!([{"kind": "behavior", "count": 1}, {"kind": "feature", "count": 1}])
    );
    assert_eq!(stats["edge_count"], 1);
    assert_eq!(stats["unconnected_count"], 0);
    assert!(stats["declared_pct"].is_number(), "{stats}");
    assert_eq!(
        stats["diagnostic_summary"],
        json!({"errors": 0, "warnings": 0, "infos": 0})
    );

    // latest_state_reflected: a new orphan, written to disk (test.spec
    // line 10), and its warning show up at once.
    server.write(
        "test.spec",
        &format!("{ALPHA_SPEC}\n\n\n\nbehavior gamma \"GAMMA\" {{\n}}\n"),
    );
    let stats = tool(&mut server, "specforge.stats", json!({}));
    assert_eq!(
        stats["entity_counts"],
        json!([{"kind": "behavior", "count": 2}, {"kind": "feature", "count": 1}])
    );
    assert_eq!(stats["unconnected_count"], 1);
    assert_eq!(stats["diagnostic_summary"]["warnings"], 1);

    assert_tool_invoked(&server, "specforge.stats");
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "Provide MCP Inspect Tool: MCP inspect tool holds — graph_available, entity_details_returned, tool_invoked_emitted"
)]
fn contract_inspect() {
    // One diagnostic inside alpha's span (test.spec line 2), one in beta's
    // file (feat.spec line 2), as a check-phase pass reports them.
    let at = |file: &str, start_col| PassSpan {
        file: file.into(),
        start_line: 2,
        start_col,
        end_line: 2,
        end_col: start_col + 8,
    };
    let mut server = contracts_project().serve(&[extension()
        .reporting(PassDiagnostic::warning("W001", "inside alpha").with_span(at("test.spec", 5)))
        .reporting(PassDiagnostic::warning("W002", "inside beta").with_span(at("feat.spec", 5)))]);

    let details = tool(
        &mut server,
        "specforge.inspect",
        json!({"entity_id": "alpha"}),
    );
    assert_eq!(details["entity_id"], "alpha");
    assert_eq!(details["kind"], "behavior");
    assert_eq!(details["title"], "Alpha");
    assert_eq!(details["contract"], "MUST work");
    assert_eq!(details["fields"]["contract"], "MUST work");
    assert_eq!(details["verify_declarations"], json!(["unit works"]));
    assert_eq!(details["referenced_by"], json!(["beta"]));
    assert_eq!(details["coverage_status"], "uncovered");
    let codes: Vec<&str> = details["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["W001"]);
    assert_eq!(details["source_span"]["file"], "test.spec");
    assert_eq!(details["source_span"]["start_line"], 1);

    assert_tool_invoked(&server, "specforge.inspect");
}

#[specforge_test(
    behavior = "provide_mcp_find_definition_tool",
    verify = "Provide MCP Find Definition Tool: MCP find definition tool holds — graph_available, source_location_returned, tool_invoked_emitted"
)]
fn contract_find_definition() {
    // more/gamma.spec line 7 is `  behavior gamma "GAMMA" {`: a 2-column
    // indent.
    let mut server = contracts_project()
        .file(
            "more/gamma.spec",
            "\n\n\n\n\n\n  behavior gamma \"GAMMA\" {\n\n  }\n",
        )
        .serve(&[extension()]);

    let alpha = tool(
        &mut server,
        "specforge.find_definition",
        json!({"entity_id": "alpha"}),
    );
    // The position is the entity's name, read from its source, and the
    // answer says so.
    let location = |v: &Value| {
        json!([
            v["entity_id"],
            v["file_path"],
            v["line"],
            v["column"],
            v["precision"]
        ])
    };
    // test.spec line 1, `behavior alpha`: the name at column 10.
    assert_eq!(
        location(&alpha),
        json!(["alpha", "test.spec", 1, 10, "token"])
    );
    assert_eq!(
        alpha["name_span"],
        json!({"file": "test.spec", "start_line": 1, "start_col": 10, "end_line": 1, "end_col": 15})
    );
    assert_eq!(alpha["source_span"]["start_col"], 1);
    assert_eq!(alpha["source_span"]["end_line"], 5);
    let gamma = tool(
        &mut server,
        "specforge.find_definition",
        json!({"entity_id": "gamma"}),
    );
    // more/gamma.spec line 7: the keyword at column 3, the name at 12.
    assert_eq!(
        location(&gamma),
        json!(["gamma", "more/gamma.spec", 7, 12, "token"])
    );
    assert_eq!(gamma["source_span"]["start_col"], 3);

    assert_tool_invoked(&server, "specforge.find_definition");
}

#[specforge_test(
    behavior = "provide_mcp_find_references_tool",
    verify = "Provide MCP Find References Tool: MCP find references tool holds — graph_available, references_returned, empty_list_for_unreferenced, tool_invoked_emitted"
)]
fn contract_find_references() {
    let mut server = test_server();

    // references_returned: beta references alpha, with where beta lives.
    let refs = call_tool(
        &mut server,
        "specforge.find_references",
        json!({"entity_id": "alpha"}),
    );
    let refs = tool_json(&refs);
    assert_eq!(refs["entity_id"], "alpha");
    let locations = refs["locations"].as_array().unwrap();
    assert_eq!(locations.len(), 1, "{locations:?}");
    assert_eq!(locations[0]["referencing_entity_id"], "beta");
    // The token as written: feat.spec line 2, `    behaviors [alpha]`.
    assert_eq!(locations[0]["source_span"]["file"], "feat.spec");
    assert_eq!(locations[0]["source_span"]["start_line"], 2);
    assert_eq!(locations[0]["source_span"]["start_col"], 16);
    assert_eq!(locations[0]["precision"], "token");

    // empty_list_for_unreferenced: nothing references beta; not an error.
    let resp = call_tool(
        &mut server,
        "specforge.find_references",
        json!({"entity_id": "beta"}),
    );
    assert!(resp["error"].is_null(), "{resp}");
    assert!(resp["result"]["isError"].as_bool() != Some(true), "{resp}");
    assert_eq!(tool_json(&resp)["locations"], json!([]));

    assert_tool_invoked(&server, "specforge.find_references");
}

#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "Provide MCP Outline Tool: MCP outline tool holds — graph_available, outline_returned, tool_invoked_emitted"
)]
fn contract_outline() {
    // test.spec: alpha on lines 1–5, the port gamma on 7–12, its method
    // `check` on line 8.
    let mut server = contracts_project()
        .file(
            "test.spec",
            format!(
                "{ALPHA_SPEC}\nport gamma \"GAMMA\" {{\n    method check(x: Int) -> Bool\n    // a\n    // b\n    // c\n}}\n"
            ),
        )
        .serve(&[extension().kind("port", false)]);

    let outline = tool(
        &mut server,
        "specforge.outline",
        json!({"file": "test.spec"}),
    )["entries"]
        .clone();
    // Every entity in test.spec (beta lives in feat.spec), by line.
    let entries = outline.as_array().unwrap();
    let ids: Vec<&str> = entries
        .iter()
        .map(|e| e["entity_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["alpha", "gamma"]);
    assert_eq!(entries[0]["kind"], "behavior");
    assert_eq!(entries[0]["title"], "Alpha");
    assert_eq!(entries[0]["range"]["start_line"], 1);
    assert_eq!(entries[0]["range"]["end_line"], 5);
    assert_eq!(entries[1]["kind"], "port");
    assert_eq!(entries[1]["range"]["start_line"], 7);
    assert_eq!(entries[1]["range"]["end_line"], 12);
    // Nested children: gamma's method member.
    let children = entries[1]["children"].as_array().unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0]["entity_id"], "gamma.check");
    assert_eq!(children[0]["kind"], "method");
    assert_eq!(children[0]["title"], "check(x: Int) -> Bool");
    assert_eq!(children[0]["range"]["start_line"], 8);

    assert_tool_invoked(&server, "specforge.outline");
}

#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "Provide MCP Coverage Tool: MCP coverage tool holds — graph_available, coverage_returned, testability_respected, tool_invoked_emitted"
)]
fn contract_coverage() {
    let mut server = test_server();

    // coverage_returned: behaviors are testable, features are not.
    let coverage = tool(&mut server, "specforge.coverage", json!({}))["entities"].clone();
    assert_eq!(
        coverage,
        json!([{
            "entity_id": "alpha",
            "kind": "behavior",
            "status": "uncovered",
            "declared": true,
            "linked": false,
            "evidence_collected": false,
            "obligations": 1,
            "proven": 0,
            "unproven": ["works"],
            "exempt": false,
        }])
    );

    // Recorded evidence: a passing test that names the obligation.
    server.write(
        "specforge-report.json",
        &json!({"results": {"alpha": {"tests": [
            {"name": "alpha_works", "status": "passed", "verify": "works"}
        ]}}})
        .to_string(),
    );
    let coverage = tool(&mut server, "specforge.coverage", json!({}))["entities"].clone();
    let alpha = find(&coverage, "entity_id", "alpha");
    assert_eq!(alpha["status"], "covered");
    assert_eq!(alpha["proven"], 1);
    assert_eq!(alpha["linked"], true);
    assert_eq!(alpha["unproven"], json!([]));

    // testability_respected: the registry, not the kind name, decides. A
    // testable feature no rule obliges to declare obligations owes none:
    // it does not count, and is reachable by id, exempt. Each declaration
    // set is a project of its own.
    let testable_feature = extension().kind("feature", true);
    let mut server = contracts_project().serve(std::slice::from_ref(&testable_feature));
    let coverage = tool(&mut server, "specforge.coverage", json!({}))["entities"].clone();
    assert!(
        coverage
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["entity_id"] != "beta")
    );
    let beta = tool(
        &mut server,
        "specforge.coverage",
        json!({"entity_id": "beta"}),
    )["entities"]
        .clone();
    assert_eq!(beta[0]["exempt"], true, "{beta}");
    assert_eq!(beta[0]["obligations"], 0);
    // Once a rule obliges features to declare obligations, beta counts.
    let mut server = contracts_project().serve(&[testable_feature.obligating("feature")]);
    let coverage = tool(&mut server, "specforge.coverage", json!({}))["entities"].clone();
    let beta = find(&coverage, "entity_id", "beta");
    assert_eq!(beta["obligations"], 0);
    assert_eq!(beta["status"], "uncovered");
    assert_eq!(beta["exempt"], false);

    assert_tool_invoked(&server, "specforge.coverage");
}

#[specforge_test(
    behavior = "provide_mcp_schema_tool",
    verify = "Provide MCP Schema Tool: MCP schema tool holds — graph_available, schema_returned, tool_invoked_emitted"
)]
fn contract_schema() {
    // graph_available: a compiled project whose extension declares kinds.
    let project = project_dir(
        json!({"name": "t", "version": "0.1.0", "extensions": ["@specforge/software"]}),
        "behavior act \"Act\" {\n  contract \"MUST act\"\n}\n",
    );
    let mut server = logged(McpServer::new());
    call(
        &mut server,
        "initialize",
        json!({"projectRoot": project.path().to_str().unwrap()}),
    );

    // schema_returned: the GraphProtocolSchema a full export embeds.
    let schema = tool(&mut server, "specforge.schema", json!({}));
    let export = tool(&mut server, "specforge.export", json!({"format": "graph"}));
    assert_eq!(schema, export["schema"]);
    assert_eq!(schema["extensions"][0]["name"], "@specforge/software");

    // Optionally filtered to one kind.
    let port = tool(&mut server, "specforge.schema", json!({"kind": "port"}));
    let kinds: Vec<&str> = port["entity_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["name"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["port"]);

    assert_tool_invoked(&server, "specforge.schema");
}

#[specforge_test(
    behavior = "provide_mcp_context_prompt",
    verify = "Provide MCP Context Prompt: MCP context prompt holds — graph_available, context_returned, hints_included, prompt_invoked_emitted"
)]
fn contract_context_prompt() {
    // An invariant nothing connects to alpha (inv.spec).
    let mut server = contracts_project()
        .file(
            "inv.spec",
            "invariant gamma \"GAMMA\" {\n    guarantee \"never negative\"\n}\n",
        )
        .serve(&[extension()]);

    let context = prompt_payload(&get_prompt(
        &mut server,
        "specforge://prompts/context",
        json!({"entity_id": "alpha", "structural_constraints": ["gamma"]}),
    ));
    // context_returned: contract, related entities, verify declarations.
    assert_eq!(context["entity_id"], "alpha");
    assert_eq!(context["kind"], "behavior");
    assert_eq!(context["contract_text"], "MUST work");
    assert_eq!(context["upstream_entities"], json!(["beta"]));
    assert_eq!(context["downstream_entities"], json!([]));
    assert_eq!(context["verify_expectations"], json!(["unit works"]));
    // hints_included: gamma rides along although no edge reaches it.
    assert_eq!(context["structural_constraints"], json!(["gamma"]));
    let hint = &context["structural_constraint_entities"][0];
    assert_eq!(hint["entity_id"], "gamma");
    assert_eq!(hint["kind"], "invariant");
    assert_eq!(hint["fields"]["guarantee"], "never negative");

    // As an MCP prompt argument string, the list is comma-separated.
    let context = prompt_payload(&get_prompt(
        &mut server,
        "specforge://prompts/context",
        json!({"entity_id": "alpha", "structural_constraints": "gamma, beta"}),
    ));
    assert_eq!(context["structural_constraints"], json!(["gamma", "beta"]));

    assert_prompt_invoked(&server, "specforge://prompts/context");
}

#[specforge_test(
    behavior = "provide_mcp_review_prompt",
    verify = "Provide MCP Review Prompt: MCP review prompt holds — graph_available, coverage_analysis_returned, gaps_identified, prompt_invoked_emitted"
)]
fn contract_review_prompt() {
    // gamma (test.spec 7–9): a testable behavior of beta with no verify
    // declarations; delta (test.spec 11–13): gamma depends on it, two hops
    // from beta, outside depth 1. A behavior must declare obligations, as
    // @specforge/software says.
    let mut server = contracts_project()
        .file(
            "test.spec",
            format!(
                "{ALPHA_SPEC}\nbehavior gamma \"GAMMA\" {{\n    depends_on [delta]\n}}\n\nbehavior delta \"DELTA\" {{\n\n}}\n"
            ),
        )
        .file(
            "feat.spec",
            "feature beta \"Beta\" {\n    behaviors [alpha, gamma]\n}\n",
        )
        .serve(&[extension()
            .obligating("behavior")
            .reference("behavior", "depends_on", "behavior")]);

    let review = prompt_payload(&get_prompt(
        &mut server,
        "specforge://prompts/review",
        json!({"entity_id": "beta", "depth": 1}),
    ));
    assert_eq!(review["entity_id"], "beta");

    // coverage_analysis_returned: the testable entities within one hop.
    let summary = review["coverage_summary"].as_array().unwrap();
    let covered: Vec<&str> = summary
        .iter()
        .map(|c| c["entity_id"].as_str().unwrap())
        .collect();
    assert_eq!(covered, ["alpha", "gamma"]);
    let alpha = find(&review["coverage_summary"], "entity_id", "alpha");
    assert_eq!(alpha["status"], "uncovered");
    // gaps_identified: the uncovered verify text and the missing evidence.
    assert_eq!(alpha["unproven"], json!(["works"]));
    assert_eq!(alpha["linked"], false);
    assert_eq!(alpha["evidence_collected"], false);
    // ...and missing verification coverage.
    let findings = review["findings"].as_array().unwrap();
    assert!(
        findings.iter().any(|f| f["entity_id"] == "gamma"
            && f["severity"] == "warning"
            && f["message"]
                .as_str()
                .unwrap()
                .contains("no verify declarations")),
        "{findings:?}"
    );
    assert!(
        !findings.iter().any(|f| f["entity_id"] == "alpha"),
        "alpha declares verify: {findings:?}"
    );

    assert_prompt_invoked(&server, "specforge://prompts/review");
}

#[specforge_test(
    behavior = "provide_mcp_trace_prompt",
    verify = "Provide MCP Trace Prompt: MCP trace prompt holds — graph_available, gaps_returned, affected_entities_listed, prompt_invoked_emitted"
)]
fn contract_trace_prompt() {
    let mut server = test_server();

    // affected_entities_listed: the plan's entry and what its chain reaches.
    let trace = prompt_payload(&get_prompt(
        &mut server,
        "specforge://prompts/trace",
        json!({"plan": agent_plan(&["alpha", "ghost"])}),
    ));
    assert_eq!(trace["affected_entities"], json!(["alpha", "beta"]));
    // alpha counts toward coverage and is not proven; beta, a feature, is
    // not testable (kind_entry("feature", false)).
    assert_eq!(trace["unverified_entities"], json!(["alpha"]));

    // gaps_returned: the entry the graph lacks, with its gap context.
    let gaps = trace["coverage_gaps"].as_array().unwrap();
    let ghost = gaps
        .iter()
        .find(|g| g["target_entity"] == "ghost")
        .unwrap_or_else(|| panic!("no gap for the unresolved entry: {gaps:?}"));
    assert!(
        ghost["gap_context"].as_str().is_some_and(|c| !c.is_empty()),
        "{ghost}"
    );
    // Deterministic: the same plan yields the same gaps.
    let again = prompt_payload(&get_prompt(
        &mut server,
        "specforge://prompts/trace",
        json!({"plan": agent_plan(&["alpha", "ghost"])}),
    ));
    assert_eq!(again, trace);

    assert_prompt_invoked(&server, "specforge://prompts/trace");
}

#[specforge_test(
    behavior = "provide_mcp_explore_prompt",
    verify = "Provide MCP Explore Prompt: MCP explore prompt holds — graph_available, exploration_returned, bfs_from_entity, prompt_invoked_emitted"
)]
fn contract_explore_prompt() {
    // alpha <-behaviors- beta -invariants-> gamma (inv.spec); delta
    // (test.spec 11–13) is unconnected.
    let mut server = contracts_project()
        .file(
            "test.spec",
            format!("{ALPHA_SPEC}\n\n\n\n\nbehavior delta \"DELTA\" {{\n\n}}\n"),
        )
        .file("feat.spec", BETA_WITH_GAMMA)
        .file("inv.spec", "invariant gamma \"GAMMA\" {\n}\n")
        .serve(&[extension()]);

    // exploration_returned: starting points, hubs and unconnected entities.
    let explore = prompt_payload(&get_prompt(
        &mut server,
        "specforge://prompts/explore",
        json!({}),
    ));
    assert_eq!(explore["high_connectivity"][0], "beta");
    assert_eq!(explore["unconnected"], json!(["delta"]));
    assert_eq!(explore["starting_points"][0], "beta");
    assert_eq!(explore["relationship_paths"], json!([]));

    // bfs_from_entity: paths from alpha, nearest first.
    let from_alpha = prompt_payload(&get_prompt(
        &mut server,
        "specforge://prompts/explore",
        json!({"entity_id": "alpha"}),
    ));
    assert_eq!(
        from_alpha["relationship_paths"],
        json!([
            {"from_entity": "alpha", "to_entity": "beta", "edge_types": ["behaviors"], "path_length": 1},
            {"from_entity": "alpha", "to_entity": "gamma", "edge_types": ["behaviors", "invariants"], "path_length": 2},
        ])
    );
    // A kind filter keeps the paths that end at that kind.
    let invariants = prompt_payload(&get_prompt(
        &mut server,
        "specforge://prompts/explore",
        json!({"entity_id": "alpha", "kind": "invariant"}),
    ));
    let ends: Vec<&str> = invariants["relationship_paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["to_entity"].as_str().unwrap())
        .collect();
    assert_eq!(ends, ["gamma"]);

    assert_prompt_invoked(&server, "specforge://prompts/explore");
}

/// `fake` also declaring the MCP tool `ext.<name>` and the resource
/// `specforge://ext/<name>`.
fn with_extension_surface(
    fake: crate::fake_extension::FakeExtension,
    name: &str,
) -> crate::fake_extension::FakeExtension {
    fake.with_tool(
        json!({"name": format!("ext.{name}"), "description": format!("{name} tool"),
        "export": format!("export_{name}"), "input_schema": {"type": "object"}}),
    )
    .with_resource(json!({"uri_template": format!("specforge://ext/{name}"),
            "name": format!("ext-{name}"), "export": format!("export_{name}_resource"),
            "mime_type": "application/json"}))
}

#[specforge_test(
    behavior = "list_mcp_tools",
    verify = "List MCP Tools: listing MCP tools holds — server_initialized, complete_list_returned, discovery_emitted, listed_once"
)]
fn contract_list_tools() {
    use crate::fake_extension::{self, FakeExtension};
    // server_initialized: over a project whose extension contributes an
    // MCP tool and two CLI commands.
    let (mut server, _ext, _dir) =
        fake_extension::initialized(with_extension_surface(FakeExtension::new(), "on"));

    let resp = call(&mut server, "tools/list", json!({}));
    let names: Vec<&str> = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for core in [
        "specforge.query",
        "specforge.validate",
        "specforge.export",
        "specforge.trace",
        "specforge.search",
        "specforge.schema",
        "specforge.coverage",
        "specforge.stats",
        "specforge.analyze",
        "specforge.inspect",
        "specforge.find_definition",
        "specforge.find_references",
        "specforge.outline",
        "specforge.suggest_fixes",
        "specforge.format",
        "specforge.rename",
        "specforge.init",
        "specforge.add_extension",
        "specforge.remove_extension",
        "specforge.migrate",
        "specforge.extensions",
        "specforge.providers",
        "specforge.doctor",
        "specforge.collect",
        "specforge.render",
        "specforge.list",
    ] {
        assert!(names.contains(&core), "{core} missing: {names:?}");
    }
    // complete_list_returned: every core tool, then the extension's
    // explicit tools, then its auto-promoted commands.
    let core: Vec<String> = crate::support::core_tools()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names[..core.len()], core);
    assert_eq!(
        names[core.len()..],
        ["specforge.cmds.check", "ext.on", "specforge.cmds.report"]
    );
    // listed_once: each name once; the command whose tool name the explicit
    // check has is reported, not listed.
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "{names:?}");
    assert!(
        server
            .state()
            .diagnostics()
            .iter()
            .any(|d| d.code == "I017" && d.message.starts_with("command 'check'")),
        "the shadowed command is reported"
    );

    // discovery_emitted: the count is what the client got.
    assert_eq!(
        events(&server, "mcp_discovery_invoked"),
        [json!({"discoveryType": "tools", "resultCount": core.len() + 3})]
    );
}

#[specforge_test(
    behavior = "auto_promote_commands_to_mcp_tools",
    verify = "Auto-Promote Commands to MCP Tools: command-to-MCP-tool auto-promotion holds — surfaces_registered, all_commands_promoted, naming_convention_enforced, explicit_tool_wins, commands_auto_promoted_emitted, schema_is_the_declaration"
)]
fn contract_auto_promote_commands() {
    use crate::fake_extension::{self, EXT, FakeExtension};
    use specforge_mcp::surface_table::ToolKind;
    let output = json!({"exit_code": 0, "stdout": "report written", "stderr": ""});
    let (mut server, ext, _dir) =
        fake_extension::initialized(FakeExtension::new().with_output("cmd__report", output));

    // surfaces_registered: the compile registered the
    // extension's contributions, commands included.
    let registries = server.state().registries();
    let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
    let routed: Vec<_> = (registries.commands().routed().iter())
        .map(|c| pair(&c.declaration().id, &c.declaration().export))
        .collect();
    assert_eq!(
        routed,
        [pair("report", "cmd__report"), pair("check", "cmd__check")]
    );
    let tools: Vec<_> = (registries.declarations().iter())
        .flat_map(|d| &d.surfaces.mcp_tools)
        .map(|t| pair(&t.name, &t.export))
        .collect();
    assert_eq!(tools, [pair("specforge.cmds.check", "mcp__check")]);

    // all_commands_promoted + naming_convention_enforced: every command
    // becomes specforge.cmds.<id> unless an explicit tool has the name.
    let promoted: Vec<(String, String)> = server
        .state()
        .surfaces()
        .tools()
        .iter()
        .filter_map(|tool| match &tool.kind {
            ToolKind::Command(command) => Some(pair(&tool.name, command.export())),
            ToolKind::McpTool { .. } => None,
        })
        .collect();
    assert_eq!(promoted, [pair("specforge.cmds.report", "cmd__report")]);
    // schema_is_the_declaration: each arg's type, values and description;
    // the required non-flag args; nothing undeclared.
    let report = find(
        &call(&mut server, "tools/list", json!({}))["result"]["tools"],
        "name",
        "specforge.cmds.report",
    )
    .clone();
    assert_eq!(
        report["inputSchema"],
        json!({
            "type": "object",
            "properties": {
                "style": {"type": "string", "enum": ["md", "json"], "description": "Output style"},
                "verbose": {"type": "boolean", "default": false},
                "limit": {"type": "integer"},
                "out": {"type": "string"}
            },
            "required": ["style"],
            "additionalProperties": false
        })
    );
    let resp = call_tool(
        &mut server,
        "specforge.cmds.report",
        json!({"style": "json"}),
    );
    assert_eq!(resp["result"]["content"][0]["text"], "report written");
    let calls = ext.calls();
    assert_eq!(
        calls
            .iter()
            .map(|(ext, export, input)| (ext.as_str(), export.as_str(), &input["args"]))
            .collect::<Vec<_>>(),
        [(
            EXT,
            "cmd__report",
            &json!({"style": "json", "verbose": false})
        )]
    );

    // explicit_tool_wins: `check` stays the explicit tool, with I017.
    let check = find(
        &call(&mut server, "tools/list", json!({}))["result"]["tools"],
        "name",
        "specforge.cmds.check",
    )
    .clone();
    assert_eq!(check["description"], "Explicit check tool");
    let diagnostics = server.state().diagnostics();
    let i017: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "I017")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        i017,
        [
            "command 'check' not auto-promoted: explicit MCP tool 'specforge.cmds.check' already exists"
        ]
    );

    // commands_auto_promoted_emitted: once, with the counts.
    assert_eq!(
        events(&server, "commands_auto_promoted"),
        [json!({"promotedCount": 1, "conflictCount": 1})]
    );
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "Dispatch Surface Command: surface command dispatch holds — command_declared, args_serialized, sandbox_restricted, traps_caught, output_returned, surface_command_dispatched_emitted, args_normalized_by_the_host, evidence_carried, one_operation"
)]
fn contract_dispatch_surface_command() {
    // The sandbox probe (fixtures/sandbox-probe), in the component runtime
    // every extension runs in. Its `probe` command reports its input and
    // what it got when it tried every capability; its `trap` command panics.
    let probe = specforge_component::committed::PROBE.path();
    let runtime = specforge_component::ComponentRuntime::new();
    runtime
        .load("@test/probe", &std::fs::read(&probe).unwrap())
        .unwrap();
    let dir = project_dir(
        json!({"name": "probed", "version": "0.1.0", "extensions": ["@test/probe"]}),
        "probe_target t1 \"Target\" {\n}\n",
    );
    // The probe is installed as any extension is, so the project loads it.
    specforge_installed::testing::install_module(
        dir.path(),
        "@test/probe",
        &std::fs::read(&probe).unwrap(),
    );
    std::fs::write(dir.path().join("secret.txt"), "secret").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut server = logged(McpServer::new().with_extension_runtime(std::sync::Arc::new(runtime)));
    let init = call(
        &mut server,
        "initialize",
        json!({"projectRoot": dir.path().to_str().unwrap()}),
    );
    assert!(init["error"].is_null(), "{init}");
    let root = server
        .state()
        .project_root()
        .map(std::path::Path::to_path_buf)
        .unwrap();

    // command_declared: the commands the probe declares are its tools.
    let tools = call(&mut server, "tools/list", json!({}));
    for name in ["specforge.probe.probe", "specforge.probe.trap"] {
        find(&tools["result"]["tools"], "name", name);
    }

    let resp = call_tool(&mut server, "specforge.probe.probe", json!({"port": port}));
    // output_returned: its stdout, then its stderr; exit code 3 fails the
    // call.
    let result = &resp["result"];
    assert_eq!(result["isError"], true, "{resp}");
    assert_eq!(result["content"][1]["text"], "probed\n", "{resp}");
    let out = tool_json(&resp);

    // args_serialized: the args, the project root and the served graph.
    assert_eq!(out["args"], json!({"port": port}));
    assert_eq!(
        out["cwd"],
        std::fs::canonicalize(&root).unwrap().display().to_string()
    );
    assert_eq!(out["nodes"], json!(["t1"]));

    // sandbox_restricted: though its declaration asks for every capability,
    // the export could not read or write the project root, see the
    // environment, its arguments or stdin, connect, or resolve a name.
    let sandbox = &out["sandbox"];
    for attempt in [
        "read_root",
        "read_dir",
        "read_file",
        "write_file",
        "connect",
        "resolve",
    ] {
        assert_eq!(sandbox[attempt]["granted"], false, "{attempt}: {sandbox}");
    }
    for empty in ["env_vars", "args", "stdin_bytes"] {
        assert_eq!(sandbox[empty], 0, "{empty}: {sandbox}");
    }
    assert!(!dir.path().join("probe.txt").exists());
    assert_eq!(
        listener.accept().map(|_| ()).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );

    // surface_command_dispatched_emitted: once the export returned.
    let mut dispatched = events(&server, "surface_command_dispatched");
    assert_eq!(dispatched.len(), 1, "{dispatched:?}");
    let duration = dispatched[0]
        .as_object_mut()
        .unwrap()
        .remove("durationMs")
        .unwrap();
    assert!(duration.is_u64(), "{duration}");
    assert_eq!(
        dispatched,
        [json!({"extensionName": "@test/probe", "commandId": "probe", "exitCode": 3})]
    );

    // traps_caught: a panicking command is the tool's E028 error, and no
    // dispatch is recorded.
    let resp = call_tool(&mut server, "specforge.probe.trap", json!({}));
    let error = crate::tool_errors::mcp_error(&resp);
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .starts_with("command cmd__trap() of '@test/probe' trapped: call_failed: "),
        "{error}"
    );
    assert_eq!(events(&server, "surface_command_dispatched").len(), 1);

    // args_normalized_by_the_host: the export gets the args the command
    // line would send, each its declared type; an argument the declaration
    // refuses is the command's INVALID_INPUT object, and it does not run.
    let resp = call_tool(
        &mut server,
        "specforge.probe.probe",
        json!({"port": port.to_string()}),
    );
    assert_eq!(tool_json(&resp)["args"], json!({"port": port}), "{resp}");
    assert_eq!(events(&server, "surface_command_dispatched").len(), 2);
    for (arguments, message) in [
        (json!({"port": "x"}), "port must be an integer, got 'x'"),
        (json!({"dir": "/"}), "unknown argument 'dir'"),
    ] {
        let resp = call_tool(&mut server, "specforge.probe.probe", arguments.clone());
        assert_eq!(resp["result"]["isError"], true, "{arguments}: {resp}");
        assert_eq!(
            resp["result"]["structuredContent"],
            json!({"code": "INVALID_INPUT", "message": message}),
            "{arguments}"
        );
    }
    assert_eq!(events(&server, "surface_command_dispatched").len(), 2);
}

#[specforge_test(
    behavior = "surface_command_dispatched",
    verify = "emits surface_command_dispatched with correct commandId and exitCode"
)]
fn event_surface_command_dispatched() {
    use crate::fake_extension::{self, FakeExtension};
    let (mut server, _ext, _dir) = fake_extension::initialized(FakeExtension::new().with_output(
        "cmd__report",
        json!({"exit_code": 0, "stdout": "ok", "stderr": ""}),
    ));
    call_tool(&mut server, "specforge.cmds.report", json!({"style": "md"}));
    let dispatched: Vec<(Value, Value, Value)> = events(&server, "surface_command_dispatched")
        .into_iter()
        .map(|e| {
            (
                e["extensionName"].clone(),
                e["commandId"].clone(),
                e["exitCode"].clone(),
            )
        })
        .collect();
    assert_eq!(
        dispatched,
        [(json!(fake_extension::EXT), json!("report"), json!(0))]
    );
}

/// The recorded `name` events, each without its `durationMs`, which must
/// be a count of milliseconds.
fn timed_events(server: &impl Logs, name: &str) -> Vec<Value> {
    events(server, name)
        .into_iter()
        .map(|mut e| {
            let duration = e.as_object_mut().unwrap().remove("durationMs");
            assert!(duration.as_ref().is_some_and(Value::is_u64), "{e}");
            e
        })
        .collect()
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_tool",
    verify = "a returned tool call is recorded as a surface_mcp_tool_dispatched event"
)]
fn a_returned_tool_call_is_a_dispatched_tool() {
    use crate::fake_extension::{self, EXT, FakeExtension};
    let dispatched = |success: bool| json!({"extensionName": EXT, "toolName": "specforge.cmds.check", "success": success});
    // Its export returned what the tool's output schema describes.
    let (mut server, _ext, _dir) = fake_extension::initialized(
        FakeExtension::new().with_output("mcp__check", json!({"checked": true})),
    );
    call_tool(&mut server, "specforge.cmds.check", json!({}));
    assert_eq!(
        timed_events(&server, "surface_mcp_tool_dispatched"),
        [dispatched(true)]
    );
    // It returned an output the schema refuses: dispatched, and failed.
    let (mut server, _ext, _dir) = fake_extension::initialized(
        FakeExtension::new().with_output("mcp__check", json!({"checked": "yes"})),
    );
    call_tool(&mut server, "specforge.cmds.check", json!({}));
    assert_eq!(
        timed_events(&server, "surface_mcp_tool_dispatched"),
        [dispatched(false)]
    );
    // It trapped, or the input was refused before it ran: not dispatched.
    let (mut server, _ext, _dir) = fake_extension::initialized(FakeExtension::new());
    call_tool(&mut server, "specforge.cmds.check", json!({}));
    call_tool(&mut server, "specforge.cmds.check", json!({"strict": "no"}));
    assert!(events(&server, "surface_mcp_tool_dispatched").is_empty());
}

#[specforge_test(
    behavior = "surface_mcp_tool_dispatched",
    verify = "emits surface_mcp_tool_dispatched with correct toolName and success"
)]
fn event_surface_mcp_tool_dispatched() {
    use crate::fake_extension::{self, FakeExtension};
    let (mut server, _ext, _dir) = fake_extension::initialized(
        FakeExtension::new().with_output("mcp__check", json!({"checked": true})),
    );
    call_tool(&mut server, "specforge.cmds.check", json!({"strict": true}));
    let dispatched: Vec<(Value, Value)> = events(&server, "surface_mcp_tool_dispatched")
        .into_iter()
        .map(|e| (e["toolName"].clone(), e["success"].clone()))
        .collect();
    assert_eq!(dispatched, [(json!("specforge.cmds.check"), json!(true))]);
    // An auto-promoted command is a dispatched command, not a tool.
    assert!(events(&server, "surface_command_dispatched").is_empty());
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_resource",
    verify = "a returned resource read is recorded as a surface_mcp_resource_dispatched event"
)]
fn a_returned_resource_read_is_a_dispatched_resource() {
    use crate::fake_extension::{self, EXT, FakeExtension};
    let summary = "specforge://ext/cmds/summary";
    let (mut server, _ext, _dir) = fake_extension::initialized(FakeExtension::new().with_output(
        "mcp__summary",
        json!({"content": "{}", "mime_type": "application/json"}),
    ));
    resource(&mut server, summary);
    assert_eq!(
        timed_events(&server, "surface_mcp_resource_dispatched"),
        [json!({"extensionName": EXT, "uriTemplate": summary, "mimeType": "application/json"})]
    );
    // A core resource is not an extension's; a trapping read is not one
    // that returned.
    resource(&mut server, "specforge://graph");
    assert_eq!(events(&server, "surface_mcp_resource_dispatched").len(), 1);
    let (mut server, _ext, _dir) = fake_extension::initialized(FakeExtension::new());
    call(&mut server, "resources/read", json!({"uri": summary}));
    assert!(events(&server, "surface_mcp_resource_dispatched").is_empty());
}

#[specforge_test(
    behavior = "surface_mcp_resource_dispatched",
    verify = "emits surface_mcp_resource_dispatched with correct uriTemplate"
)]
fn event_surface_mcp_resource_dispatched() {
    use crate::fake_extension::{self, FakeExtension};
    let (mut server, _ext, _dir) = fake_extension::initialized(FakeExtension::new().with_output(
        "mcp__summary",
        json!({"content": "{}", "mime_type": "application/json"}),
    ));
    resource(&mut server, "specforge://ext/cmds/summary");
    let templates: Vec<Value> = events(&server, "surface_mcp_resource_dispatched")
        .into_iter()
        .map(|e| e["uriTemplate"].clone())
        .collect();
    assert_eq!(templates, [json!("specforge://ext/cmds/summary")]);
}

#[specforge_test(
    behavior = "list_mcp_resources",
    verify = "List MCP Resources: listing MCP resources holds — server_initialized, complete_list_returned, discovery_emitted"
)]
fn contract_list_resources() {
    use crate::fake_extension::{self, FakeExtension};
    let (mut server, _ext, _dir) = fake_extension::initialized(with_extension_surface(
        FakeExtension::declaring(json!({})),
        "on",
    ));

    let resp = call(&mut server, "resources/list", json!({}));
    let mut uris: Vec<&str> = resp["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    uris.sort();
    // complete_list_returned: every core resource plus the extension's,
    // templated ones under resources/templates/list.
    assert_eq!(
        uris,
        [
            "specforge://brief",
            "specforge://context",
            "specforge://diagnostics",
            "specforge://ext/on",
            "specforge://graph",
            "specforge://schema",
        ]
    );
    let templates = call(&mut server, "resources/templates/list", json!({}));
    let mut uri_templates: Vec<&str> = templates["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();
    uri_templates.sort();
    assert_eq!(
        uri_templates,
        [
            "specforge://context/{entity_id}",
            "specforge://entities/{kind}",
            "specforge://graph/{entity_id}",
        ]
    );

    // The count is what the client got.
    assert_eq!(
        events(&server, "mcp_discovery_invoked"),
        [
            json!({"discoveryType": "resources", "resultCount": uris.len()}),
            json!({"discoveryType": "resource_templates", "resultCount": uri_templates.len()}),
        ]
    );

    // server_initialized: an uninitialized server lists nothing.
    let mut fresh = McpServer::new();
    let resp = call(&mut fresh, "resources/list", json!({}));
    assert_eq!(resp["error"]["code"], -32600, "{resp}");
}

#[specforge_test(
    behavior = "list_mcp_prompts",
    verify = "List MCP Prompts: listing MCP prompts holds — server_initialized, complete_list_returned, discovery_emitted"
)]
#[specforge_test(
    behavior = "list_mcp_prompts",
    verify = "lists every core prompt, each one prompts/get serves"
)]
fn contract_list_prompts() {
    let mut server = test_server();

    // The listing is the Prompt spec table: every prompt listed is one
    // prompts/get serves.
    let resp = call(&mut server, "prompts/list", json!({}));
    let mut names: Vec<&str> = resp["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "specforge://prompts/context",
            "specforge://prompts/explore",
            "specforge://prompts/infer",
            "specforge://prompts/review",
            "specforge://prompts/trace",
        ]
    );

    // The count is what the client got.
    assert_eq!(
        events(&server, "mcp_discovery_invoked"),
        [json!({"discoveryType": "prompts", "resultCount": names.len()})]
    );
    for name in names {
        let get = call(&mut server, "prompts/get", json!({"name": name}));
        let message = get["error"]["message"].as_str().unwrap_or_default();
        assert!(!message.starts_with("Unknown prompt"), "{name}: {get}");
    }

    let mut fresh = McpServer::new();
    let resp = call(&mut fresh, "prompts/list", json!({}));
    assert_eq!(resp["error"]["code"], -32600, "{resp}");
}

#[specforge_test(
    behavior = "guard_mcp_reinitialization",
    verify = "Guard MCP Reinitialization: MCP reinitialization guard holds — server_initialized, reinit_rejected, session_unaffected, error_handled_emitted"
)]
fn contract_guard_reinit() {
    let mut server = test_server();
    let root = server
        .state()
        .project_root()
        .map(std::path::Path::to_path_buf);
    let tools = specforge_mcp::registry::listed_tools(server.state(), latest()).count();
    let other = tempfile::TempDir::new().unwrap();

    // reinit_rejected: -32600, even when naming another project.
    let resp = call(
        &mut server,
        "initialize",
        json!({"projectRoot": other.path().to_str().unwrap()}),
    );
    assert_eq!(resp["error"]["code"], -32600, "{resp}");

    // session_unaffected: same project, graph and registries.
    let state = server.state();
    assert_eq!(state.project_root().map(std::path::Path::to_path_buf), root);
    assert_eq!(
        specforge_mcp::registry::listed_tools(state, latest()).count(),
        tools
    );
    assert_eq!(state.graph().node_count(), 2);
    let stats = tool(&mut server, "specforge.stats", json!({}));
    assert_eq!(stats["edge_count"], 1);

    // error_handled_emitted.
    let handled = events(&server, "mcp_protocol_error_handled");
    assert!(
        handled
            .iter()
            .any(|p| p["requestMethod"] == "initialize" && p["errorCode"] == -32600),
        "{handled:?}"
    );
}

#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "Handle MCP Protocol Error: MCP protocol error handling holds — mcp_protocol_available, standard_error_returned, no_state_leaked, server_operational, error_handled_emitted"
)]
fn contract_protocol_error() {
    let mut server = test_server();
    let root = server
        .state()
        .project_root()
        .map(std::path::Path::to_path_buf)
        .unwrap()
        .display()
        .to_string();

    // standard_error_returned: the JSON-RPC code, with a readable message.
    let unknown = call(&mut server, "nonexistent/method", json!({}));
    assert_eq!(unknown["jsonrpc"], "2.0");
    assert_eq!(unknown["id"], 1);
    assert_eq!(unknown["error"]["code"], -32601);
    assert_eq!(
        unknown["error"]["message"],
        "Method not found: nonexistent/method"
    );

    let parse = server.handle_message("{not json").unwrap();
    let parse: Value = serde_json::from_str(&parse).unwrap();
    assert_eq!(parse["error"]["code"], -32700);
    assert_eq!(parse["error"]["message"], "Parse error");

    let missing = call(&mut server, "tools/call", json!({}));
    assert_eq!(missing["error"]["code"], -32602);

    // no_state_leaked: no paths, panics or addresses in the messages.
    for resp in [&unknown, &parse, &missing] {
        let message = resp["error"]["message"].as_str().unwrap();
        assert!(!message.is_empty());
        for leak in [root.as_str(), "panicked", "0x", ".rs:", "src/"] {
            assert!(!message.contains(leak), "{message:?} leaks {leak:?}");
        }
    }

    // server_operational: requests after the errors still succeed.
    let listed = call(&mut server, "tools/list", json!({}));
    assert!(!listed["result"]["tools"].as_array().unwrap().is_empty());
    assert_eq!(server.state().graph().node_count(), 2);

    // error_handled_emitted, once per error with its code.
    let codes: Vec<i64> = events(&server, "mcp_protocol_error_handled")
        .iter()
        .map(|p| p["errorCode"].as_i64().unwrap())
        .collect();
    assert_eq!(codes, [-32601, -32700, -32602]);
}

#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "Provide MCP Validate Tool: MCP validate tool holds — compiler_api_available, diagnostics_returned, strict_promotion_enforced, tool_invoked_emitted, verdict_in_meta"
)]
fn contract_validate() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = json!({
        "name": "v",
        "version": "0.1.0",
        "extensions": ["@specforge/software", "@specforge/testing"]
    });
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    std::fs::write(
        dir.path().join("clean.spec"),
        "behavior greet \"Greet\" {\n  category command\n  contract \"MUST greet\"\n  verify unit \"greets\"\n}\n",
    )
    .unwrap();
    let mut server = logged(McpServer::new());
    call(
        &mut server,
        "initialize",
        json!({"projectRoot": dir.path().to_str().unwrap()}),
    );

    // compiler_api_available: a file written after initialize is compiled
    // by the call itself.
    std::fs::write(
        dir.path().join("broken.spec"),
        "behavior wave \"Wave\" {\n  category command\n}\n",
    )
    .unwrap();

    // diagnostics_returned: every diagnostic, with severity, message, file
    // and line (the missing contract is an error, the missing verify a
    // warning).
    let resp = call_tool(&mut server, "specforge.validate", json!({}));
    let mut found = tool_json(&resp);
    found
        .as_array_mut()
        .unwrap()
        .sort_by_key(|d| d["code"].to_string());
    assert_eq!(
        found,
        json!([
            {
                "code": "E006",
                "title": "Missing required field",
                "severity": "Error",
                "message": "behavior 'wave' is missing required field 'contract'",
                "span": {"file": "broken.spec", "start_line": 1, "start_col": 1, "end_line": 3, "end_col": 2},
                "suggestion": null,
                "file": "broken.spec",
                "line": 1,
                "column": 1,
            },
            {
                "code": "W004",
                "title": "Untested testable entity",
                "severity": "Warning",
                "message": "behavior 'wave' is testable but declares no verify obligations",
                "span": {"file": "broken.spec", "start_line": 1, "start_col": 1, "end_line": 3, "end_col": 2},
                "suggestion": null,
                "file": "broken.spec",
                "line": 1,
                "column": 1,
                "origin": "@specforge/testing",
            },
        ]),
        "{resp}"
    );
    // Finding errors is what a validation run is for: a successful call.
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    // verdict_in_meta: whether the check passed, over everything reported.
    assert_eq!(
        resp["result"]["_meta"]["specforge/check"],
        json!({"ok": false, "errors": 1, "warnings": 1, "infos": 0, "shown": 2}),
        "{resp}"
    );

    // strict_promotion_enforced: the warning comes back as an error.
    let strict = tool(&mut server, "specforge.validate", json!({"strict": true}));
    let severities: Vec<(&str, &str)> = strict
        .as_array()
        .unwrap()
        .iter()
        .map(|d| (d["code"].as_str().unwrap(), d["severity"].as_str().unwrap()))
        .collect();
    assert_eq!(severities.len(), 2, "{strict}");
    assert!(severities.contains(&("W004", "Error")), "{strict}");
    assert!(severities.contains(&("E006", "Error")), "{strict}");
    let strict = call_tool(
        &mut server,
        "specforge.validate",
        json!({"strict": true, "severity_filter": "warning"}),
    );
    assert_eq!(
        strict["result"]["_meta"]["specforge/check"],
        json!({"ok": false, "errors": 2, "warnings": 0, "infos": 0, "shown": 0}),
        "{strict}"
    );

    assert_tool_invoked(&server, "specforge.validate");
}

#[specforge_test(
    behavior = "provide_mcp_suggest_fixes_tool",
    verify = "Provide MCP Suggest Fixes Tool: MCP suggest fixes tool holds — graph_available, fixes_returned, empty_for_clean, tool_invoked_emitted"
)]
fn contract_suggest_fixes() {
    // graph_available: a project compiled from disk, logout naming
    // `sesion_limit`, which no entity declares.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"c","extensions":["@specforge/software"]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("a.spec"),
        "invariant session_limit \"L\" {\n  guarantee \"g\"\n}\n\
         behavior logout \"Logout\" {\n  invariants [sesion_limit]\n}\n",
    )
    .unwrap();
    let mut server = logged(McpServer::new());
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"projectRoot": dir.path().to_str().unwrap()}});
    server.handle_message(&init.to_string());

    // fixes_returned: each fix with its title, kind, diagnostic and edits.
    let fixes = tool(
        &mut server,
        "specforge.suggest_fixes",
        json!({"diagnostic_code": "E003"}),
    )["fixes"]
        .clone();
    let replace = fixes
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["title"] == "Replace with 'session_limit'")
        .unwrap_or_else(|| panic!("no replacement in {fixes}"));
    assert_eq!(replace["kind"], "quickfix");
    assert_eq!(replace["diagnostic_code"], "E003");
    assert_eq!(
        replace["edits"],
        json!([{
            "file_path": "a.spec",
            "range": {"file": "a.spec", "start_line": 5, "start_col": 15, "end_line": 5, "end_col": 27},
            "new_text": "session_limit",
        }])
    );
    let for_logout = tool(
        &mut server,
        "specforge.suggest_fixes",
        json!({"entity_id": "logout"}),
    )["fixes"]
        .clone();
    assert_eq!(for_logout, fixes);

    // empty_for_clean: session_limit has no diagnostics.
    let for_limit = tool(
        &mut server,
        "specforge.suggest_fixes",
        json!({"entity_id": "session_limit"}),
    )["fixes"]
        .clone();
    assert_eq!(for_limit, json!([]));

    assert_tool_invoked(&server, "specforge.suggest_fixes");
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "Provide MCP Format Tool: MCP format tool holds — filesystem_available, files_formatted, check_mode_readonly, mutation_completed_emitted, tool_invoked_emitted"
)]
fn contract_format() {
    let mut server = test_server();
    let root = server
        .state()
        .project_root()
        .map(std::path::Path::to_path_buf)
        .unwrap();
    let messy = root.join("messy.spec");
    let source = "behavior   gamma \"Gamma\"{\ncontract \"x\"\n}\n";
    std::fs::write(&messy, source).unwrap();
    let only_messy = json!(["messy.spec"]);

    // check_mode_readonly: reported, not written, no mutation event.
    let check = tool(
        &mut server,
        "specforge.format",
        json!({"check": true, "paths": only_messy}),
    );
    assert_eq!(check["all_clean"], false, "{check}");
    assert_eq!(check["changed_files"].as_array().unwrap().len(), 1);
    assert!(
        check["changed_files"][0]
            .as_str()
            .unwrap()
            .ends_with("messy.spec")
    );
    assert_eq!(std::fs::read_to_string(&messy).unwrap(), source);
    assert!(events(&server, "mcp_mutation_completed").is_empty());

    // files_formatted: written in the canonical style.
    let write = tool(
        &mut server,
        "specforge.format",
        json!({"paths": only_messy}),
    );
    assert_eq!(write["check_only"], false, "{write}");
    assert_eq!(
        std::fs::read_to_string(&messy).unwrap(),
        "behavior gamma \"Gamma\" {\n  contract \"x\"\n}\n"
    );
    let recheck = tool(
        &mut server,
        "specforge.format",
        json!({"check": true, "paths": only_messy}),
    );
    assert_eq!(recheck["all_clean"], true, "{recheck}");

    // mutation_completed_emitted: once, for the write.
    // It rewrote the one messy file.
    assert_eq!(
        events(&server, "mcp_mutation_completed"),
        [json!({
            "toolName": "specforge.format",
            "files_changed": 1,
            "entities_affected": 0,
            "success": true,
        })]
    );
    assert_tool_invoked(&server, "specforge.format");
}

#[specforge_test(
    behavior = "provide_mcp_extensions_tool",
    verify = "Provide MCP Extensions Tool: MCP extensions tool holds — compiler_api_available, extensions_listed, config_reflected, tool_invoked_emitted"
)]
fn contract_extensions() {
    let dir = project_dir(
        json!({"name":"t","version":"0.1.0","extensions":["@specforge/product", "@acme/absent@1.2.0"]}),
        "",
    );
    let mut server = logged(McpServer::new());
    call(
        &mut server,
        "initialize",
        json!({"projectRoot": dir.path().to_str().unwrap()}),
    );

    // extensions_listed: name, version, entity kinds and status.
    let listed = tool(&mut server, "specforge.extensions", json!({}));
    let product = find(&listed["extensions"], "name", "@specforge/product");
    assert_eq!(product["status"], "loaded");
    assert!(
        product["version"].as_str().is_some_and(|v| !v.is_empty()),
        "{product}"
    );
    let kinds = product["entity_kinds"].as_array().unwrap();
    assert!(kinds.contains(&json!("journey")), "{kinds:?}");
    // config_reflected: a configured extension that did not load is listed.
    let absent = find(&listed["extensions"], "name", "@acme/absent");
    assert_eq!(absent["status"], "not_loaded");
    assert_eq!(absent["version"], "1.2.0");
    assert_eq!(absent["entity_kinds"], json!([]));

    // ...and follows specforge.json as it changes.
    std::fs::write(
        dir.path().join("specforge.json"),
        json!({"name":"t","version":"0.1.0","extensions":["@specforge/product"]}).to_string(),
    )
    .unwrap();
    let listed = tool(&mut server, "specforge.extensions", json!({}));
    let names: Vec<&str> = listed["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["@specforge/product"]);

    assert_tool_invoked(&server, "specforge.extensions");
}

#[specforge_test(
    behavior = "provide_mcp_doctor_tool",
    verify = "Provide MCP Doctor Tool: MCP doctor tool holds — compiler_api_available, health_checked, resolution_steps_provided, tool_invoked_emitted"
)]
fn contract_doctor() {
    // A project that enables nothing, served with its own component
    // runtime, so the extension it installs loads.
    let mut server = TestProject::new().serve_components();
    let dir = server.root().to_path_buf();
    let installed = tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [product_wasm(&dir).to_str().unwrap()]}),
    );
    assert_eq!(installed["added"][0]["action"], "installed", "{installed}");

    // health_checked: a fresh install is healthy.
    let healthy = tool(&mut server, "specforge.doctor", json!({}));
    assert_eq!(healthy["extensions_ok"], true, "{healthy}");
    assert_eq!(healthy["cache_status"], "ok");

    // Tamper with the installed binary: a stale Wasm cache entry.
    let wasm = walk(&dir.join(".specforge").join("extensions"))
        .into_iter()
        .find(|p| p.extension().is_some_and(|e| e == "wasm"))
        .expect("the install wrote a wasm binary");
    std::fs::write(&wasm, b"not the installed module").unwrap();

    let report = tool(&mut server, "specforge.doctor", json!({}));
    assert_eq!(report["extensions_ok"], false, "{report}");
    assert_eq!(report["cache_status"], "stale");
    // resolution_steps_provided: a deterministic step for the issue. A
    // local install reinstalls from the path it was installed from (the
    // project's own, so relative to it).
    let finding = find(&report["findings"], "code", "stale_hash");
    assert_eq!(finding["status"], "error");
    let local = product_wasm(&dir);
    let local = local.strip_prefix(&dir).unwrap().display();
    assert_eq!(
        finding["remediation"],
        format!("run `specforge add {local}` to reinstall it")
    );
    assert_eq!(tool(&mut server, "specforge.doctor", json!({})), report);

    assert_tool_invoked(&server, "specforge.doctor");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[specforge_test(
    behavior = "expose_graph_as_mcp_resource",
    verify = "Expose Graph as MCP Resource: graph MCP resource holds — validation_complete_fired, graph_json_returned, resource_read_emitted"
)]
fn contract_graph_resource() {
    let mut server = test_server();
    let (content, graph) = resource(&mut server, "specforge://graph");
    assert_eq!(content["uri"], "specforge://graph");
    assert_eq!(content["mimeType"], "application/json");

    // graph_json_returned: the whole graph, schema embedded and versioned.
    assert_eq!(node_ids(&graph), ["alpha", "beta"]);
    assert_eq!(
        graph["edges"],
        json!([{"source": "beta", "target": "alpha", "label": "behaviors"}])
    );
    assert!(graph["schema"].is_object(), "{graph}");
    assert!(graph["schema"]["entity_kinds"].is_array(), "{graph}");
    assert!(graph["schema_version"].is_string(), "{graph}");
    // The same nodes specforge export --format=graph produces.
    let export = tool(&mut server, "specforge.export", json!({"format": "graph"}));
    assert_eq!(graph["nodes"], export["nodes"]);

    assert_resource_read(&server, "specforge://graph");
}

#[specforge_test(
    behavior = "expose_schema_as_mcp_resource",
    verify = "Expose Schema as MCP Resource: schema MCP resource holds — validation_complete_fired, schema_json_returned, resource_read_emitted"
)]
fn contract_schema_resource() {
    // validation_complete_fired: initialize compiled the project.
    let project = project_dir(
        json!({"name": "t", "version": "0.1.0", "extensions": ["@specforge/software"]}),
        "behavior act \"Act\" {\n  contract \"MUST act\"\n}\n",
    );
    let mut server = logged(McpServer::new());
    call(
        &mut server,
        "initialize",
        json!({"projectRoot": project.path().to_str().unwrap()}),
    );

    // schema_json_returned: the GraphProtocolSchema, the same document
    // specforge.schema returns unfiltered.
    let (content, schema) = resource(&mut server, "specforge://schema");
    assert_eq!(content["uri"], "specforge://schema");
    assert_eq!(content["mimeType"], "application/json");
    assert_eq!(schema, tool(&mut server, "specforge.schema", json!({})));
    assert_eq!(schema["extensions"][0]["name"], "@specforge/software");

    assert_resource_read(&server, "specforge://schema");
}

#[specforge_test(
    behavior = "expose_context_as_mcp_resource",
    verify = "Expose Context as MCP Resource: context MCP resource holds — validation_complete_fired, context_format_returned, resource_read_emitted"
)]
fn contract_context_resource() {
    let mut server = test_server();
    let (content, context) = resource(&mut server, "specforge://context");
    assert_eq!(content["uri"], "specforge://context");
    let alpha = find(&context["nodes"], "id", "alpha");
    assert_eq!(alpha["contract"], "MUST work");
    assert_eq!(
        alpha["verify"],
        json!([{"kind": "unit", "description": "works"}])
    );
    assert_eq!(context["edges"].as_array().unwrap().len(), 1);
    // context_format_returned: what export --format=context returns.
    let export = tool(
        &mut server,
        "specforge.export",
        json!({"format": "context"}),
    );
    assert_eq!(context, export);

    assert_resource_read(&server, "specforge://context");
}

#[specforge_test(
    behavior = "expose_brief_as_mcp_resource",
    verify = "Expose Brief as MCP Resource: brief MCP resource holds — validation_complete_fired, brief_format_returned, resource_read_emitted"
)]
fn contract_brief_resource() {
    let mut server = test_server();
    let (content, brief) = resource(&mut server, "specforge://brief");
    assert_eq!(content["uri"], "specforge://brief");
    // brief_format_returned: ids, kinds, titles and edges only.
    assert_eq!(
        brief["nodes"],
        json!([
            {"id": "alpha", "kind": "behavior", "title": "Alpha"},
            {"id": "beta", "kind": "feature", "title": "Beta"},
        ])
    );
    assert_eq!(
        brief["edges"],
        json!([{"source": "beta", "target": "alpha", "label": "behaviors"}])
    );
    let export = tool(&mut server, "specforge.export", json!({"format": "brief"}));
    assert_eq!(brief, export);

    assert_resource_read(&server, "specforge://brief");
}

#[specforge_test(
    behavior = "expose_diagnostics_as_mcp_resource",
    verify = "Expose Diagnostics as MCP Resource: diagnostics MCP resource holds — validation_complete_fired, diagnostics_returned, resource_read_emitted"
)]
fn contract_diagnostics_resource() {
    // broken.spec: `broken` names `ghost`, which no entity declares (E003
    // at 2:16–2:21); `lonely` (lines 4–5), a behavior nothing references,
    // is a warning by a rule the extension declares (W901, in its range).
    let mut server = contracts_project()
        .file(
            "broken.spec",
            "feature broken \"Broken\" {\n    behaviors [ghost]\n}\nbehavior lonely \"Lonely\" {\n}\n",
        )
        .serve(&[extension().declaring(|c| {
            c.rule("W901", |r| {
                r.check(CheckKind::NoIncomingEdges)
                    .target_kind("behavior")
                    .severity(ValidationSeverity::Warning)
                    .message_template("a warning");
            });
        })]);

    // diagnostics_returned: severity, code, message, file and position.
    let (content, bag) = resource(&mut server, "specforge://diagnostics");
    assert_eq!(content["uri"], "specforge://diagnostics");
    assert_eq!(
        bag,
        json!([
            {"code": "E003", "title": "Unresolved reference", "severity": "Error",
             "message": "unresolved reference 'ghost' in entity 'broken'",
             "span": {"file": "broken.spec", "start_line": 2, "start_col": 16, "end_line": 2, "end_col": 21},
             "suggestion": null,
             "data": {"kind": "unresolved_reference", "target": "ghost", "entity": "broken", "field": "behaviors"},
             "file": "broken.spec", "line": 2, "column": 16},
            {"code": "W901", "title": null, "origin": "@test/ext", "severity": "Warning", "message": "a warning",
             "span": {"file": "broken.spec", "start_line": 4, "start_col": 1, "end_line": 5, "end_col": 2},
             "suggestion": null,
             "file": "broken.spec", "line": 4, "column": 1},
        ])
    );

    // Updates with the compilation's diagnostics: the file removed, the
    // next read compiles a project that reports nothing.
    server.remove("broken.spec");
    let (_, bag) = resource(&mut server, "specforge://diagnostics");
    assert_eq!(bag, json!([]));

    assert_resource_read(&server, "specforge://diagnostics");
}

#[specforge_test(
    behavior = "expose_entity_as_mcp_resource",
    verify = "Expose Per-Entity MCP Resource: per-entity MCP resource holds — validation_complete_fired, subgraph_returned, resource_read_emitted"
)]
fn contract_entity_resource() {
    // gamma (inv.spec) hangs off beta: two hops from alpha.
    let mut server = contracts_project()
        .file("feat.spec", BETA_WITH_GAMMA)
        .file("inv.spec", "invariant gamma \"GAMMA\" {\n}\n")
        .serve(&[extension()]);

    // subgraph_returned: alpha, its direct neighbor, the edge between them.
    let (content, entity) = resource(&mut server, "specforge://graph/alpha");
    assert_eq!(content["uri"], "specforge://graph/alpha");
    assert_eq!(node_ids(&entity), ["alpha", "beta"]);
    assert_eq!(
        entity["edges"],
        json!([{"source": "beta", "target": "alpha", "label": "behaviors"}])
    );
    let (_, beta) = resource(&mut server, "specforge://graph/beta");
    assert_eq!(node_ids(&beta), ["alpha", "beta", "gamma"]);

    // An unknown entity is an error, not an empty subgraph.
    let missing = call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://graph/ghost"}),
    );
    assert!(missing["error"].is_object(), "{missing}");

    assert_resource_read(&server, "specforge://graph/alpha");
}

/// Subscribe the default client to `uri`'s delta notifications.
fn subscribe(server: &mut McpServer, uri: &str) {
    let resp = call(server, "resources/subscribe", json!({"uri": uri}));
    assert_eq!(resp["result"], json!({}), "{resp}");
}

/// Unsubscribe the default client from `uri`'s delta notifications.
fn unsubscribe(server: &mut McpServer, uri: &str) {
    let resp = call(server, "resources/unsubscribe", json!({"uri": uri}));
    assert_eq!(resp["result"], json!({}), "{resp}");
}

/// A server serving a project that enables no extension, alpha and beta
/// in its test.spec; and that spec file's path.
fn project_server() -> (Served, PathBuf) {
    let server = TestProject::new()
        .file(
            "test.spec",
            "behavior alpha \"Alpha\" {\n}\nfeature beta \"Beta\" {\n    behaviors [alpha]\n}\n",
        )
        .serve_components();
    let spec = server.root().join("test.spec");
    (server, spec)
}

/// Rebuild the project the way a client does: `specforge.validate` brings
/// it up to date with disk, and the delta notifications follow.
fn rebuild(server: &mut McpServer) {
    let resp = call_tool(server, "specforge.validate", json!({}));
    assert!(resp["error"].is_null(), "{resp}");
}

/// `notifications/resources/updated` for `uri`, as a handshake subscriber
/// hears it.
fn updated(uri: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/resources/updated",
        "params": {"uri": uri},
    })
}

#[specforge_test(
    behavior = "notify_graph_delta_via_mcp",
    verify = "Notify Graph Delta via MCP: graph delta MCP notification holds — graph_delta_computed_fired, subscribers_notified, no_notification_when_empty, delta_notified_emitted"
)]
fn contract_graph_notification() {
    let (mut server, spec) = project_server();

    // no_notification_when_empty: the first build adds alpha and beta, but
    // nobody is subscribed.
    rebuild(&mut server);
    assert_eq!(
        node_ids(&tool(&mut server, "specforge.export", json!({}))),
        ["alpha", "beta"]
    );
    assert!(server.take_notifications().is_empty());

    // graph_delta_computed_fired + subscribers_notified: after a rebuild
    // that drops beta (and its edge) and adds gamma, the subscriber gets
    // specforge/graphChanged with the delta.
    subscribe(&mut server, "specforge://graph");
    std::fs::write(
        &spec,
        "behavior alpha \"Alpha\" {\n}\nbehavior gamma \"Gamma\" {\n}\n",
    )
    .unwrap();
    rebuild(&mut server);
    assert_eq!(
        server.take_notifications(),
        [
            updated("specforge://graph"),
            json!({
                "jsonrpc": "2.0",
                "method": "specforge/graphChanged",
                "params": {
                    "added_nodes": [{"id": "gamma", "kind": "behavior", "file": "test.spec", "line": 3}],
                    "removed_nodes": [{"id": "beta", "kind": "feature", "file": "test.spec", "line": 3}],
                    "modified_nodes": [],
                    "added_edges": [],
                    "removed_edges": [{"source": "beta", "target": "alpha", "label": "behaviors"}],
                    "affected_files": ["test.spec"],
                },
            }),
        ]
    );

    // subscribers_notified: a rebuild that only changes gamma's fields
    // reports gamma as modified.
    std::fs::write(
        &spec,
        "behavior alpha \"Alpha\" {\n}\nbehavior gamma \"Gamma\" {\n  contract \"now defined\"\n}\n",
    )
    .unwrap();
    rebuild(&mut server);
    assert_eq!(
        server.take_notifications(),
        [
            updated("specforge://graph"),
            json!({
                "jsonrpc": "2.0",
                "method": "specforge/graphChanged",
                "params": {
                    "added_nodes": [],
                    "removed_nodes": [],
                    "modified_nodes": [{
                        "id": "gamma",
                        "changed_fields": ["contract"],
                        "changed_aspects": [],
                        "file": "test.spec",
                        "line": 3,
                    }],
                    "added_edges": [],
                    "removed_edges": [],
                    "affected_files": ["test.spec"],
                },
            }),
        ]
    );

    // delta_notified_emitted: once per delivered delta.
    assert_eq!(
        events(&server, "mcp_delta_notified"),
        [
            json!({"notificationType": "graph", "subscriberCount": 1,
                "addedNodes": 1, "removedNodes": 1, "modifiedNodes": 0}),
            json!({"notificationType": "graph", "subscriberCount": 1,
                "addedNodes": 0, "removedNodes": 0, "modifiedNodes": 1}),
        ]
    );

    // An unchanged rebuild sends nothing.
    rebuild(&mut server);
    assert!(server.take_notifications().is_empty());

    // no_notification_when_empty: once the client unsubscribes, a changed
    // graph is not announced.
    unsubscribe(&mut server, "specforge://graph");
    std::fs::write(&spec, "behavior alpha \"Alpha\" {\n}\n").unwrap();
    rebuild(&mut server);
    assert!(server.take_notifications().is_empty());
    assert_eq!(events(&server, "mcp_delta_notified").len(), 2);
}

/// The server over the [`project_server`] project, built once, with the
/// default client subscribed to graph deltas.
fn subscribed_graph_server() -> (Served, PathBuf) {
    let (mut server, spec) = project_server();
    rebuild(&mut server);
    subscribe(&mut server, "specforge://graph");
    assert!(server.take_notifications().is_empty());
    (server, spec)
}

/// The IDs of the `modified_nodes` of the one graph notification a rebuild
/// sends.
fn modified_after_rebuild(server: &mut McpServer) -> Value {
    rebuild(server);
    let sent = server.take_notifications();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!(sent[1]["method"], "specforge/graphChanged");
    sent[1]["params"]["modified_nodes"]
        .as_array()
        .expect("modified nodes")
        .iter()
        .map(|node| node["id"].clone())
        .collect()
}

#[specforge_test(
    behavior = "notify_graph_delta_via_mcp",
    verify = "a field-only edit is reported as a modified node"
)]
fn field_only_edits_are_modified_nodes() {
    let (mut server, spec) = subscribed_graph_server();

    // A new field on alpha.
    std::fs::write(
        &spec,
        "behavior alpha \"Alpha\" {\n  contract \"first\"\n}\nfeature beta \"Beta\" {\n    behaviors [alpha]\n}\n",
    )
    .unwrap();
    assert_eq!(modified_after_rebuild(&mut server), json!(["alpha"]));

    // A changed field value.
    std::fs::write(
        &spec,
        "behavior alpha \"Alpha\" {\n  contract \"second\"\n}\nfeature beta \"Beta\" {\n    behaviors [alpha]\n}\n",
    )
    .unwrap();
    assert_eq!(modified_after_rebuild(&mut server), json!(["alpha"]));

    // A new verify line.
    std::fs::write(
        &spec,
        "behavior alpha \"Alpha\" {\n  contract \"second\"\n  verify unit \"it works\"\n}\nfeature beta \"Beta\" {\n    behaviors [alpha]\n}\n",
    )
    .unwrap();
    assert_eq!(modified_after_rebuild(&mut server), json!(["alpha"]));

    // Two titles at once: the list is sorted.
    std::fs::write(
        &spec,
        "behavior alpha \"Alpha 2\" {\n  contract \"second\"\n  verify unit \"it works\"\n}\nfeature beta \"Beta 2\" {\n    behaviors [alpha]\n}\n",
    )
    .unwrap();
    assert_eq!(
        modified_after_rebuild(&mut server),
        json!(["alpha", "beta"])
    );

    assert_eq!(
        events(&server, "mcp_delta_notified")
            .iter()
            .map(|e| e["modifiedNodes"].clone())
            .collect::<Vec<_>>(),
        [json!(1), json!(1), json!(1), json!(2)]
    );
}

#[specforge_test(
    behavior = "notify_graph_delta_via_mcp",
    verify = "moving an entity is not a modification: no graphChanged is sent"
)]
fn moving_an_entity_is_not_a_modification() {
    let (mut server, spec) = subscribed_graph_server();
    let moved_graph = vec![updated("specforge://graph")];

    // Blank lines above alpha shift every span.
    std::fs::write(
        &spec,
        "\n\n\nbehavior alpha \"Alpha\" {\n}\nfeature beta \"Beta\" {\n    behaviors [alpha]\n}\n",
    )
    .unwrap();
    rebuild(&mut server);
    assert_eq!(server.take_notifications(), moved_graph);

    // Swapping the two entities moves both.
    std::fs::write(
        &spec,
        "feature beta \"Beta\" {\n    behaviors [alpha]\n}\n\nbehavior alpha \"Alpha\" {\n}\n",
    )
    .unwrap();
    rebuild(&mut server);
    assert_eq!(server.take_notifications(), moved_graph);

    // Moving alpha to another file.
    std::fs::write(
        spec.with_file_name("moved.spec"),
        "behavior alpha \"Alpha\" {\n}\n",
    )
    .unwrap();
    std::fs::write(&spec, "feature beta \"Beta\" {\n    behaviors [alpha]\n}\n").unwrap();
    rebuild(&mut server);
    assert_eq!(server.take_notifications(), moved_graph);
    assert!(events(&server, "mcp_delta_notified").is_empty());
}

#[specforge_test(
    behavior = "notify_graph_delta_via_mcp",
    verify = "a moved entity updates the resources that write its place, and no graphChanged is sent"
)]
fn a_moved_entity_updates_the_graph_documents() {
    let (mut server, spec) = project_server();
    rebuild(&mut server);
    subscribe(&mut server, "specforge://graph");
    subscribe(&mut server, "specforge://context");
    assert!(server.take_notifications().is_empty());

    // Three blank lines above alpha: the graph writes its line, the context does not.
    std::fs::write(
        &spec,
        "\n\n\nbehavior alpha \"Alpha\" {\n}\nfeature beta \"Beta\" {\n    behaviors [alpha]\n}\n",
    )
    .unwrap();
    rebuild(&mut server);
    assert_eq!(
        server.take_notifications(),
        vec![updated("specforge://graph")]
    );
    assert!(events(&server, "mcp_delta_notified").is_empty());

    // Alpha to another file.
    std::fs::write(
        spec.with_file_name("moved.spec"),
        "behavior alpha \"Alpha\" {\n}\n",
    )
    .unwrap();
    std::fs::write(&spec, "feature beta \"Beta\" {\n    behaviors [alpha]\n}\n").unwrap();
    rebuild(&mut server);
    assert_eq!(
        server.take_notifications(),
        vec![updated("specforge://graph")]
    );
    assert!(events(&server, "mcp_delta_notified").is_empty());
}

#[specforge_test(
    behavior = "notify_diagnostics_delta_via_mcp",
    verify = "Notify Diagnostics Delta via MCP: diagnostics delta MCP notification holds — validation_complete_fired, subscribers_notified, unchanged_suppressed, delta_notified_emitted"
)]
fn contract_diagnostics_notification() {
    let (mut server, spec) = project_server();
    rebuild(&mut server);
    let clean = server.state().diagnostics().clone();
    subscribe(&mut server, "specforge://diagnostics");

    // validation_complete_fired + subscribers_notified: a rebuild whose
    // validation finds a duplicate entity sends the new diagnostic.
    std::fs::write(
        &spec,
        "behavior alpha \"Alpha\" {\n}\nbehavior alpha \"Again\" {\n}\n",
    )
    .unwrap();
    rebuild(&mut server);
    // Whole, as every surface presents a diagnostic (`specforge check
    // --format json`), so the client can locate it.
    let duplicate = json!({
        "code": "E002",
        "title": "Duplicate entity ID",
        "severity": "Error",
        "message": "duplicate entity ID 'alpha' (first declared at test.spec:1:1)",
        "span": {
            "file": "test.spec",
            "start_line": 3,
            "start_col": 1,
            "end_line": 4,
            "end_col": 2,
        },
        "suggestion": "rename one of the entities to avoid the collision",
        "file": "test.spec",
        "line": 3,
        "column": 1,
    });
    let changed = |added: Value, removed: Value| {
        json!({
            "jsonrpc": "2.0",
            "method": "specforge/diagnosticsChanged",
            "params": {"added": added, "removed": removed},
        })
    };
    assert_eq!(
        server.take_notifications(),
        [
            updated("specforge://diagnostics"),
            changed(json!([duplicate]), json!([])),
        ]
    );

    // unchanged_suppressed: rebuilding the same project sends nothing.
    rebuild(&mut server);
    assert!(server.take_notifications().is_empty());

    // subscribers_notified: fixing it sends the removal.
    std::fs::write(&spec, "behavior alpha \"Alpha\" {\n}\n").unwrap();
    rebuild(&mut server);
    assert_eq!(server.state().diagnostics(), clean);
    assert_eq!(
        server.take_notifications(),
        [
            updated("specforge://diagnostics"),
            changed(json!([]), json!([duplicate])),
        ]
    );

    // delta_notified_emitted: once per delivered delta.
    assert_eq!(
        events(&server, "mcp_delta_notified"),
        [
            json!({"notificationType": "diagnostics", "subscriberCount": 1,
                "addedDiagnostics": 1, "removedDiagnostics": 0}),
            json!({"notificationType": "diagnostics", "subscriberCount": 1,
                "addedDiagnostics": 0, "removedDiagnostics": 1}),
        ]
    );

    // unchanged_suppressed: with no subscriber, a change sends nothing.
    unsubscribe(&mut server, "specforge://diagnostics");
    std::fs::write(
        &spec,
        "behavior alpha \"Alpha\" {\n}\nbehavior alpha \"Again\" {\n}\n",
    )
    .unwrap();
    rebuild(&mut server);
    assert!(server.take_notifications().is_empty());
    assert_eq!(events(&server, "mcp_delta_notified").len(), 2);
}

/// Serve `before` as test.spec, subscribe to the diagnostics, rewrite the
/// file to `after` and rebuild: the `(removed, added)` of the one
/// `specforge/diagnosticsChanged` the subscriber hears, after the
/// `resources/updated` that precedes it.
fn diagnostics_heard_after(before: &str, after: &str) -> (Vec<Value>, Vec<Value>) {
    let (mut server, spec) = project_server();
    std::fs::write(&spec, before).unwrap();
    rebuild(&mut server);
    subscribe(&mut server, "specforge://diagnostics");
    server.take_notifications();
    std::fs::write(&spec, after).unwrap();
    rebuild(&mut server);
    let notifications = server.take_notifications();
    assert_eq!(notifications.len(), 2, "{notifications:?}");
    assert_eq!(notifications[0], updated("specforge://diagnostics"));
    assert_eq!(notifications[1]["method"], "specforge/diagnosticsChanged");
    let side = |name: &str| notifications[1]["params"][name].as_array().unwrap().clone();
    (side("removed"), side("added"))
}

/// `(code, line)` of each diagnostic.
fn located(diagnostics: &[Value]) -> Vec<(String, i64)> {
    diagnostics
        .iter()
        .map(|d| {
            (
                d["code"].as_str().unwrap().to_string(),
                d["line"].as_i64().unwrap(),
            )
        })
        .collect()
}

#[specforge_test(
    behavior = "notify_diagnostics_delta_via_mcp",
    verify = "a diagnostic that moved is removed where it was and added where it is"
)]
fn a_moved_diagnostic_is_notified() {
    let text = "behavior alpha \"Alpha\" {\n}\nbogus thing\n";
    let (removed, added) = diagnostics_heard_after(text, &format!("\n\n\n{text}"));
    assert_eq!(located(&removed), [("E001".to_string(), 3)]);
    assert_eq!(located(&added), [("E001".to_string(), 6)]);
}

#[specforge_test(
    behavior = "notify_diagnostics_delta_via_mcp",
    verify = "a diagnostic whose span changed is removed and added"
)]
fn a_diagnostic_whose_span_changed_is_notified() {
    let alpha = "behavior alpha \"Alpha\" {\n}\n";
    let (removed, added) = diagnostics_heard_after(
        &format!("{alpha}bogus thing\n\nbogus thing\n"),
        &format!("{alpha}bogus thing\n"),
    );
    assert_eq!(located(&removed), [("E001".to_string(), 3)]);
    assert_eq!(located(&added), [("E001".to_string(), 3)]);
    assert_eq!(removed[0]["span"]["end_line"], 5);
    assert_eq!(added[0]["span"]["end_line"], 3);
}

#[specforge_test(
    behavior = "notify_diagnostics_delta_via_mcp",
    verify = "removing one of two identical diagnostics is notified"
)]
fn removing_one_of_two_identical_diagnostics_is_notified() {
    let two = "behavior alpha \"Alpha\" {\n}\nbehavior alpha \"Again\" {\n}\n";
    let (removed, added) =
        diagnostics_heard_after(&format!("{two}behavior alpha \"Third\" {{\n}}\n"), two);
    assert_eq!(located(&removed), [("E002".to_string(), 5)]);
    assert!(added.is_empty(), "{added:?}");
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "Provide MCP Add Extension Tool: MCP add extension tool holds — filesystem_available, extension_installed, wasm_downloaded, extension_added_emitted, dry_run_safe, tool_invoked_emitted"
)]
fn contract_add_extension() {
    // A project that enables nothing, served with its own component
    // runtime, so an installed extension loads.
    let mut server = TestProject::new().serve_components();
    let dir = server.root().to_path_buf();
    assert!(dir.join("specforge.json").is_file());
    // Real offline install of the vendored product blob.
    let blob = product_wasm(&dir);
    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [blob.to_str().unwrap()]}),
    );
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(text).unwrap();
    assert_eq!(parsed["added"][0]["action"], "installed");
    // Truthful install is observable on disk.
    let lock = std::fs::read_to_string(dir.as_path().join("specforge.lock")).unwrap();
    assert!(lock.contains("@sdk/greet"));
}

#[test]
fn contract_remove_extension() {
    // A project that enables nothing, served with its own component
    // runtime, so an installed extension loads.
    let mut server = TestProject::new().serve_components();
    let dir = server.root().to_path_buf();
    assert!(dir.join("specforge.json").is_file());
    // Removing something that is not installed must refuse.
    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"name": "@specforge/software"}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "extension_not_found", "{error}");
}

// NOT LINKED to "Provide MCP Migrate Tool: MCP migrate tool holds — …":
// there is only one format version (1.0), so no migration can be applied
// (migrations_applied), and the tool runs no post-migration validation
// (post_migration_validated). What holds today: a current project is left
// alone.
#[test]
fn contract_migrate() {
    let mut server = test_server();
    let root = server
        .state()
        .project_root()
        .map(std::path::Path::to_path_buf)
        .unwrap();
    let before = std::fs::read_to_string(root.join("test.spec")).unwrap();

    let result = tool(&mut server, "specforge.migrate", json!({"dry_run": false}));
    assert_eq!(result["migrated"], false, "{result}");
    assert_eq!(result["from_version"], result["to_version"]);
    assert_eq!(
        std::fs::read_to_string(root.join("test.spec")).unwrap(),
        before
    );
    assert_tool_invoked(&server, "specforge.migrate");
}

#[specforge_test(
    behavior = "provide_mcp_providers_tool",
    verify = "Provide MCP Providers Tool: MCP providers tool holds — compiler_api_available, providers_listed, tool_invoked_emitted"
)]
fn contract_providers() {
    // The project names two providers; only an extension that contributes
    // providers can back a scheme: `@acme/github-provider`, enabled after
    // @test/ext. `@acme/jira` is not enabled.
    let mut server = contracts_project()
        .config(|c| {
            c["providers"] = json!([
                {"alias": "github", "scheme": "gh", "extension": "@acme/github-provider"},
                {"alias": "tracker", "scheme": "jira", "extension": "@acme/jira"},
            ]);
        })
        .serve(&[
            extension(),
            TestExtension::named("@acme/github-provider").declaring(|c| {
                c.providers();
            }),
        ]);

    // providers_listed: scheme, alias, backing extension and status.
    let listed = tool(&mut server, "specforge.providers", json!({}));
    assert_eq!(listed["count"], 2);
    assert_eq!(
        find(&listed["providers"], "scheme", "gh"),
        &json!({
            "scheme": "gh", "alias": "github",
            "extension": "@acme/github-provider", "status": "registered",
        })
    );
    assert_eq!(
        find(&listed["providers"], "scheme", "jira"),
        &json!({
            "scheme": "jira", "alias": "tracker",
            "extension": "@acme/jira", "status": "extension_not_loaded",
        })
    );

    assert_tool_invoked(&server, "specforge.providers");
}

#[specforge_test(
    behavior = "provide_mcp_render_tool",
    verify = "Provide MCP Render Tool: MCP render tool holds — graph_available, filesystem_available, files_written, files_listed, tool_invoked_emitted"
)]
fn contract_render() {
    let mut server = test_server();
    let out_dir = server.root().join("rendered");

    // files_written / files_listed: the json renderer's file, in out_dir.
    let json_render = tool(
        &mut server,
        "specforge.render",
        json!({"format": "json", "out_dir": out_dir.to_str().unwrap()}),
    );
    let graph_file = out_dir.join("graph.json");
    assert_eq!(json_render["output_files"], json!(["rendered/graph.json"]));
    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(&graph_file).unwrap()).unwrap();
    assert_eq!(node_ids(&written), ["alpha", "beta"]);

    // The dot renderer writes its own file.
    let dot_render = tool(
        &mut server,
        "specforge.render",
        json!({"format": "dot", "out_dir": out_dir.to_str().unwrap()}),
    );
    let dot_file = out_dir.join("graph.dot");
    assert_eq!(dot_render["output_files"], json!(["rendered/graph.dot"]));
    let dot = std::fs::read_to_string(&dot_file).unwrap();
    assert!(dot.starts_with("digraph"), "{dot}");
    assert!(dot.contains("alpha") && dot.contains("beta"), "{dot}");

    // An unknown format lists the available renderers.
    let unknown = call_tool(&mut server, "specforge.render", json!({"format": "pdf"}));
    assert_eq!(
        crate::tool_errors::mcp_error(&unknown)["data"]["available_renderers"],
        json!(["graph", "context", "brief", "dot", "markdown"])
    );

    assert_tool_invoked(&server, "specforge.render");
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "Provide MCP Analyze Tool: MCP analyze tool holds — graph_available, passes_run, results_structured, tool_invoked_emitted"
)]
fn contract_analyze() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.analyze", json!({}));
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(text).unwrap();
    assert!(parsed["ok"].is_boolean(), "analyze must return ok flag");
    let passes = parsed["passes"].as_array().unwrap();
    assert!(!passes.is_empty(), "all-pass run must include passes");
    // Built-in passes always run; coverage is @specforge/testing's pass and
    // this fixture enables no extensions.
    assert!(passes.iter().any(|p| p["pass"] == "contracts"));
}

// ── specforge.model ─────────────────────────────────────────────────────────

/// A server serving a project with @specforge/software.
fn model_server() -> (Logged, tempfile::TempDir) {
    let project = project_dir(
        json!({"name": "t", "version": "0.1.0", "extensions": ["@specforge/software"]}),
        "behavior act \"Act\" {\n  contract \"MUST act\"\n}\n",
    );
    let mut server = logged(McpServer::new());
    call(
        &mut server,
        "initialize",
        json!({"projectRoot": project.path().to_str().unwrap()}),
    );
    (server, project)
}

/// `specforge.model`'s rendered text.
fn model_text(server: &mut McpServer, args: Value) -> String {
    let resp = call_tool(server, "specforge.model", args);
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text in {resp}"))
        .to_string()
}

/// The entity kinds a JSON model lists.
fn model_kinds(server: &mut McpServer, mut args: Value) -> Vec<String> {
    args["format"] = json!("json");
    let model: Value = serde_json::from_str(&model_text(server, args)).unwrap();
    model["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "expose_model_mcp_tool",
    verify = "specforge.model appears in MCP tool list"
)]
fn the_model_tool_is_listed_with_its_parameters() {
    let (mut server, _project) = model_server();
    let tools = call(&mut server, "tools/list", json!({}));
    let model = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "specforge.model")
        .unwrap_or_else(|| panic!("specforge.model is not listed: {tools}"));
    let properties = model["inputSchema"]["properties"].as_object().unwrap();
    for parameter in [
        "format",
        "group_by",
        "fields",
        "extension",
        "kinds",
        "root",
        "depth",
    ] {
        assert!(properties.contains_key(parameter), "{parameter}: {model}");
    }
}

#[specforge_test(
    behavior = "expose_model_mcp_tool",
    verify = "default format is markdown"
)]
fn the_model_defaults_to_markdown() {
    let (mut server, _project) = model_server();
    let default = model_text(&mut server, json!({}));
    assert!(default.starts_with("# Logical Data Model"), "{default}");
    assert_eq!(
        default,
        model_text(&mut server, json!({"format": "markdown"}))
    );
}

#[specforge_test(
    behavior = "expose_model_mcp_tool",
    verify = "all five formats produce valid output"
)]
fn every_model_format_renders() {
    let (mut server, _project) = model_server();
    let text = |server: &mut McpServer, format: &str| model_text(server, json!({"format": format}));
    assert!(text(&mut server, "mermaid").starts_with("erDiagram"));
    assert!(text(&mut server, "dot").starts_with("digraph model {"));
    assert!(
        text(&mut server, "dbml").starts_with("// Generated by specforge model"),
        "dbml"
    );
    let json: Value = serde_json::from_str(&text(&mut server, "json")).unwrap();
    assert_eq!(json["extensions"][0]["name"], "@specforge/software");
    assert!(
        text(&mut server, "markdown").contains("behavior"),
        "markdown names the kinds"
    );
    // Any other format is refused on the argument.
    let refused = call_tool(&mut server, "specforge.model", json!({"format": "svg"}));
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    let error = tool_json(&refused);
    assert_eq!(error["code"], "invalid_input");
    assert_eq!(error["argument"], "format");
}

#[specforge_test(
    behavior = "expose_model_mcp_tool",
    verify = "filter parameters are passed through to model options"
)]
fn model_filters_reach_the_model() {
    let (mut server, _project) = model_server();
    let all = model_kinds(&mut server, json!({}));
    assert!(all.len() > 1, "{all:?}");
    assert_eq!(
        model_kinds(&mut server, json!({"kinds": ["behavior"]})),
        ["behavior"]
    );
    // An extension the project does not load is refused.
    let refused = call_tool(
        &mut server,
        "specforge.model",
        json!({"extension": "@specforge/product"}),
    );
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    let error = tool_json(&refused);
    assert_eq!(error["code"], "extension_not_found");
    assert_eq!(error["argument"], "extension");
    assert!(
        error["data"]["suggestion"]
            .as_str()
            .unwrap()
            .contains("@specforge/software"),
        "{error}"
    );
    assert_eq!(
        model_kinds(&mut server, json!({"extension": "@specforge/software"})),
        all
    );
    // root + depth: the root kind and what it reaches in one hop.
    let near = model_kinds(&mut server, json!({"root": "behavior", "depth": 1}));
    assert!(near.contains(&"behavior".to_string()), "{near:?}");
    assert!(near.len() <= all.len());
    // group_by and fields change the rendering, not the kinds.
    let grouped = model_text(&mut server, json!({"format": "markdown"}));
    let flat = model_text(
        &mut server,
        json!({"format": "markdown", "group_by": "none"}),
    );
    assert_ne!(grouped, flat);
    let keys = model_text(&mut server, json!({"format": "dbml"}));
    let every = model_text(&mut server, json!({"format": "dbml", "fields": "all"}));
    assert!(every.len() > keys.len());
}

#[specforge_test(
    behavior = "filter_model",
    verify = "a depth without a root is refused"
)]
fn a_model_depth_without_a_root_is_refused() {
    let (mut server, _project) = model_server();
    let resp = call_tool(&mut server, "specforge.model", json!({"depth": 0}));
    assert_eq!(resp["result"]["isError"], true, "{resp}");
    let error = tool_json(&resp);
    assert_eq!(error["code"], "invalid_input");
    assert_eq!(error["argument"], "root");
    assert_eq!(
        error["message"],
        "a depth counts from a root kind: give a root too"
    );
}

#[specforge_test(
    behavior = "filter_model",
    verify = "an empty kind list selects every kind"
)]
fn an_empty_kind_list_selects_every_kind() {
    let (mut server, _project) = model_server();
    assert_eq!(
        model_kinds(&mut server, json!({"kinds": []})),
        model_kinds(&mut server, json!({}))
    );
}

#[specforge_test(
    behavior = "filter_model",
    verify = "a root no loaded extension declares is refused with unknown_kind naming the closest declared kind"
)]
fn an_unknown_model_root_is_refused_on_root() {
    let (mut server, _project) = model_server();
    let resp = call_tool(&mut server, "specforge.model", json!({"root": "behaviour"}));
    assert_eq!(resp["result"]["isError"], true, "{resp}");
    let error = tool_json(&resp);
    assert_eq!(error["code"], "invalid_input");
    assert_eq!(error["argument"], "root");
    assert_eq!(error["message"], "unknown entity kind 'behaviour'");
    assert_eq!(error["data"]["suggestion"], "did you mean 'behavior'?");
}

#[specforge_test(
    behavior = "filter_model",
    verify = "a listed kind the project does not know is reported as I020 and selects nothing"
)]
fn an_unknown_model_kind_is_an_i020_notice() {
    let (mut server, _project) = model_server();
    let resp = call_tool(
        &mut server,
        "specforge.model",
        json!({"format": "json", "kinds": ["behavior", "behaviour"]}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    let model: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let kinds: Vec<&str> = model["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["behavior"]);
    assert_eq!(
        resp["result"]["_meta"]["diagnostics"],
        json!([unknown_kind("behaviour", Some("behavior"))])
    );
}

#[specforge_test(
    behavior = "expose_model_mcp_tool",
    verify = "Expose Model as MCP Tool: MCP model tool holds — validation_complete_fired, tool_registered, all_formats_available, all_filters_available, result_is_string"
)]
fn contract_model() {
    // validation_complete_fired: initialize compiled the project.
    let (mut server, _project) = model_server();
    // tool_registered.
    let tools = call(&mut server, "tools/list", json!({}));
    assert!(
        tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "specforge.model")
    );
    // all_formats_available and result_is_string: each format is one text
    // block, with no structured content.
    for format in ["markdown", "mermaid", "dot", "json", "dbml"] {
        let resp = call_tool(&mut server, "specforge.model", json!({"format": format}));
        let content = resp["result"]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1, "{format}: {resp}");
        assert_eq!(content[0]["type"], "text");
        assert!(!content[0]["text"].as_str().unwrap().is_empty());
        assert!(resp["result"].get("structuredContent").is_none(), "{resp}");
    }
    // all_filters_available: every filter is accepted.
    let filtered = call_tool(
        &mut server,
        "specforge.model",
        json!({"extension": "@specforge/software", "kinds": ["behavior"], "root": "behavior",
               "depth": 1, "group_by": "none", "fields": "all"}),
    );
    assert_eq!(filtered["result"]["isError"], false, "{filtered}");
    assert_tool_invoked(&server, "specforge.model");
}

#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "entity_id filter returns single entity coverage"
)]
fn coverage_by_entity_id_is_one_row() {
    let mut server = test_server();
    let rows = tool(
        &mut server,
        "specforge.coverage",
        json!({"entity_id": "alpha"}),
    )["entities"]
        .clone();
    assert_eq!(rows.as_array().unwrap().len(), 1, "{rows}");
    assert_eq!(rows[0]["entity_id"], "alpha");
    assert_eq!(rows[0]["obligations"], 1);
    // The kind filter does not narrow a named entity, and an entity the
    // graph lacks has no row.
    let named = tool(
        &mut server,
        "specforge.coverage",
        json!({"entity_id": "alpha", "kind": "feature"}),
    )["entities"]
        .clone();
    assert_eq!(named, rows);
    let ghost = tool(
        &mut server,
        "specforge.coverage",
        json!({"entity_id": "ghost"}),
    )["entities"]
        .clone();
    assert_eq!(ghost, json!([]));
}
