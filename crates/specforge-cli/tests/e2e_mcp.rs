use crate::e2e_fixtures::*;
use specforge_test_macros::test as specforge_test;
use std::io::Write;
use std::process::{Command, Stdio};

// --- Existing MCP smoke tests ---

fn specforge_binary() -> Command {
    Command::new(assert_cmd::cargo_bin!("specforge"))
}

/// The ids of a Graph Protocol document's nodes, sorted.
fn node_ids(doc: &serde_json::Value) -> Vec<&str> {
    let mut ids: Vec<&str> = doc["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("no nodes array: {doc}"))
        .iter()
        .map(|n| n["id"].as_str().expect("node id"))
        .collect();
    ids.sort_unstable();
    ids
}

/// A Graph Protocol document's edges as sorted (source, label, target).
fn edge_triples(doc: &serde_json::Value) -> Vec<(&str, &str, &str)> {
    let mut edges: Vec<(&str, &str, &str)> = doc["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("no edges array: {doc}"))
        .iter()
        .map(|e| {
            (
                e["source"].as_str().expect("edge source"),
                e["label"].as_str().expect("edge label"),
                e["target"].as_str().expect("edge target"),
            )
        })
        .collect();
    edges.sort_unstable();
    edges
}

/// The JSON a `resources/read` response carries in `contents[0].text`.
fn resource_json(resp: &serde_json::Value) -> serde_json::Value {
    let text = resp["result"]["contents"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no contents[0].text: {resp}"));
    serde_json::from_str(text).unwrap_or_else(|e| panic!("resource text not JSON: {e}\n{text}"))
}

/// The JSON data message (the second user message, after the instruction)
/// a `prompts/get` response carries.
fn prompt_data(resp: &serde_json::Value) -> serde_json::Value {
    let messages = resp["result"]["messages"]
        .as_array()
        .unwrap_or_else(|| panic!("no messages: {resp}"));
    assert_eq!(messages.len(), 2, "instruction + data messages: {resp}");
    assert!(
        messages.iter().all(|m| m["role"] == "user"),
        "both messages are user messages: {resp}"
    );
    let text = messages.last().unwrap()["content"]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("data message has no text: {resp}"));
    serde_json::from_str(text).unwrap_or_else(|e| panic!("data message not JSON: {e}\n{text}"))
}

/// Every edge of BASIC_SPEC, as (source, label, target).
const BASIC_EDGES: [(&str, &str, &str); 3] = [
    ("gamma", "behaviors", "alpha"),
    ("gamma", "behaviors", "beta"),
    ("inv", "enforced_by", "alpha"),
];

#[specforge_test(
    behavior = "mcp_initialize",
    verify = "all core tools registered before accepting requests"
)]
fn mcp_server_responds_to_initialize() {
    let dir = setup_project(&[("main.spec", r#"behavior alpha "A" { contract "first" }"#)]);

    let mut child = specforge_binary()
        .args(["mcp"])
        .arg(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start specforge mcp");

    let stdin = child.stdin.as_mut().unwrap();

    // A standard client handshake: `initialize` without `projectRoot`, the
    // `initialized` notification, then a request.
    writeln!(stdin, "{}", mcp_initialize(1)).unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .unwrap();
    writeln!(
        stdin,
        "{}",
        mcp_request(2, "tools/list", serde_json::json!({}))
    )
    .unwrap();
    stdin.flush().unwrap();

    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    let responses: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("invalid JSON {e}: {l}")))
        .collect();
    // One response per request, nothing unsolicited.
    let ids: Vec<&serde_json::Value> = responses.iter().map(|r| &r["id"]).collect();
    assert_eq!(
        ids,
        [1, 2],
        "exactly the two requests are answered: {stdout}"
    );

    let init = &responses[0];
    assert!(
        init["error"].is_null(),
        "the client's initialize must succeed: {init}"
    );
    assert!(init["result"]["capabilities"]["tools"].is_object());

    // The very first request after the handshake already sees every core tool.
    let tools = responses[1]["result"]["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name"))
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
    ] {
        assert!(
            names.contains(&core),
            "core tool {core} missing from tools/list: {names:?}"
        );
    }
}

#[test]
fn mcp_initialize_without_project_root_compiles_cli_path() {
    let responses = mcp_session(
        r#"behavior alpha "A" { contract "first" }"#,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({"name": "specforge.search", "arguments": {"query": "alpha"}}),
        )],
    );

    let init = find_response(&responses, 0).expect("initialize response");
    assert!(init["error"].is_null(), "initialize succeeds: {init}");
    let search = find_response(&responses, 1).expect("search response");
    let text = search["result"]["content"][0]["text"]
        .as_str()
        .expect("search text");
    assert!(
        text.contains("alpha"),
        "the `specforge mcp <path>` project was compiled: {text}"
    );
}

#[test]
fn mcp_server_lists_tools() {
    let dir = setup_project(&[("main.spec", r#"behavior alpha "A" { contract "first" }"#)]);

    let mut child = specforge_binary()
        .args(["mcp"])
        .arg(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start specforge mcp");

    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, "{}", mcp_initialize(0)).unwrap();
    let request = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
    writeln!(stdin, "{}", request).unwrap();
    stdin.flush().unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();

    let tools_response = lines.iter().find(|line| {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            v["result"]["tools"].is_array()
        } else {
            false
        }
    });

    assert!(
        tools_response.is_some(),
        "should have a tools/list response with tools array. lines: {:?}",
        lines,
    );
}

#[test]
fn mcp_server_handles_eof_gracefully() {
    let dir = setup_project(&[("main.spec", r#"behavior alpha "A" { contract "first" }"#)]);

    let mut child = specforge_binary()
        .args(["mcp"])
        .arg(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start specforge mcp");

    drop(child.stdin.take());

    let status = child.wait().unwrap();
    assert!(
        status.success(),
        "MCP server should exit 0 on EOF, got {:?}",
        status.code()
    );
}

// --- Protocol & Error Handling ---

const BASIC_SPEC: &str = r#"behavior alpha "Alpha" { contract "first" }
behavior beta "Beta" { contract "second" }
feature gamma "Gamma" { problem "p" solution "s" behaviors [alpha, beta] }
invariant inv "Invariant" { guarantee "always" enforced_by [alpha] }"#;

#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "invalid method produces -32601 Method not found"
)]
fn mcp_unknown_method_returns_error() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(1, "nonexistent/method", serde_json::json!({}))],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_object(), "should be error response");
    assert_eq!(resp["error"]["code"], -32601, "should be METHOD_NOT_FOUND");
}

#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "malformed JSON produces -32700 Parse error"
)]
fn mcp_invalid_json_returns_parse_error() {
    let dir = setup_project(&[("main.spec", BASIC_SPEC)]);

    let mut child = specforge_binary()
        .args(["mcp"])
        .arg(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start specforge mcp");

    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, "{{not valid json}}").unwrap();
    stdin.flush().unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();

    // The bad line gets exactly one response: a -32700 Parse error.
    assert_eq!(lines.len(), 1, "one response to one bad line: {lines:?}");
    let resp: serde_json::Value = serde_json::from_str(lines[0]).expect("response is JSON");
    assert_eq!(resp["error"]["code"], -32700, "Parse error: {resp}");
    assert_eq!(resp["error"]["message"], "Parse error", "{resp}");
}

#[specforge_test(
    behavior = "handle_mcp_protocol_error",
    verify = "missing required params produces -32602 Invalid params"
)]
fn mcp_tool_call_missing_name_returns_error() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(1, "tools/call", serde_json::json!({}))],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_object(), "should be error response");
    assert_eq!(resp["error"]["code"], -32602, "should be INVALID_PARAMS");
}

#[test]
fn mcp_ping_returns_empty_object() {
    let responses = mcp_session(BASIC_SPEC, &[mcp_request(1, "ping", serde_json::json!({}))]);

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(
        resp["result"].is_object(),
        "ping should return result object"
    );
    assert!(
        !resp.get("error").is_some_and(|e| e.is_object()),
        "ping should not return error"
    );
}

// --- Tool Invocations ---

#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "specforge.query tool returns subgraph for valid entityId"
)]
fn mcp_tool_query_returns_subgraph() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.query",
                "arguments": { "entity_id": "gamma", "depth": 1 }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    // gamma and its direct neighbors; inv is two hops away (via alpha).
    assert_eq!(node_ids(&content), ["alpha", "beta", "gamma"], "{content}");
    assert_eq!(
        edge_triples(&content),
        [
            ("gamma", "behaviors", "alpha"),
            ("gamma", "behaviors", "beta")
        ],
        "{content}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_trace_tool",
    verify = "specforge.trace tool returns traceability chain for valid entityId"
)]
fn mcp_tool_trace_returns_chain() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.trace",
                "arguments": { "entity_id": "alpha" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert_eq!(content["entity_id"], "alpha");
    assert_eq!(content["entity_kind"], "behavior");
    // gamma (behaviors) and inv (enforced_by) reference alpha.
    let mut upstream: Vec<(&str, &str, u64)> = content["upstream"]
        .as_array()
        .expect("upstream array")
        .iter()
        .map(|l| {
            (
                l["entity_id"].as_str().unwrap(),
                l["edge_label"].as_str().unwrap(),
                l["depth"].as_u64().unwrap(),
            )
        })
        .collect();
    upstream.sort_unstable();
    assert_eq!(
        upstream,
        [("gamma", "behaviors", 1), ("inv", "enforced_by", 1)],
        "{content}"
    );
    assert_eq!(
        content["downstream"],
        serde_json::json!([]),
        "alpha references nothing: {content}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_trace_tool",
    verify = "missing links flagged in trace output"
)]
fn mcp_tool_trace_of_an_isolated_entity_lists_no_invented_gaps() {
    let responses = mcp_session(
        ISOLATED_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.trace",
                "arguments": { "entity_id": "isolated_node" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    // Isolation is an empty upstream and downstream; what the entity
    // lacks are its missing links, the trace document's only gaps.
    assert_eq!(content["upstream"], serde_json::json!([]), "{content}");
    assert_eq!(content["downstream"], serde_json::json!([]), "{content}");
    assert!(content["missing"].is_array(), "{content}");
    assert!(content.get("gaps").is_none(), "{content}");
}

// The MCP trace flags the same missing links as `specforge trace`: pay has
// its feature, but none of the edges the software extension declares for
// behaviors toward ports, events or types.
#[specforge_test(
    behavior = "provide_mcp_trace_tool",
    verify = "missing links flagged in trace output"
)]
fn mcp_tool_trace_flags_missing_links() {
    let dir = setup_project_with_config(
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software","@specforge/product"]}"#,
        &[(
            "main.spec",
            r#"
feature checkout "Checkout" {
    problem "Buyers need to pay"
    solution "A payment step"
}

behavior pay "Pay" {
    contract "The system MUST take payment once"
    features [checkout]
}
"#,
        )],
    );
    let responses = mcp_session_in(
        &dir,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.trace",
                "arguments": { "entity_id": "pay" }
            }),
        )],
    );
    let resp = find_response(&responses, 1).expect("should get response for id 1");
    let content = parse_tool_content(resp);
    let labels: Vec<&str> = content["missing"]
        .as_array()
        .unwrap_or_else(|| panic!("trace has no missing array: {content}"))
        .iter()
        .inspect(|m| {
            assert_eq!(m["from"], "pay");
            assert_eq!(m["status"], "missing");
        })
        .map(|m| m["edge_label"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        ["consumes", "invariants", "ports", "produces", "types"],
        "{content}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "specforge.export tool returns graph in requested format"
)]
fn mcp_tool_export_graph_format() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.export",
                "arguments": { "format": "graph" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    // Graph Protocol 2.0 with the schema, as `specforge export` writes it.
    assert_eq!(content["format_version"], "2.0", "{content}");
    assert!(content["schema"].is_object(), "{content}");
    // The whole graph: every entity and every reference.
    assert_eq!(
        node_ids(&content),
        ["alpha", "beta", "gamma", "inv"],
        "{content}"
    );
    assert_eq!(edge_triples(&content), BASIC_EDGES, "{content}");
    // The graph format carries each node's source location and full fields.
    let inv = content["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "inv")
        .unwrap();
    assert_eq!(
        *inv,
        serde_json::json!({
            "id": "inv", "kind": "invariant", "title": "Invariant",
            "file": "main.spec", "line": 4,
            "fields": { "enforced_by": ["alpha"], "guarantee": "always" }
        })
    );
}

#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "text search finds entities matching by name or contract"
)]
fn mcp_tool_search_fuzzy_match() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[
            mcp_request(
                1,
                "tools/call",
                serde_json::json!({
                    "name": "specforge.search",
                    "arguments": { "query": "alph" }
                }),
            ),
            // "second" is no entity's name: only beta's contract contains it.
            mcp_request(
                2,
                "tools/call",
                serde_json::json!({
                    "name": "specforge.search",
                    "arguments": { "query": "second" }
                }),
            ),
        ],
    );

    let result_ids = |id: u64| -> Vec<String> {
        let resp = find_response(&responses, id).expect("search response");
        assert!(resp["error"].is_null(), "should not be error: {}", resp);
        parse_tool_content(resp)["results"]
            .as_array()
            .expect("search returns an array")
            .iter()
            .map(|r| r["entity_id"].as_str().unwrap().to_string())
            .collect()
    };

    assert_eq!(
        result_ids(1),
        ["alpha"],
        "a partial name finds the entity by name"
    );
    assert_eq!(
        result_ids(2),
        ["beta"],
        "contract text finds the entity whose contract matches"
    );
}

#[specforge_test(
    behavior = "provide_mcp_stats_tool",
    verify = "specforge.stats returns entity counts by kind"
)]
fn mcp_tool_stats_returns_counts() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.stats",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    let mut counts: Vec<(&str, u64)> = content["entity_counts"]
        .as_array()
        .expect("entity_counts array")
        .iter()
        .map(|c| (c["kind"].as_str().unwrap(), c["count"].as_u64().unwrap()))
        .collect();
    counts.sort_unstable();
    assert_eq!(
        counts,
        [("behavior", 2), ("feature", 1), ("invariant", 1)],
        "{content}"
    );
    assert_eq!(content["edge_count"], 3, "{content}");
}

#[test]
fn mcp_tool_unknown_returns_error() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "nonexistent.tool",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(
        resp["error"].is_object(),
        "should be error for unknown tool"
    );
    assert_eq!(
        resp["error"]["code"], -32602,
        "unknown tool is an Invalid params protocol error (MCP spec example)"
    );
}

// --- Resource Reads ---

#[test]
fn mcp_resource_list_returns_six_resources() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[
            mcp_request(1, "resources/list", serde_json::json!({})),
            mcp_request(2, "resources/templates/list", serde_json::json!({})),
        ],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let resources = resp["result"]["resources"]
        .as_array()
        .expect("should have resources array");
    assert_eq!(
        resources.len(),
        5,
        "should have 5 plain default resources, got {}",
        resources.len()
    );
    // The templated ones (graph/{entity_id}, context/{entity_id},
    // entities/{kind}) are resource templates.
    let resp = find_response(&responses, 2).expect("should get response for id 2");
    let templates = resp["result"]["resourceTemplates"]
        .as_array()
        .expect("should have resourceTemplates array");
    assert_eq!(templates.len(), 3, "{resp}");
}

#[specforge_test(
    behavior = "expose_graph_as_mcp_resource",
    verify = "specforge://graph resource returns full Graph Protocol JSON"
)]
fn mcp_resource_read_graph() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "resources/read",
            serde_json::json!({ "uri": "specforge://graph" }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    assert_eq!(resp["result"]["contents"][0]["uri"], "specforge://graph");
    let doc = resource_json(resp);
    assert!(doc["schema_version"].is_string(), "schema_version: {doc}");
    assert!(doc["schema"].is_object(), "embedded schema: {doc}");
    // The full graph: every entity with its fields, every reference.
    assert_eq!(node_ids(&doc), ["alpha", "beta", "gamma", "inv"], "{doc}");
    assert_eq!(edge_triples(&doc), BASIC_EDGES, "{doc}");
    let gamma = doc["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "gamma")
        .unwrap();
    assert_eq!(gamma["kind"], "feature");
    assert_eq!(
        gamma["fields"],
        serde_json::json!({ "behaviors": ["alpha", "beta"], "problem": "p", "solution": "s" })
    );
}

#[specforge_test(
    behavior = "expose_diagnostics_as_mcp_resource",
    verify = "specforge://diagnostics resource returns current DiagnosticBag as JSON"
)]
fn mcp_resource_read_diagnostics() {
    // Line 5 references `alpah`, which no entity declares.
    let spec = format!(
        "{BASIC_SPEC}\nfeature broken \"Broken\" {{ problem \"p\" solution \"s\" behaviors [alpah] }}"
    );
    let responses = mcp_session(
        &spec,
        &[mcp_request(
            1,
            "resources/read",
            serde_json::json!({ "uri": "specforge://diagnostics" }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    assert_eq!(
        resp["result"]["contents"][0]["uri"],
        "specforge://diagnostics"
    );
    let diagnostics = resource_json(resp);
    let bag = diagnostics.as_array().expect("the bag is a JSON array");
    // The project enables no extension, so the structural-only notice
    // (I002) comes first, then exactly the unresolved reference.
    assert_eq!(
        bag.len(),
        2,
        "I002 and exactly the unresolved reference: {diagnostics}"
    );
    assert_eq!(bag[0]["code"], "I002", "{diagnostics}");
    let d = &bag[1];
    assert_eq!(d["code"], "E003", "{d}");
    assert_eq!(d["severity"], "Error", "{d}");
    assert_eq!(
        d["message"], "unresolved reference 'alpah' in entity 'broken'",
        "{d}"
    );
    assert_eq!(d["file"], "main.spec", "{d}");
    assert_eq!(d["line"], 5, "{d}");
    assert_eq!(d["column"], 63, "{d}");
}

#[specforge_test(
    behavior = "expose_entity_as_mcp_resource",
    verify = "specforge://graph/{entity_id} returns entity and its neighbors"
)]
fn mcp_resource_read_entity_subgraph() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "resources/read",
            serde_json::json!({ "uri": "specforge://graph/alpha" }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let doc = resource_json(resp);
    // alpha plus the entities directly connected to it; beta is two hops
    // away (via gamma) and stays out.
    assert_eq!(node_ids(&doc), ["alpha", "gamma", "inv"], "{doc}");
    assert_eq!(
        edge_triples(&doc),
        [
            ("gamma", "behaviors", "alpha"),
            ("inv", "enforced_by", "alpha")
        ],
        "{doc}"
    );
}

// --- Prompts listing ---

#[test]
fn mcp_prompts_list_returns_prompts() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(1, "prompts/list", serde_json::json!({}))],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let prompts = resp["result"]["prompts"]
        .as_array()
        .expect("should have prompts array");
    assert!(
        prompts.len() >= 3,
        "should have at least 3 default prompts, got {}",
        prompts.len()
    );
}

// --- Multiple requests in one session ---

#[test]
fn mcp_multiple_requests_in_single_session() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[
            mcp_request(1, "tools/list", serde_json::json!({})),
            mcp_request(2, "resources/list", serde_json::json!({})),
            mcp_request(3, "ping", serde_json::json!({})),
        ],
    );

    assert!(
        find_response(&responses, 1).is_some(),
        "should get tools/list response"
    );
    assert!(
        find_response(&responses, 2).is_some(),
        "should get resources/list response"
    );
    assert!(
        find_response(&responses, 3).is_some(),
        "should get ping response"
    );
}

// ============================================================
// Phase 1: Navigation Tools
// ============================================================

#[test]
fn mcp_tool_validate_returns_diagnostics() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.validate",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .expect("should have content[0].text");
    // Diagnostics text should be valid JSON (array of diagnostics)
    let _parsed: serde_json::Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("diagnostics text not valid JSON: {}\ntext: {}", e, text));
    // isError field should be present on the result
    assert!(
        resp["result"].get("isError").is_some()
            || resp["result"]["content"][0].get("isError").is_some()
            || text.contains("[]"),
        "validate should return diagnostics or isError field"
    );
}

// Not linked to "specforge.schema returns full GraphProtocolSchema": the
// tool returns a summary of the graph (each kind with the fields it uses,
// the edge labels, the graph format's version), not the GraphProtocolSchema
// `specforge schema` prints. This pins the summary.
/// BASIC_SPEC in a project that loads the extensions its kinds come from.
fn basic_project_with_extensions() -> tempfile::TempDir {
    setup_project_with_config(
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software","@specforge/product"]}"#,
        &[("main.spec", BASIC_SPEC)],
    )
}

#[test]
fn mcp_tool_schema_returns_entity_kinds() {
    let dir = basic_project_with_extensions();
    let responses = mcp_session_in(
        &dir,
        &[
            mcp_request(
                1,
                "tools/call",
                serde_json::json!({ "name": "specforge.schema", "arguments": {} }),
            ),
            mcp_request(
                2,
                "tools/call",
                serde_json::json!({ "name": "specforge.export", "arguments": { "format": "graph" } }),
            ),
        ],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    // The kinds the two extensions declare, typed, whether or not the
    // graph uses them.
    let kinds: Vec<&str> = content["entity_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["name"].as_str().unwrap())
        .collect();
    for kind in ["behavior", "feature", "invariant", "event", "port"] {
        assert!(kinds.contains(&kind), "{kind} missing from {kinds:?}");
    }
    let export = parse_tool_content(find_response(&responses, 2).expect("export response"));
    assert_eq!(
        content, export["schema"],
        "the schema a graph export embeds"
    );
}

#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "specforge.coverage returns coverage for all testable entities"
)]
fn mcp_tool_coverage_returns_status() {
    // alpha declares two obligations; the rest declare none.
    let spec = BASIC_SPEC.replace(
        r#"contract "first" }"#,
        r#"contract "first" verify unit "alpha works" verify unit "alpha fails safely" }"#,
    );
    // Testability comes from the extensions; without any, nothing is testable.
    let dir = testable_project(&spec);
    let responses = mcp_session_in(
        &dir,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.coverage",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    let arr = content["entities"]
        .as_array()
        .expect("coverage should return array");
    // No filter: one entry for every testable entity — the behaviors and
    // the invariant — none left out. The feature (gamma) is not testable.
    let mut ids: Vec<&str> = arr
        .iter()
        .map(|e| e["entity_id"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, ["alpha", "beta", "inv"], "{content}");

    let alpha = arr.iter().find(|e| e["entity_id"] == "alpha").unwrap();
    assert_eq!(alpha["kind"], "behavior");
    assert_eq!(alpha["obligations"], 2, "{alpha}");
    assert_eq!(alpha["proven"], 0, "no test report recorded: {alpha}");
    assert_eq!(
        alpha["unproven"],
        serde_json::json!(["alpha works", "alpha fails safely"])
    );
    assert_eq!(alpha["status"], "uncovered", "{alpha}");
    let beta = arr.iter().find(|e| e["entity_id"] == "beta").unwrap();
    assert_eq!(beta["obligations"], 0, "{beta}");
    assert_eq!(beta["status"], "uncovered", "{beta}");
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "specforge.inspect returns full entity details"
)]
fn mcp_tool_inspect_returns_entity_detail() {
    // The builtins declare behavior's `contract` the headline statement.
    let responses = mcp_session_with_builtins(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.inspect",
                "arguments": { "entity_id": "alpha" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert_eq!(content["entity_id"], "alpha");
    assert_eq!(content["kind"], "behavior", "{content}");
    assert_eq!(content["title"], "Alpha", "{content}");
    assert_eq!(content["contract"], "first", "{content}");
    assert_eq!(
        content["fields"],
        serde_json::json!({ "contract": "first" }),
        "{content}"
    );
    // gamma and inv both reference alpha.
    let mut referenced_by: Vec<&str> = content["referenced_by"]
        .as_array()
        .expect("referenced_by array")
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    referenced_by.sort_unstable();
    assert_eq!(referenced_by, ["gamma", "inv"], "{content}");
    assert!(content.get("references").is_none(), "{content}");
    assert_eq!(content["source_span"]["file"], "main.spec", "{content}");
    assert_eq!(content["source_span"]["start_line"], 1, "{content}");
    assert_eq!(content["coverage_status"], "uncovered", "{content}");
    // Only alpha's own diagnostics (the builtins' rules), never beta's.
    let diagnostics = content["diagnostics"].as_array().expect("diagnostics");
    assert!(
        diagnostics
            .iter()
            .all(|d| d["message"].as_str().unwrap().contains("'alpha'")),
        "{content}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "non-existent entity returns error response"
)]
fn mcp_tool_inspect_missing_entity_returns_error() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.inspect",
                "arguments": { "entity_id": "nonexistent_entity" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    // C9-00/C9-12: missing entity is a tool execution error (isError
    // result), not a -32602 protocol error.
    assert!(
        resp["result"]["isError"] == true,
        "missing entity must be an isError tool result"
    );
}

#[specforge_test(
    behavior = "provide_mcp_find_definition_tool",
    verify = "specforge.find_definition returns file, line, and column"
)]
fn mcp_tool_find_definition_returns_location() {
    // alpha's declaration starts on line 3, column 3; its name, where the
    // position points, at column 12.
    let spec = "behavior beta \"Beta\" { contract \"second\" }\n\n  behavior alpha \"Alpha\" {\n    contract \"first\"\n  }\n";
    let responses = mcp_session(
        spec,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.find_definition",
                "arguments": { "entity_id": "alpha" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert_eq!(content["entity_id"], "alpha");
    assert_eq!(content["file_path"], "main.spec", "{content}");
    assert_eq!(content["line"], 3, "{content}");
    assert_eq!(content["column"], 12, "{content}");
    assert_eq!(content["source_span"]["start_col"], 3, "{content}");
}

#[specforge_test(
    behavior = "provide_mcp_find_references_tool",
    verify = "specforge.find_references returns all reference locations"
)]
fn mcp_tool_find_references_returns_locations() {
    // alpha is referenced by gamma's behaviors list
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.find_references",
                "arguments": { "entity_id": "alpha" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert_eq!(content["entity_id"], "alpha");
    // Both referencing entities, each at its token: gamma's behaviors
    // [alpha, ...] (line 3) and inv's enforced_by [alpha] (line 4).
    let mut locations: Vec<(&str, &str, u64, u64)> = content["locations"]
        .as_array()
        .expect("should have locations array")
        .iter()
        .map(|l| {
            (
                l["referencing_entity_id"].as_str().unwrap(),
                l["source_span"]["file"].as_str().unwrap(),
                l["source_span"]["start_line"].as_u64().unwrap(),
                l["source_span"]["start_col"].as_u64().unwrap(),
            )
        })
        .collect();
    locations.sort_unstable();
    assert_eq!(
        locations,
        [("gamma", "main.spec", 3, 61), ("inv", "main.spec", 4, 61)],
        "{content}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "specforge.outline returns all entities defined in file"
)]
fn mcp_tool_outline_returns_entities_in_file() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.outline",
                "arguments": { "file": "main.spec" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    let arr = content["entries"]
        .as_array()
        .expect("outline should return array");
    assert!(
        arr.len() >= 4,
        "BASIC_SPEC has 4 entities, got {}",
        arr.len()
    );

    // Should be sorted by start_line
    let lines: Vec<u64> = arr
        .iter()
        .map(|e| {
            e["range"]["start_line"]
                .as_u64()
                .expect("should have range.start_line")
        })
        .collect();
    let mut sorted = lines.clone();
    sorted.sort();
    assert_eq!(lines, sorted, "outline should be sorted by start_line");
}

// ============================================================
// Phase 2: Tool Parameter Variants
// ============================================================

#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "format parameter changes output serialization"
)]
fn mcp_tool_query_format_context() {
    let responses = mcp_session_with_builtins(
        BASIC_SPEC,
        &[
            mcp_request(
                1,
                "tools/call",
                serde_json::json!({
                    "name": "specforge.query",
                    "arguments": { "entity_id": "gamma", "depth": 1, "format": "context" }
                }),
            ),
            mcp_request(
                2,
                "tools/call",
                serde_json::json!({
                    "name": "specforge.query",
                    "arguments": { "entity_id": "gamma", "depth": 1 }
                }),
            ),
        ],
    );

    let context = parse_tool_content(find_response(&responses, 1).expect("context response"));
    let graph = parse_tool_content(find_response(&responses, 2).expect("default response"));
    // Same subgraph either way...
    assert_eq!(node_ids(&context), ["alpha", "beta", "gamma"], "{context}");
    assert_eq!(node_ids(&graph), node_ids(&context));
    // ...serialized differently: context lifts the contract and drops the
    // source location and raw fields; the default graph format keeps them.
    let node = |doc: &serde_json::Value, id: &str| -> serde_json::Value {
        doc["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(
        node(&context, "alpha"),
        serde_json::json!({ "id": "alpha", "kind": "behavior", "title": "Alpha", "contract": "first" })
    );
    assert_eq!(
        node(&graph, "alpha"),
        serde_json::json!({
            "id": "alpha", "kind": "behavior", "title": "Alpha",
            "file": "main.spec", "line": 1, "fields": { "contract": "first" }
        })
    );
}

#[test]
fn mcp_tool_query_format_brief() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.query",
                "arguments": { "entity_id": "gamma", "depth": 1, "format": "brief" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let _content: serde_json::Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("brief output not valid JSON: {}\ntext: {}", e, text));
}

#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "include_coverage parameter includes coverage status in response"
)]
fn mcp_tool_query_include_coverage() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.query",
                "arguments": { "entity_id": "gamma", "depth": 1, "include_coverage": true }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    // With include_coverage, nodes should have coverage_status
    let nodes = content["nodes"].as_array().expect("should have nodes");
    assert!(!nodes.is_empty(), "should have nodes");
    let has_coverage = nodes.iter().any(|n| n.get("coverage_status").is_some());
    assert!(
        has_coverage,
        "at least one node should have coverage_status when include_coverage=true"
    );
}

#[specforge_test(
    behavior = "provide_mcp_query_tool",
    verify = "kind filter restricts returned node types"
)]
fn mcp_tool_query_with_kinds_filter() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.query",
                "arguments": { "entity_id": "gamma", "depth": 2, "kinds": ["behavior"] }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    let nodes = content["nodes"].as_array().expect("should have nodes");
    // All non-root nodes should be behaviors (root gamma is a feature but is always included)
    for node in nodes {
        let kind = node["kind"].as_str().unwrap_or("");
        let id = node["id"].as_str().unwrap_or("");
        if id != "gamma" {
            assert_eq!(
                kind, "behavior",
                "filtered node {} should be behavior, got {}",
                id, kind
            );
        }
    }
}

#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "scope parameter restricts to subgraph"
)]
fn mcp_tool_export_scoped() {
    // `loner` is connected to nothing, so alpha's subgraph leaves it out.
    let spec = format!("{BASIC_SPEC}\nbehavior loner \"Loner\" {{ contract \"alone\" }}");
    let responses = mcp_session(
        &spec,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.export",
                "arguments": { "scope": "alpha" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    // alpha's connected subgraph: everything reachable from it, not loner.
    assert_eq!(
        node_ids(&content),
        ["alpha", "beta", "gamma", "inv"],
        "{content}"
    );
    assert_eq!(edge_triples(&content), BASIC_EDGES, "{content}");
}

#[specforge_test(
    behavior = "provide_mcp_export_tool",
    verify = "all three formats (context, brief, graph) supported"
)]
fn mcp_tool_export_format_context() {
    let export = |id: u64, format: &str| {
        mcp_request(
            id,
            "tools/call",
            serde_json::json!({
                "name": "specforge.export",
                "arguments": { "format": format }
            }),
        )
    };
    let responses = mcp_session_with_builtins(
        BASIC_SPEC,
        &[export(1, "context"), export(2, "brief"), export(3, "graph")],
    );

    let beta_of = |id: u64| -> serde_json::Value {
        let resp = find_response(&responses, id).expect("export response");
        assert!(resp["error"].is_null(), "should not be error: {}", resp);
        let doc = parse_tool_content(resp);
        assert_eq!(node_ids(&doc), ["alpha", "beta", "gamma", "inv"], "{doc}");
        assert_eq!(edge_triples(&doc), BASIC_EDGES, "{doc}");
        doc["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "beta")
            .unwrap()
            .clone()
    };
    // Each format serializes the same node its own way.
    assert_eq!(
        beta_of(1),
        serde_json::json!({ "id": "beta", "kind": "behavior", "title": "Beta", "contract": "second" }),
        "context: identity plus the contract"
    );
    assert_eq!(
        beta_of(2),
        serde_json::json!({ "id": "beta", "kind": "behavior", "title": "Beta" }),
        "brief: identity only"
    );
    assert_eq!(
        beta_of(3),
        serde_json::json!({
            "id": "beta", "kind": "behavior", "title": "Beta",
            "file": "main.spec", "line": 2, "fields": { "contract": "second" }
        }),
        "graph: location and every field"
    );
}

#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "kind filter restricts results to matching entity kinds"
)]
fn mcp_tool_search_with_kinds() {
    // "a" matches entities of several kinds; the filter keeps behaviors.
    let responses = mcp_session(
        BASIC_SPEC,
        &[
            mcp_request(
                1,
                "tools/call",
                serde_json::json!({
                    "name": "specforge.search",
                    "arguments": { "query": "a" }
                }),
            ),
            mcp_request(
                2,
                "tools/call",
                serde_json::json!({
                    "name": "specforge.search",
                    "arguments": { "query": "a", "kinds": ["behavior"] }
                }),
            ),
        ],
    );

    let kinds_of = |id: u64| -> Vec<(String, String)> {
        let resp = find_response(&responses, id).expect("search response");
        assert!(resp["error"].is_null(), "should not be error: {}", resp);
        let mut hits: Vec<(String, String)> = parse_tool_content(resp)["results"]
            .as_array()
            .expect("search returns an array")
            .iter()
            .map(|r| {
                (
                    r["entity_id"].as_str().unwrap().to_string(),
                    r["kind"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        hits.sort_unstable();
        hits
    };
    let pair = |id: &str, kind: &str| (id.to_string(), kind.to_string());

    // "a" is in every id: alpha (prefix), beta, gamma (substring) and
    // inv's title "Invariant".
    assert_eq!(
        kinds_of(1),
        [
            pair("alpha", "behavior"),
            pair("beta", "behavior"),
            pair("gamma", "feature"),
            pair("inv", "invariant")
        ],
        "unfiltered, the query spans three kinds"
    );
    assert_eq!(
        kinds_of(2),
        [pair("alpha", "behavior"), pair("beta", "behavior")],
        "kinds=[behavior] keeps only the behaviors"
    );
}

#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "references filter returns entities referencing target"
)]
fn mcp_tool_search_references() {
    // Find entities that reference alpha
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.search",
                "arguments": { "query": "", "references": "alpha" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    let arr = content["results"]
        .as_array()
        .expect("search should return array");
    // gamma references alpha via behaviors [alpha, beta], and inv via
    // enforced_by [alpha]; alpha itself is not a referrer.
    let mut ids: Vec<&str> = arr
        .iter()
        .map(|r| r["entity_id"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, ["gamma", "inv"], "{content}");
}

/// `references` is one filter among the others, ANDed: the referrers of
/// alpha that are also invariants, or also match the query.
#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "the references filter combines with the other filters"
)]
fn mcp_tool_search_references_combines_with_the_other_filters() {
    let search = |id: u64, arguments: serde_json::Value| {
        mcp_request(
            id,
            "tools/call",
            serde_json::json!({"name": "specforge.search", "arguments": arguments}),
        )
    };
    let responses = mcp_session(
        BASIC_SPEC,
        &[
            search(
                1,
                serde_json::json!({"query": "", "references": "alpha", "kinds": ["invariant"]}),
            ),
            search(
                2,
                serde_json::json!({"query": "gamma", "references": "alpha"}),
            ),
            search(
                3,
                serde_json::json!({"query": "zzz", "references": "alpha"}),
            ),
            search(
                4,
                serde_json::json!({"query": "", "references": "alpha", "field": "guarantee", "value": "never"}),
            ),
            search(5, serde_json::json!({"query": "", "references": "nope"})),
        ],
    );
    let ids = |id: u64| -> Vec<String> {
        let resp = find_response(&responses, id).expect("a response");
        parse_tool_content(resp)["results"]
            .as_array()
            .expect("search returns an array")
            .iter()
            .map(|r| r["entity_id"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(ids(1), ["inv"], "kinds narrows the referrers");
    assert_eq!(ids(2), ["gamma"], "the query narrows the referrers");
    assert!(ids(3).is_empty(), "no referrer matches zzz");
    assert!(ids(4).is_empty(), "inv's guarantee does not contain never");
    assert!(ids(5).is_empty(), "an unknown target has no referrers");
}

// ============================================================
// Phase 3: Resources & Error Paths
// ============================================================

// Not linked to "specforge://schema resource returns GraphProtocolSchema
// JSON": the resource is the same graph summary as the specforge.schema
// tool, not the GraphProtocolSchema. This pins the summary.
#[test]
fn mcp_resource_read_schema() {
    let dir = basic_project_with_extensions();
    let responses = mcp_session_in(
        &dir,
        &[
            mcp_request(
                1,
                "resources/read",
                serde_json::json!({ "uri": "specforge://schema" }),
            ),
            mcp_request(
                2,
                "tools/call",
                serde_json::json!({ "name": "specforge.schema", "arguments": {} }),
            ),
        ],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    assert_eq!(
        resp["result"]["contents"][0]["mimeType"],
        "application/json"
    );
    let parsed = resource_json(resp);
    assert_eq!(
        parsed["schema_version"],
        serde_json::json!({"major": 1, "minor": 0, "patch": 0}),
        "{parsed}"
    );
    let tool = parse_tool_content(find_response(&responses, 2).expect("schema tool response"));
    assert_eq!(parsed, tool, "the resource is the unfiltered tool reply");
}

#[specforge_test(
    behavior = "expose_context_as_mcp_resource",
    verify = "specforge://context resource returns token-optimized format"
)]
fn mcp_resource_read_context() {
    let responses = mcp_session_with_builtins(
        BASIC_SPEC,
        &[
            mcp_request(
                1,
                "resources/read",
                serde_json::json!({ "uri": "specforge://context" }),
            ),
            mcp_request(
                2,
                "resources/read",
                serde_json::json!({ "uri": "specforge://graph" }),
            ),
        ],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let doc = resource_json(resp);
    assert_eq!(node_ids(&doc), ["alpha", "beta", "gamma", "inv"], "{doc}");
    assert_eq!(edge_triples(&doc), BASIC_EDGES, "{doc}");
    // Token-optimized: each node is its identity plus the contract, with no
    // source location, raw field map, or embedded schema.
    let alpha = doc["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "alpha")
        .unwrap();
    assert_eq!(
        *alpha,
        serde_json::json!({ "id": "alpha", "kind": "behavior", "title": "Alpha", "contract": "first" })
    );
    assert!(doc.get("schema").is_none(), "no embedded schema: {doc}");
    let text_len = |id: u64| {
        find_response(&responses, id).unwrap()["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .len()
    };
    assert!(
        text_len(1) < text_len(2),
        "context ({}) is smaller than the full graph ({})",
        text_len(1),
        text_len(2)
    );
}

#[specforge_test(
    behavior = "expose_brief_as_mcp_resource",
    verify = "specforge://brief resource returns minimal IDs and edges format"
)]
fn mcp_resource_read_brief() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "resources/read",
            serde_json::json!({ "uri": "specforge://brief" }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let doc = resource_json(resp);
    assert_eq!(node_ids(&doc), ["alpha", "beta", "gamma", "inv"], "{doc}");
    assert_eq!(edge_triples(&doc), BASIC_EDGES, "{doc}");
    // Minimal: nodes carry identity only, edges only their endpoints and label.
    for node in doc["nodes"].as_array().unwrap() {
        let mut keys: Vec<&str> = node
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["id", "kind", "title"], "brief node: {node}");
    }
    for edge in doc["edges"].as_array().unwrap() {
        let mut keys: Vec<&str> = edge
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["label", "source", "target"], "brief edge: {edge}");
    }
}

#[test]
fn mcp_resource_read_unknown_uri_returns_error() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "resources/read",
            serde_json::json!({ "uri": "specforge://nonexistent" }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_object(), "should be error for unknown URI");
    // Not found, as the handshake revisions say it (MCP 2025-11-25).
    assert_eq!(
        resp["error"]["code"], -32002,
        "should be resource not found"
    );
    assert_eq!(resp["error"]["data"]["uri"], "specforge://nonexistent");
}

#[test]
fn mcp_resource_read_missing_uri_returns_error() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(1, "resources/read", serde_json::json!({}))],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_object(), "should be error for missing uri");
    assert_eq!(resp["error"]["code"], -32602, "should be INVALID_PARAMS");
}

#[test]
fn mcp_resource_read_entity_not_found() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "resources/read",
            serde_json::json!({ "uri": "specforge://graph/nonexistent" }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(
        resp["error"].is_object(),
        "should be error for nonexistent entity"
    );
    assert_eq!(
        resp["error"]["code"], -32002,
        "should be resource not found"
    );
    assert_eq!(resp["error"]["data"]["code"], "entity_not_found");
    assert_eq!(
        resp["error"]["data"]["uri"],
        "specforge://graph/nonexistent"
    );
}

#[test]
fn mcp_resource_contents_format() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "resources/read",
            serde_json::json!({ "uri": "specforge://graph" }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let contents = resp["result"]["contents"]
        .as_array()
        .or_else(|| resp["result"]["content"].as_array())
        .expect("should have contents array");
    let first = &contents[0];
    assert!(
        first["uri"].is_string() || first.get("uri").is_some(),
        "contents[0] should have uri field: {}",
        first
    );
    assert!(
        first["mimeType"].is_string() || first.get("mimeType").is_some(),
        "contents[0] should have mimeType field: {}",
        first
    );
}

// ============================================================
// Phase 4: Prompts
// ============================================================

#[specforge_test(
    behavior = "provide_mcp_context_prompt",
    verify = "specforge://prompts/context returns structured entity context"
)]
fn mcp_prompt_context_returns_messages() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "prompts/get",
            serde_json::json!({
                "name": "specforge://prompts/context",
                "arguments": { "entity_id": "alpha" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let messages = resp["result"]["messages"]
        .as_array()
        .expect("prompt should return messages array");
    assert!(
        messages.len() >= 2,
        "should have instruction + data messages"
    );
    // Data is in the last message (a user message, after the instruction)
    let data_msg = messages.last().unwrap();
    let text = data_msg["content"]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("data message should have content.text: {}", data_msg));
    let parsed: serde_json::Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("data message text not valid JSON: {}\ntext: {}", e, text));
    assert_eq!(parsed["entity_id"], "alpha");
}

#[specforge_test(
    behavior = "provide_mcp_review_prompt",
    verify = "specforge://prompts/review returns coverage analysis"
)]
fn mcp_prompt_review_returns_findings() {
    // alpha declares an obligation no test proves; its neighbors declare none.
    let spec = BASIC_SPEC.replace(
        r#"contract "first" }"#,
        r#"contract "first" verify unit "alpha works" }"#,
    );
    let dir = testable_project(&spec);
    let responses = mcp_session_in(
        &dir,
        &[mcp_request(
            1,
            "prompts/get",
            serde_json::json!({
                "name": "specforge://prompts/review",
                "arguments": { "entity_id": "alpha" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let parsed = prompt_data(resp);

    // The coverage analysis spans alpha and its testable neighbors at the
    // default depth 1: inv. Not beta (two hops away), not gamma (a feature,
    // which is not testable).
    let mut summary: Vec<(&str, &str, bool)> = parsed["coverage_summary"]
        .as_array()
        .expect("coverage_summary array")
        .iter()
        .map(|c| {
            (
                c["entity_id"].as_str().unwrap(),
                c["status"].as_str().unwrap(),
                c["declared"].as_bool().unwrap(),
            )
        })
        .collect();
    summary.sort_unstable();
    assert_eq!(
        summary,
        [("alpha", "uncovered", true), ("inv", "uncovered", false)],
        "{parsed}"
    );
    let alpha = &parsed["coverage_summary"][0];
    assert_eq!(
        alpha["unproven"],
        serde_json::json!(["alpha works"]),
        "{parsed}"
    );

    // Entities with no verify declarations are called out; alpha has one.
    let mut missing: Vec<&str> = parsed["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .filter(|f| {
            f["message"]
                .as_str()
                .unwrap()
                .contains("no verify declarations")
        })
        .map(|f| f["entity_id"].as_str().unwrap())
        .collect();
    missing.sort_unstable();
    assert_eq!(missing, ["inv"], "{parsed}");
}

/// A project with the software and testing extensions, which make behaviors
/// and invariants testable, holding `spec` as its only spec file.
fn testable_project(spec: &str) -> tempfile::TempDir {
    setup_project_with_config(
        r#"{"name":"test","version":"0.1.0","extensions":["@specforge/software","@specforge/testing"]}"#,
        &[("main.spec", spec)],
    )
}

#[specforge_test(
    behavior = "provide_mcp_trace_prompt",
    verify = "response returns identified gaps with gap context"
)]
fn mcp_prompt_trace_returns_gaps() {
    // beta declares an obligation, so a plan must cover it.
    let spec = BASIC_SPEC.replace(
        r#"contract "second" }"#,
        r#"contract "second" verify unit "beta works" }"#,
    );
    let dir = testable_project(&spec);
    // The plan changes gamma before the alpha it depends on, names an entity
    // that does not exist, and leaves beta out.
    let plan = serde_json::json!({
        "plan_id": "p1",
        "entries": [
            { "entity_id": "gamma", "action": "modify" },
            { "entity_id": "alpha", "action": "modify" },
            { "entity_id": "ghost", "action": "add" }
        ]
    });
    let responses = mcp_session_in(
        &dir,
        &[mcp_request(
            1,
            "prompts/get",
            serde_json::json!({
                "name": "specforge://prompts/trace",
                "arguments": { "plan": plan }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let parsed = prompt_data(resp);
    // One gap per problem, each naming its endpoints, its kind, and a
    // human-readable context.
    let mut gaps: Vec<(&str, &str, &str, &str)> = parsed["coverage_gaps"]
        .as_array()
        .expect("coverage_gaps array")
        .iter()
        .map(|g| {
            (
                g["missing_link_type"].as_str().unwrap(),
                g["source_entity"].as_str().unwrap(),
                g["target_entity"].as_str().unwrap(),
                g["gap_context"].as_str().unwrap(),
            )
        })
        .collect();
    gaps.sort_unstable();
    assert_eq!(
        gaps,
        [
            (
                "missing_plan_entry",
                "plan",
                "beta",
                "testable entity 'beta' (behavior) is not covered by the plan"
            ),
            (
                "ordering",
                "gamma",
                "alpha",
                "'gamma' depends on 'alpha' (via behaviors), but 'alpha' appears later in the plan"
            ),
            (
                "unresolved_entity",
                "plan",
                "ghost",
                "E003: unresolved entity 'ghost' in plan — not found in graph"
            ),
        ],
        "{parsed}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_explore_prompt",
    verify = "specforge://prompts/explore returns exploration starting points"
)]
fn mcp_prompt_explore_returns_starting_points() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "prompts/get",
            serde_json::json!({
                "name": "specforge://prompts/explore",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let messages = resp["result"]["messages"]
        .as_array()
        .expect("prompt should return messages array");
    assert!(
        messages.len() >= 2,
        "should have instruction + data messages"
    );
    let data_msg = messages.last().unwrap();
    let text = data_msg["content"]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("data message should have content.text: {}", data_msg));
    let parsed: serde_json::Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("data message text not valid JSON: {}\ntext: {}", e, text));
    let strings = |key: &str| -> Vec<&str> {
        parsed[key]
            .as_array()
            .unwrap_or_else(|| panic!("{key} array: {parsed}"))
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect()
    };
    // gamma references two entities and nothing references it: the top of
    // the graph, the first place to start.
    let starting = strings("starting_points");
    assert_eq!(starting.first(), Some(&"gamma"), "{parsed}");
    let mut all = starting.clone();
    all.sort_unstable();
    assert_eq!(all, ["alpha", "beta", "gamma", "inv"], "{parsed}");
    // alpha and gamma carry two edges each, more than beta or inv.
    let high = strings("high_connectivity");
    let mut top_two = high[..2].to_vec();
    top_two.sort_unstable();
    assert_eq!(top_two, ["alpha", "gamma"], "{parsed}");
    assert_eq!(parsed["unconnected"], serde_json::json!([]), "{parsed}");
}

#[test]
fn mcp_prompt_unknown_returns_error() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "prompts/get",
            serde_json::json!({
                "name": "specforge://prompts/nonexistent",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(
        resp["error"].is_object(),
        "should be error for unknown prompt"
    );
    assert_eq!(resp["error"]["code"], -32602, "should be INVALID_PARAMS");
}

#[test]
fn mcp_prompt_context_missing_entity_returns_error() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "prompts/get",
            serde_json::json!({
                "name": "specforge://prompts/context",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(
        resp["error"].is_object(),
        "should be error for missing entity_id"
    );
    assert_eq!(resp["error"]["code"], -32602, "should be INVALID_PARAMS");
}

#[specforge_test(
    behavior = "provide_mcp_context_prompt",
    verify = "non-existent entity returns error"
)]
fn mcp_prompt_context_entity_not_found() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "prompts/get",
            serde_json::json!({
                "name": "specforge://prompts/context",
                "arguments": { "entity_id": "nonexistent_entity" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(
        resp["error"].is_object(),
        "should be error for nonexistent entity"
    );
    assert_eq!(resp["error"]["code"], -32602, "should be INVALID_PARAMS");
}

// ============================================================
// Phase 5: Operation Tools
// ============================================================

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "specforge.format formats spec files"
)]
fn mcp_tool_format_returns_result() {
    let dir = setup_project(&[("main.spec", UNFORMATTED_SPEC)]);
    let responses = mcp_session_in(
        &dir,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.format",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert_eq!(content["check_only"], false, "{content}");
    assert_eq!(content["all_clean"], false, "{content}");
    assert_eq!(content["total_checked"], 1, "{content}");
    let changed = content["changed_files"].as_array().expect("changed_files");
    assert_eq!(changed.len(), 1, "{content}");
    assert!(
        changed[0].as_str().unwrap().ends_with("main.spec"),
        "{content}"
    );
    // The file on disk now has the canonical layout.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("main.spec")).unwrap(),
        FORMATTED_SPEC
    );
}

/// A valid spec in a non-canonical layout, and its canonical form.
const UNFORMATTED_SPEC: &str = "behavior   alpha \"Alpha\"   {   contract \"first\"   }\n";
const FORMATTED_SPEC: &str = "behavior alpha \"Alpha\" {\n  contract \"first\"\n}\n";

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "check mode reports without modifying files"
)]
fn mcp_tool_format_check_mode() {
    let dir = setup_project(&[("main.spec", UNFORMATTED_SPEC)]);
    let responses = mcp_session_in(
        &dir,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.format",
                "arguments": { "check": true }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert_eq!(content["check_only"], true, "{content}");
    // Reports the file that needs formatting...
    assert_eq!(content["all_clean"], false, "{content}");
    let changed = content["changed_files"].as_array().expect("changed_files");
    assert_eq!(changed.len(), 1, "{content}");
    assert!(
        changed[0].as_str().unwrap().ends_with("main.spec"),
        "{content}"
    );
    // ...without touching it.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("main.spec")).unwrap(),
        UNFORMATTED_SPEC
    );
}

#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "specforge.rename renames entity and all references"
)]
fn mcp_tool_rename_returns_affected() {
    let dir = setup_project(&[("main.spec", BASIC_SPEC)]);
    let responses = mcp_session_in(
        &dir,
        &[
            mcp_request(
                1,
                "tools/call",
                serde_json::json!({
                    "name": "specforge.rename",
                    "arguments": { "entity_id": "alpha", "new_name": "alpha_renamed" }
                }),
            ),
            // The server recompiled: the new name now resolves.
            mcp_request(
                2,
                "tools/call",
                serde_json::json!({
                    "name": "specforge.find_references",
                    "arguments": { "entity_id": "alpha_renamed" }
                }),
            ),
        ],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert_eq!(content["old_name"], "alpha");
    assert_eq!(content["new_name"], "alpha_renamed");
    assert_eq!(content["affected_files"], serde_json::json!(["main.spec"]));
    // Clean: only the structural-only notice (the project enables no
    // extension).
    let codes: Vec<&str> = content["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["I002"], "the renamed project is clean: {content}");

    // The declaration and both references are rewritten on disk; nothing
    // else changes.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("main.spec")).unwrap(),
        BASIC_SPEC
            .replace("behavior alpha ", "behavior alpha_renamed ")
            .replace("[alpha, beta]", "[alpha_renamed, beta]")
            .replace("[alpha]", "[alpha_renamed]")
    );

    let refs = parse_tool_content(find_response(&responses, 2).expect("find_references"));
    let mut referrers: Vec<&str> = refs["locations"]
        .as_array()
        .expect("locations")
        .iter()
        .map(|l| l["referencing_entity_id"].as_str().unwrap())
        .collect();
    referrers.sort_unstable();
    assert_eq!(referrers, ["gamma", "inv"], "{refs}");
}

#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "non-existent entity returns error response"
)]
fn mcp_tool_rename_missing_entity_error() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.rename",
                "arguments": { "entity_id": "nonexistent_xyz", "new_name": "new_xyz" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    let error = tool_error(resp);
    assert_eq!(error["code"], "entity_not_found", "{error}");
}

#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "invalid new_name returns validation error"
)]
fn mcp_tool_rename_invalid_name_error() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.rename",
                "arguments": { "entity_id": "alpha", "new_name": "" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    let error = tool_error(resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "specforge.init creates specforge.json project"
)]
fn mcp_tool_init_returns_project() {
    let dir = tempfile::TempDir::new().unwrap();
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.init",
                "arguments": { "path": dir.path().to_str().unwrap(), "name": "fresh" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert_eq!(
        content["project_root"],
        dir.path().canonicalize().unwrap().to_str().unwrap()
    );
    assert!(dir.path().join("specforge.json").is_file());
    assert_eq!(content["config_file"], "specforge.json");
    assert_eq!(content["starter_file"], "spec/hello.spec");

    // specforge.json is on disk: the given name, default version, no
    // extensions.
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("specforge.json"))
            .expect("specforge.json written"),
    )
    .expect("specforge.json is JSON");
    // The config `specforge init` writes.
    assert_eq!(
        config,
        serde_json::json!({
            "$schema": "https://specforge.dev/schema/specforge.json",
            "name": "fresh", "version": "0.1.0", "spec_root": "spec", "extensions": []
        })
    );
    // The spec directory is scaffolded with the starter file.
    assert!(
        dir.path().join("spec/hello.spec").is_file(),
        "starter spec written"
    );
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "specforge.add_extension adds extension to config"
)]
fn mcp_tool_add_extension_returns_installed() {
    // Local .wasm install: real, offline, and verifiable.
    let blob = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures/greet-extension/greet.wasm");
    let dir = setup_project(&[("main.spec", BASIC_SPEC)]);
    let responses = mcp_session_in(
        &dir,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.add_extension",
                "arguments": { "specifier": blob.to_str().unwrap() }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert_eq!(content["extension"], "@sdk/greet");
    assert_eq!(content["action"], "installed");

    // The project's config now lists the extension (it listed none before)...
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("specforge.json")).unwrap())
            .expect("specforge.json is JSON");
    assert_eq!(
        config["extensions"],
        serde_json::json!(["@sdk/greet"]),
        "{config}"
    );
    // ...and its module is installed where the compiler loads it.
    assert!(
        dir.path()
            .join(".specforge/extensions/@sdk/greet/extension.wasm")
            .is_file(),
        "extension module installed"
    );
}

#[test]
fn mcp_tool_add_extension_invalid_specifier() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.add_extension",
                "arguments": { "specifier": "bad" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    let error = tool_error(resp);
    assert_eq!(error["diagnostic"]["code"], "E054", "{error}");
}

#[test]
fn mcp_tool_remove_extension_returns_success() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.remove_extension",
                "arguments": { "name": "@specforge/software" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    // Nothing is installed in this fixture — refusing is the honest answer.
    let error = tool_error(resp);
    assert_eq!(error["code"], "extension_not_found", "{error}");
}

#[test]
fn mcp_tool_migrate_returns_result() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.migrate",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    // from_version is the format version detected on disk.
    assert_eq!(content["from_version"], "1.0");
    assert_eq!(content["migrated"], false, "fixture is already current");
    assert!(content["message"].is_string());
}

#[test]
fn mcp_tool_extensions_returns_list() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.extensions",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert!(
        content["extensions"].is_array(),
        "extensions should have extensions array"
    );
    assert!(
        content["entity_kinds_in_graph"].is_array() || content["entity_kinds_in_graph"].is_object(),
        "extensions should have entity_kinds_in_graph: {}",
        content
    );
}

#[test]
fn mcp_tool_providers_returns_list() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.providers",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert!(
        content["providers"].is_array(),
        "providers should have providers array"
    );
}

#[test]
fn mcp_tool_doctor_returns_health() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.doctor",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    assert_eq!(content["extensions_ok"], true);
    assert!(
        content["findings"].is_array(),
        "doctor should have findings array"
    );
    assert!(
        content["cache_status"].is_string(),
        "doctor should have cache_status"
    );
}

#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "a project without a collector returns an E058 error"
)]
fn mcp_tool_collect_without_collector_errors() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.collect",
                "arguments": {}
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    let error = tool_error(resp);
    assert_eq!(
        error["diagnostic"]["code"], "E058",
        "expected E058, got: {resp}"
    );
}

#[test]
fn mcp_tool_render_returns_output() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.render",
                "arguments": { "format": "json" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    // `json` is the graph renderer's alias; the result names the renderer.
    assert_eq!(content["format"], "graph");
    assert!(
        content["output"].is_string(),
        "render should return the rendered output"
    );
}

// ============================================================
// Phase 6: Suggest Fixes + Parameter Variants
// ============================================================

#[specforge_test(
    behavior = "provide_mcp_suggest_fixes_tool",
    verify = "clean entity with no diagnostics returns empty list"
)]
fn mcp_tool_suggest_fixes_returns_array() {
    // `broken` misspells alpha (a fixable E003); beta is clean.
    let spec = format!(
        "{BASIC_SPEC}\nfeature broken \"Broken\" {{ problem \"p\" solution \"s\" behaviors [alpah] }}"
    );
    let fixes_for = |id: u64, entity: &str| {
        mcp_request(
            id,
            "tools/call",
            serde_json::json!({
                "name": "specforge.suggest_fixes",
                "arguments": { "entity_id": entity }
            }),
        )
    };
    let responses = mcp_session(&spec, &[fixes_for(1, "beta"), fixes_for(2, "broken")]);

    let content = |id: u64| {
        let resp = find_response(&responses, id).expect("suggest_fixes response");
        assert!(resp["error"].is_null(), "should not be error: {}", resp);
        parse_tool_content(resp)
    };
    assert_eq!(
        content(1),
        serde_json::json!({"fixes": []}),
        "the clean entity gets no suggestions"
    );
    // The project does have a fix to offer — just not for beta.
    let broken = content(2);
    let fixes = broken["fixes"].as_array().expect("array");
    assert_eq!(fixes.len(), 1, "{broken}");
    assert_eq!(fixes[0]["diagnostic_code"], "E003", "{broken}");
    // The LSP's title, and the edit that applies it: the token.
    assert_eq!(fixes[0]["title"], "Replace with 'alpha'", "{broken}");
    let edits = fixes[0]["edits"].as_array().expect("edits");
    assert_eq!(edits.len(), 1, "{broken}");
    assert_eq!(edits[0]["new_text"], "alpha", "{broken}");
    assert_eq!(edits[0]["file_path"], "main.spec", "{broken}");
}

#[test]
fn mcp_tool_validate_severity_filter() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.validate",
                "arguments": { "severity_filter": "error" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .expect("should have content[0].text");
    let parsed: serde_json::Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("diagnostics text not valid JSON: {}\ntext: {}", e, text));
    // All returned diagnostics (if any) should be errors
    if let Some(arr) = parsed.as_array() {
        for diag in arr {
            assert_eq!(
                diag["severity"].as_str().unwrap_or("error"),
                "error",
                "severity_filter=error should only return errors, got: {}",
                diag
            );
        }
    }
}

#[test]
fn mcp_tool_coverage_kind_filter() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.coverage",
                "arguments": { "kind": "behavior" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    let arr = content["entities"]
        .as_array()
        .expect("coverage should return array");
    for entry in arr {
        assert_eq!(
            entry["kind"].as_str().unwrap(),
            "behavior",
            "kind=behavior filter should only return behaviors, got: {}",
            entry
        );
    }
}

#[specforge_test(
    behavior = "provide_mcp_schema_tool",
    verify = "kind filter restricts schema to single entity kind"
)]
fn mcp_tool_schema_kind_filter() {
    let dir = setup_project_with_config(
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software","@specforge/product"]}"#,
        &[("main.spec", BASIC_SPEC)],
    );
    let responses = mcp_session_in(
        &dir,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.schema",
                "arguments": { "kind": "behavior" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let content = parse_tool_content(resp);
    let kinds: Vec<&str> = content["entity_kinds"]
        .as_array()
        .expect("should have an entity_kinds list")
        .iter()
        .map(|k| k["name"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["behavior"], "kind=behavior keeps only behavior");
    // Every edge type left can start or end at a behavior (or is open).
    for edge in content["edge_types"].as_array().unwrap() {
        let on = |side: &str| {
            edge[side]
                .as_array()
                .is_none_or(|k| k.iter().any(|k| k == "behavior"))
        };
        assert!(on("source_kinds") || on("target_kinds"), "{edge}");
    }
    assert!(
        content["edge_types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["label"] == "BehaviorImplementsFeature"),
        "{content}"
    );
}

#[test]
fn mcp_tool_export_format_brief() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.export",
                "arguments": { "format": "brief" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(resp["error"].is_null(), "should not be error: {}", resp);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let _content: serde_json::Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("brief export not valid JSON: {}\ntext: {}", e, text));
}

#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "limit caps the number of returned results"
)]
fn mcp_tool_search_with_limit() {
    let search = |id: u64, args: serde_json::Value| {
        mcp_request(
            id,
            "tools/call",
            serde_json::json!({ "name": "specforge.search", "arguments": args }),
        )
    };
    let responses = mcp_session(
        BASIC_SPEC,
        &[
            search(1, serde_json::json!({ "query": "a" })),
            search(2, serde_json::json!({ "query": "a", "limit": 1 })),
        ],
    );

    let ids = |id: u64| -> Vec<String> {
        let resp = find_response(&responses, id).expect("search response");
        assert!(resp["error"].is_null(), "should not be error: {}", resp);
        parse_tool_content(resp)["results"]
            .as_array()
            .expect("search should return array")
            .iter()
            .map(|r| r["entity_id"].as_str().unwrap().to_string())
            .collect()
    };
    let unlimited = ids(1);
    assert_eq!(unlimited.len(), 4, "the query matches four: {unlimited:?}");
    assert_eq!(unlimited[0], "alpha", "the prefix match ranks first");
    assert_eq!(
        ids(2),
        [unlimited[0].clone()],
        "limit=1 keeps only the best match"
    );
}

#[specforge_test(
    behavior = "provide_mcp_render_tool",
    verify = "unrecognized format returns error listing available renderers"
)]
fn mcp_tool_render_invalid_format() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.render",
                "arguments": { "format": "xyz" }
            }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    let error = tool_error(resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    // The message names the bad format and lists the renderers by their
    // export names; `data` carries the same list (the `json` alias of
    // `graph` is accepted, never listed).
    let message = error["message"].as_str().expect("message");
    assert!(
        message.starts_with("Unknown format: xyz. Expected: "),
        "{message}"
    );
    let available: Vec<&str> = error["data"]["available_renderers"]
        .as_array()
        .expect("available_renderers")
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    for core in ["graph", "dot"] {
        assert!(available.contains(&core), "{core} listed: {available:?}");
        assert!(message.contains(core), "{core} named in: {message}");
    }
    assert!(!available.contains(&"json"), "{available:?}");
    // One list, in the message and in the data.
    assert_eq!(
        message.trim_start_matches("Unknown format: xyz. Expected: "),
        available.join(", ")
    );
}

// ============================================================
// Phase 7: Lifecycle & Protocol
// ============================================================

#[test]
fn mcp_lifecycle_shutdown() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(1, "shutdown", serde_json::json!({}))],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(
        resp["result"].is_object(),
        "shutdown should return result object"
    );
    assert!(resp["error"].is_null(), "shutdown should not return error");
}

#[test]
fn mcp_lifecycle_cancel_request() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "$/cancelRequest",
            serde_json::json!({ "id": 999 }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(
        resp["result"].is_object(),
        "cancelRequest should return result object"
    );
    assert!(
        resp["error"].is_null(),
        "cancelRequest should not return error"
    );
}

#[specforge_test(
    behavior = "guard_mcp_reinitialization",
    verify = "second initialize request returns -32600 error"
)]
fn mcp_lifecycle_double_init_error() {
    // mcp_session already sent the client's initialize (id 0); a second one fails
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "initialize",
            serde_json::json!({ "projectRoot": "." }),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(
        resp["error"].is_object(),
        "double initialize should return error"
    );
    assert_eq!(resp["error"]["code"], -32600, "should be INVALID_REQUEST");
}

#[test]
fn mcp_lifecycle_notifications_initialized() {
    let responses = mcp_session(
        BASIC_SPEC,
        &[mcp_request(
            1,
            "notifications/initialized",
            serde_json::json!({}),
        )],
    );

    let resp = find_response(&responses, 1).expect("should get response for id 1");
    assert!(
        resp["result"].is_object(),
        "notifications/initialized should return result"
    );
    assert!(
        resp["error"].is_null(),
        "notifications/initialized should not return error"
    );
}

/// A stdio client subscribed to the graph gets `specforge/graphChanged` on
/// stdout once watch has rebuilt the project.
#[specforge_test(
    behavior = "notify_graph_delta_via_mcp",
    verify = "graph_changed notification sent after incremental rebuild"
)]
fn mcp_stdio_client_receives_graph_notification() {
    use std::io::{BufRead, BufReader};

    let dir = setup_project(&[("spec/base.spec", r#"behavior base "B" { contract "b" }"#)]);
    let mut child = specforge_binary()
        .args(["mcp"])
        .arg(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start specforge mcp");
    let mut stdin = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut send = move |req: String| {
        writeln!(stdin, "{req}").unwrap();
        stdin.flush().unwrap();
    };

    send(mcp_initialize(0));
    lines.next().unwrap().unwrap();
    send(mcp_request(
        1,
        "resources/subscribe",
        serde_json::json!({"uri": "specforge://graph"}),
    ));
    lines.next().unwrap().unwrap();

    // A new entity on disk: the next read brings the project up to date,
    // with no watch running.
    std::fs::write(
        dir.path().join("spec/added.spec"),
        r#"behavior added "A" { contract "a" }"#,
    )
    .unwrap();

    send(mcp_request(
        2,
        "resources/read",
        serde_json::json!({"uri": "specforge://diagnostics"}),
    ));
    drop(send);
    let out: Vec<serde_json::Value> = lines
        .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
        .collect();
    child.wait().unwrap();

    let notification = out
        .iter()
        .find(|m| m["method"] == "specforge/graphChanged")
        .unwrap_or_else(|| panic!("no graphChanged notification in {out:?}"));
    assert!(notification.get("id").is_none(), "{notification}");
    let added = notification["params"]["added_nodes"].as_array().unwrap();
    assert!(added.iter().any(|n| n == "added"), "{notification}");
}

/// A stateless (MCP 2026-07-28) client over stdio: no initialize, the
/// revision in each request's _meta.
#[test]
fn mcp_stateless_client_needs_no_initialize() {
    let dir = basic_project_with_extensions();
    let meta = serde_json::json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    let responses = mcp_raw_session_in(
        &dir,
        &[
            mcp_request(1, "server/discover", serde_json::json!({ "_meta": meta })),
            mcp_request(
                2,
                "tools/call",
                serde_json::json!({
                    "_meta": meta,
                    "name": "specforge.query",
                    "arguments": { "entity_id": "alpha" }
                }),
            ),
            mcp_request(
                3,
                "subscriptions/listen",
                serde_json::json!({
                    "_meta": meta,
                    "notifications": { "resourceSubscriptions": ["specforge://graph"] }
                }),
            ),
        ],
    );

    let discover = find_response(&responses, 1).expect("discover reply");
    assert_eq!(
        discover["result"]["supportedVersions"],
        serde_json::json!(["2026-07-28"]),
        "{discover}"
    );
    let query = find_response(&responses, 2).expect("query reply");
    assert_eq!(query["result"]["resultType"], "complete", "{query}");
    assert_eq!(
        query["result"]["structuredContent"]["nodes"][0]["id"], "alpha",
        "{query}"
    );
    // The listen stream is acknowledged; its request gets no response
    // while it is open, and the connection's end closes it.
    assert!(find_response(&responses, 3).is_none(), "{responses:?}");
    assert!(
        responses.iter().any(
            |r| r["method"] == "notifications/subscriptions/acknowledged"
                && r["params"]["_meta"]["io.modelcontextprotocol/subscriptionId"] == 3
        ),
        "{responses:?}"
    );
}
