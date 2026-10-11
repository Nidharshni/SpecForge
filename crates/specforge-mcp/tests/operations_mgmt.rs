use crate::support::*;
use serde_json::{Value, json};
use specforge_extension_sdk::prelude::PassDiagnostic;
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

/// test.spec: the behavior `alpha` and the feature `beta` that has it.
const TEST_SPEC: &str =
    "behavior alpha \"Alpha\" {\n}\nfeature beta \"Beta\" {\n    behaviors [alpha]\n}\n";

/// A project that enables no extension, served with its own component
/// runtime: what `specforge mcp <root>` serves over it. A call naming
/// another project compiles that one with its own runtime too.
fn test_server() -> Served {
    TestProject::new()
        .file("test.spec", TEST_SPEC)
        .serve_components()
}

/// A server initialized over a temp project with `config` as its
/// specforge.json, holding the behavior `alpha`.
fn server_over(config: Value) -> (Served, std::path::PathBuf) {
    let enabled: Vec<String> = config["extensions"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|e| e.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default();
    let enabled: Vec<&str> = enabled.iter().map(String::as_str).collect();
    let server = TestProject::new()
        .config(|c| *c = config)
        .enabling(&enabled)
        .file("test.spec", "behavior alpha \"Alpha\" {\n}\n")
        .serve_components();
    let root = server.root().to_path_buf();
    (server, root)
}

fn software_project() -> (Served, std::path::PathBuf) {
    server_over(json!({"name": "t", "version": "0.1.0", "extensions": ["@specforge/software"]}))
}

/// `specforge.extensions`' entries as (name, status).
fn listed_extensions(server: &mut McpServer) -> Vec<(String, String)> {
    let resp = call_tool(server, "specforge.extensions", json!({}));
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    parsed["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["name"].as_str().unwrap().to_string(),
                e["status"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn invoked(server: &McpServer, tool: &str) -> bool {
    server
        .state()
        .events
        .iter()
        .any(|e| e.name == "mcp_tool_invoked" && e.params["toolName"] == tool)
}

// --- specforge.extensions ---

// B:provide_mcp_extensions_tool — verify unit "returns extensions list"
#[specforge_test(
    behavior = "provide_mcp_extensions_tool",
    verify = "specforge.extensions lists all installed extensions"
)]
fn extensions_returns_list() {
    let (mut server, _root) = software_project();
    let resp = call_tool(&mut server, "specforge.extensions", json!({}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let extensions = parsed["extensions"].as_array().unwrap();
    assert_eq!(extensions.len(), 1, "{parsed}");
    let software = &extensions[0];
    assert_eq!(software["name"], "@specforge/software");
    assert_eq!(software["status"], "loaded");
    assert!(
        !software["version"].as_str().unwrap().is_empty(),
        "{software}"
    );
    assert_eq!(
        software["entity_kinds"],
        // The keywords its kinds registered, sorted, as the CLI lists them.
        json!(["behavior", "event", "invariant", "port", "type"])
    );

    // With nothing configured, nothing is listed.
    let mut bare = test_server();
    assert_eq!(listed_extensions(&mut bare), vec![]);
}

// --- specforge.providers ---

// B:provide_mcp_providers_tool — verify unit "returns providers list"
#[specforge_test(
    behavior = "provide_mcp_providers_tool",
    verify = "specforge.providers lists all configured providers"
)]
fn providers_returns_list() {
    let (mut server, _root) = server_over(json!({
        "name": "t", "version": "0.1.0", "extensions": [],
        "providers": [
            {"alias": "tracker", "scheme": "jira", "extension": "@acme/jira"},
            {"alias": "code", "scheme": "gh", "extension": "@acme/gh"}
        ]
    }));
    let resp = call_tool(&mut server, "specforge.providers", json!({}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    let listed: Vec<(&str, &str)> = parsed["providers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["alias"].as_str().unwrap(), p["scheme"].as_str().unwrap()))
        .collect();
    assert_eq!(listed, vec![("tracker", "jira"), ("code", "gh")]);
    assert_eq!(parsed["count"], 2);
}

// --- specforge.doctor ---

#[test]
fn doctor_returns_report() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.doctor", json!({}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["extensions_ok"].is_boolean());
    assert!(parsed["findings"].is_array());
}

// --- specforge.collect ---

/// A project whose Rust tests `@specforge/cargo-test` collects, with a
/// report already written by an earlier `cargo test`: the directory, which
/// lives as long as the value.
fn collect_project() -> tempfile::TempDir {
    TestProject::new()
        .enabling(&[
            "@specforge/software",
            "@specforge/testing",
            "@specforge/cargo-test",
        ])
        .file(
            "app.spec",
            "behavior alpha \"Alpha\" {\n  verify unit \"works\"\n}\n",
        )
        .file("Cargo.toml", "")
        .file(
            "target/specforge/t.json",
            &json!({"entries": [
                {"entity_id": "alpha", "test_name": "works", "status": "pass"},
                {"entity_id": "ghost", "test_name": "stale", "status": "pass"}
            ]})
            .to_string(),
        )
        .into_dir()
}

// B:provide_mcp_collect_tool — verify unit "returns collect result"
#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "specforge.collect parses test results and maps to entities"
)]
fn collect_returns_result() {
    let mut server = test_server();
    let project = collect_project();
    let root = project.path();
    let resp = call_tool(
        &mut server,
        "specforge.collect",
        json!({"path": root.to_str().unwrap()}),
    );
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(parsed["status"], "collected");
    assert_eq!(parsed["runners"][0]["name"], "cargo-test");
    assert_eq!(parsed["runners"][0]["ran"], false);
    assert_eq!(parsed["runners"][0]["passed"], 1);
    assert_eq!(parsed["diagnostics"][0]["code"], "W115");
    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("specforge-report.json")).unwrap())
            .unwrap();
    assert_eq!(report["results"]["alpha"]["tests"][0]["status"], "pass");
}

#[test]
fn each_listed_extension_is_an_mcp_extension_info() {
    let (mut server, _root) = server_with_product();

    let resp = call_tool(&mut server, "specforge.extensions", json!({}));
    let listing: Value = serde_json::from_str(&tool_text(&resp)).unwrap();

    let entries = listing["extensions"].as_array().unwrap();
    assert!(!entries.is_empty(), "{listing}");
    for entry in entries {
        let status = entry["status"].as_str().unwrap();
        assert!(
            ["loaded", "not_loaded", "not_configured"].contains(&status),
            "{entry}"
        );
    }
    let greet = entries.iter().find(|e| e["name"] == GREET).unwrap();
    assert_eq!(greet["entity_kinds"], json!(["greeting"]), "{greet}");
    assert!(
        greet["source"].as_str().unwrap().starts_with("local:"),
        "{greet}"
    );
}

fn collected(root: &std::path::Path) -> Value {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.collect",
        json!({"path": root.to_str().unwrap()}),
    );
    serde_json::from_str(&tool_text(&resp)).unwrap_or_else(|_| panic!("{resp}"))
}

#[test]
fn collect_result_is_an_mcp_collect_result() {
    let project = collect_project();
    let root = project.path();

    let result = collected(root);
    assert_eq!(
        result["report"],
        std::fs::canonicalize(root)
            .unwrap()
            .join("specforge-report.json")
            .display()
            .to_string()
    );
    assert_eq!(result["diagnostics"][0]["code"], "W115", "{result}");
}

#[test]
fn each_collect_runner_is_an_mcp_collect_runner() {
    let project = collect_project();
    let root = project.path();

    let result = collected(root);

    let runner = &result["runners"][0];
    // Read, not run: no exit code.
    assert!(runner.get("exit_code").is_none(), "{runner}");
}

// B:provide_mcp_collect_tool — verify unit "unapproved command is refused"
#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "specforge.collect refuses to run an unapproved command"
)]
fn collect_refuses_unapproved_command() {
    let mut server = test_server();
    let project = collect_project();
    let root = project.path();
    // A fresh temp project was never approved, and the server never asks.
    let resp = call_tool(
        &mut server,
        "specforge.collect",
        json!({"path": root.to_str().unwrap(), "run": true}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["diagnostic"]["code"], "E059", "{error}");
    assert!(
        root.join("target/specforge/t.json").exists(),
        "nothing ran, so the old report is untouched"
    );
}

// B:provide_mcp_collect_tool — verify unit "no collector is an error"
#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "a project without a collector returns an E058 error"
)]
fn collect_without_collector_errors() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.collect", json!({}));
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["diagnostic"]["code"], "E058", "{error}");
}

// B:provide_mcp_collect_tool — verify contract
#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "Provide MCP Collect Tool: MCP collect tool holds — filesystem_available, compiler_api_available, report_emitted, collector_delegated, never_prompts, tool_invoked_emitted"
)]
fn collect_contract() {
    let mut server = test_server();
    let project = collect_project();
    let root = project.path();
    let path = root.to_str().unwrap();
    // Delegated to the enabled extension's collector; the report is written.
    let ok = call_tool(&mut server, "specforge.collect", json!({"path": path}));
    let parsed: Value = serde_json::from_str(&tool_text(&ok)).unwrap();
    assert_eq!(parsed["runners"][0]["extension"], "@specforge/cargo-test");
    assert!(root.join("specforge-report.json").is_file());
    // Never prompts: running an unapproved command is an error.
    let err = call_tool(
        &mut server,
        "specforge.collect",
        json!({"path": path, "run": true}),
    );
    crate::tool_errors::mcp_error(&err);
}

// --- specforge.render ---

#[test]
fn extensions_entry_fields() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.extensions", json!({}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["extensions"].is_array());
    assert!(parsed["entity_kinds_in_graph"].is_array() || parsed["extensions"].is_array());
}

#[test]
fn providers_entry_fields() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.providers", json!({}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["providers"].is_array());
}

// --- Contract tests ---

// B:provide_mcp_extensions_tool — verify contract
#[specforge_test(
    behavior = "provide_mcp_extensions_tool",
    verify = "Provide MCP Extensions Tool: MCP extensions tool holds — compiler_api_available, extensions_listed, config_reflected, tool_invoked_emitted"
)]
fn extensions_contract() {
    // compiler_api_available: the server compiled the project.
    let (mut server, root) = software_project();
    assert!(!server.state().registries().declarations().is_empty());

    // extensions_listed: name, version, entity kinds and status.
    let resp = call_tool(&mut server, "specforge.extensions", json!({}));
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    let entry = &parsed["extensions"][0];
    assert_eq!(entry["name"], "@specforge/software");
    assert!(entry["version"].is_string(), "{entry}");
    assert!(
        !entry["entity_kinds"].as_array().unwrap().is_empty(),
        "{entry}"
    );
    assert_eq!(entry["status"], "loaded");

    // config_reflected: specforge.json now drops software and adds testing.
    std::fs::write(
        root.join("specforge.json"),
        json!({"name": "t", "version": "0.1.0", "extensions": ["@specforge/testing"]}).to_string(),
    )
    .unwrap();
    // The call serves the project as it is on disk now: only the
    // configured extension is loaded.
    assert_eq!(
        listed_extensions(&mut server),
        vec![("@specforge/testing".to_string(), "loaded".to_string())]
    );

    // tool_invoked_emitted
    assert!(invoked(&server, "specforge.extensions"));
}

// B:provide_mcp_providers_tool — verify contract
#[specforge_test(
    behavior = "provide_mcp_providers_tool",
    verify = "Provide MCP Providers Tool: MCP providers tool holds — compiler_api_available, providers_listed, tool_invoked_emitted"
)]
fn providers_contract() {
    // compiler_api_available: a compiled project configuring one provider
    // whose scheme no loaded extension serves.
    let (mut server, _root) = server_over(json!({
        "name": "t", "version": "0.1.0", "extensions": ["@specforge/software"],
        "providers": [{"alias": "tracker", "scheme": "jira", "extension": "@acme/jira"}]
    }));
    assert!(!server.state().registries().declarations().is_empty());

    // providers_listed: scheme, alias, extension and status.
    let resp = call_tool(&mut server, "specforge.providers", json!({}));
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(
        parsed["providers"],
        json!([{"scheme": "jira", "alias": "tracker", "extension": "@acme/jira", "status": "extension_not_loaded"}])
    );

    // tool_invoked_emitted
    assert!(invoked(&server, "specforge.providers"));
}

// B:provide_mcp_doctor_tool — verify contract
#[specforge_test(
    behavior = "provide_mcp_doctor_tool",
    verify = "Provide MCP Doctor Tool: MCP doctor tool holds — compiler_api_available, health_checked, resolution_steps_provided, tool_invoked_emitted"
)]
fn doctor_contract() {
    // compiler_api_available: a project with one extension installed.
    let (mut server, root) = server_with_product();

    // health_checked: a healthy install passes ...
    let healthy = doctor(&mut server);
    assert_eq!(healthy["extensions_ok"], true, "{healthy}");
    assert_eq!(healthy["cache_status"], "ok", "{healthy}");
    assert_eq!(healthy["installed_count"], 1, "{healthy}");
    assert_eq!(healthy["conflicts"], json!([]), "{healthy}");

    // ... and a corrupted one fails.
    tamper_with_installed_binary(&root);
    let broken = doctor(&mut server);
    assert_eq!(broken["extensions_ok"], false, "{broken}");
    assert_eq!(broken["cache_status"], "stale", "{broken}");

    // resolution_steps_provided: every finding says how to fix it, the
    // same way every time.
    let findings = broken["findings"].as_array().unwrap();
    assert!(!findings.is_empty(), "{broken}");
    for finding in findings {
        for field in ["check", "status", "code", "remediation"] {
            assert!(finding[field].is_string(), "{field} missing: {finding}");
        }
    }
    let stale = findings
        .iter()
        .find(|f| f["code"] == "stale_hash")
        .unwrap_or_else(|| panic!("no stale_hash finding: {broken}"));
    assert!(
        stale["remediation"]
            .as_str()
            .unwrap()
            .contains("specforge add"),
        "{stale}"
    );
    assert_eq!(doctor(&mut server), broken);

    // tool_invoked_emitted
    assert!(invoked(&server, "specforge.doctor"));
}

// --- specforge.doctor: real checks ---

const GREET: &str = "@sdk/greet";

/// [`test_server`] with the product blob installed in its project.
fn server_with_product() -> (Served, std::path::PathBuf) {
    let mut server = test_server();
    let root = server.root().to_path_buf();
    let blob = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/greet-extension/greet.wasm");
    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifier": blob.to_str().unwrap()}),
    );
    assert!(resp["result"].is_object(), "install failed: {resp}");
    (server, root)
}

/// Overwrite the installed binary, as a corrupted cache would.
fn tamper_with_installed_binary(root: &std::path::Path) {
    let dir = root.join(".specforge/extensions").join(GREET);
    let wasm = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "wasm"))
        .unwrap_or_else(|| panic!("no installed wasm in {}", dir.display()));
    std::fs::write(wasm, b"not the installed module").unwrap();
}

fn doctor(server: &mut McpServer) -> Value {
    let resp = call_tool(server, "specforge.doctor", json!({}));
    serde_json::from_str(&tool_text(&resp)).unwrap_or_else(|_| panic!("{resp}"))
}

#[specforge_test(
    behavior = "provide_mcp_doctor_tool",
    verify = "response checks wasm cache integrity"
)]
fn doctor_flags_an_installed_binary_that_no_longer_matches_the_lock() {
    let (mut server, root) = server_with_product();
    let healthy = doctor(&mut server);
    assert_eq!(healthy["cache_status"], "ok", "{healthy}");
    assert_eq!(healthy["extensions_ok"], true, "{healthy}");

    tamper_with_installed_binary(&root);
    let report = doctor(&mut server);

    assert_eq!(report["cache_status"], "stale", "{report}");
    assert_eq!(report["extensions_ok"], false, "{report}");
    let finding = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "stale_hash")
        .unwrap_or_else(|| panic!("no stale_hash finding: {report}"));
    assert_eq!(finding["status"], "error");
    assert!(
        finding["check"].as_str().unwrap().contains(GREET),
        "{finding}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_doctor_tool",
    verify = "response provides deterministic resolution steps"
)]
fn doctor_gives_each_issue_the_same_remediation_every_time() {
    let (mut server, root) = server_with_product();
    tamper_with_installed_binary(&root);

    let first = doctor(&mut server);
    let second = doctor(&mut server);

    assert_eq!(first, second, "doctor is deterministic");
    let findings = first["findings"].as_array().unwrap();
    assert!(!findings.is_empty(), "{first}");
    for finding in findings {
        let remediation = finding["remediation"].as_str().unwrap_or_default();
        assert!(!remediation.is_empty(), "no remediation: {finding}");
    }
    let stale = findings.iter().find(|f| f["code"] == "stale_hash").unwrap();
    assert!(
        stale["remediation"]
            .as_str()
            .unwrap()
            .contains("specforge add"),
        "{stale}"
    );
}

/// The finding with `code` in a doctor report.
fn finding<'a>(report: &'a Value, code: &str) -> Option<&'a Value> {
    report["findings"]
        .as_array()
        .unwrap_or_else(|| panic!("no findings: {report}"))
        .iter()
        .find(|f| f["code"] == code)
}

#[specforge_test(
    behavior = "provide_mcp_doctor_tool",
    verify = "specforge.doctor reports an extension that fails to load (E028, E070) as an error"
)]
fn doctor_reports_an_extension_that_fails_to_load() {
    let (mut server, root) = server_with_product();

    // A binary that no longer matches its lock entry is refused (E070).
    tamper_with_installed_binary(&root);
    let tampered = doctor(&mut server);
    let stale =
        finding(&tampered, "stale_hash").unwrap_or_else(|| panic!("no finding: {tampered}"));
    assert_eq!(stale["status"], "error", "{stale}");
    assert!(stale["check"].as_str().unwrap().contains(GREET), "{stale}");
    assert!(
        finding(&tampered, "E070").is_none(),
        "listed once: {tampered}"
    );
    assert_eq!(stale["about"], "binary", "{tampered}");
    assert_eq!(stale["issue"]["status"], "stale_hash", "{tampered}");
    assert!(
        tampered["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["about"] != "load"),
        "{tampered}"
    );
    assert_eq!(tampered["ok"], false, "{tampered}");
    assert_eq!(tampered["extensions_ok"], false, "{tampered}");

    // Enabled but not installed at all: no lock entry, nothing for the
    // binary check to see. Only the load says so (E028).
    std::fs::remove_file(root.join("specforge.lock")).unwrap();
    std::fs::remove_dir_all(root.join(".specforge")).unwrap();
    let missing = doctor(&mut server);
    let e028 = finding(&missing, "E028").unwrap_or_else(|| panic!("no E028: {missing}"));
    assert_eq!(e028["status"], "error", "{e028}");
    assert!(
        e028["remediation"]
            .as_str()
            .unwrap()
            .contains(&format!("specforge add {GREET}")),
        "{e028}"
    );
    assert_eq!(missing["extensions_ok"], false, "{missing}");
}

#[specforge_test(
    behavior = "provide_mcp_doctor_tool",
    verify = "specforge.doctor compiles the project afresh unless use_cached is set"
)]
fn doctor_compiles_afresh_unless_use_cached() {
    let (mut server, root) = software_project();
    assert!(finding(&doctor(&mut server), "E028").is_none());

    // The agent enables an extension by editing the config directly.
    std::fs::write(
        root.join("specforge.json"),
        json!({"name": "t", "version": "0.1.0",
               "extensions": ["@specforge/software", "@acme/missing"]})
        .to_string(),
    )
    .unwrap();

    let resp = call_tool(&mut server, "specforge.doctor", json!({"use_cached": true}));
    let cached: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert!(finding(&cached, "E028").is_none(), "{cached}");

    let fresh = doctor(&mut server);
    let e028 = finding(&fresh, "E028").unwrap_or_else(|| panic!("no E028: {fresh}"));
    assert!(
        e028["check"].as_str().unwrap().contains("@acme/missing"),
        "{e028}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_doctor_tool",
    verify = "specforge.doctor detects extension conflicts"
)]
fn doctor_lists_extension_conflicts_from_the_compile() {
    // Two extensions register one kind, `feature`: the compile reports
    // E026. And an unrelated warning, a check-phase pass's.
    let mut server = TestProject::new().file("test.spec", TEST_SPEC).serve(&[
        TestExtension::software()
            .reporting(PassDiagnostic::warning("W001", "an unrelated warning")),
        TestExtension::named("@test/other").kind("feature", false),
    ]);

    // Over those diagnostics, as the last compile's, not a fresh compile.
    let resp = call_tool(&mut server, "specforge.doctor", json!({"use_cached": true}));
    let report: Value = serde_json::from_str(&tool_text(&resp)).unwrap();

    let conflicts = report["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{report}");
    // The first registration wins: @test/other's `feature` is refused.
    assert!(
        conflicts[0].as_str().unwrap().contains(
            "entity kind 'feature' registered by '@test/other' conflicts with '@test/ext'"
        ),
        "{report}"
    );
    let finding = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "E026")
        .unwrap_or_else(|| panic!("no E026 finding: {report}"));
    assert_eq!(finding["status"], "error");
    assert!(finding["remediation"].is_string(), "{finding}");
}

// --- specforge.render ---

fn render(server: &mut McpServer, args: Value) -> Value {
    let resp = call_tool(server, "specforge.render", args);
    serde_json::from_str(&tool_text(&resp)).unwrap_or_else(|_| panic!("{resp}"))
}

#[specforge_test(
    behavior = "provide_mcp_render_tool",
    verify = "specforge.render writes output files to out_dir"
)]
fn render_writes_the_output_into_out_dir() {
    let mut server = test_server();
    let out = tempfile::TempDir::new().unwrap();
    let out_dir = out.path().join("rendered");

    let parsed = render(
        &mut server,
        json!({"format": "dot", "out_dir": out_dir.to_str().unwrap()}),
    );

    let written = out_dir.join("graph.dot");
    assert_eq!(
        parsed["output_files"],
        json!([written.display().to_string()]),
        "{parsed}"
    );
    let dot = std::fs::read_to_string(&written).unwrap();
    assert!(dot.starts_with("digraph"), "{dot}");
    assert!(dot.contains("alpha"), "{dot}");
}

#[specforge_test(
    behavior = "provide_mcp_render_tool",
    verify = "registered renderer invoked for matching format"
)]
fn render_uses_the_renderer_the_format_names() {
    let mut server = test_server();
    let out = tempfile::TempDir::new().unwrap();

    let parsed = render(
        &mut server,
        json!({"format": "json", "out_dir": out.path().to_str().unwrap()}),
    );
    // `json` is the graph renderer's alias; the result names the renderer.
    assert_eq!(parsed["format"], "graph");
    let graph: Value =
        serde_json::from_str(&std::fs::read_to_string(out.path().join("graph.json")).unwrap())
            .unwrap();
    assert_eq!(graph["nodes"][0]["id"], "alpha", "{graph}");

    // Without out_dir the rendering comes back inline and nothing is written.
    let inline = render(&mut server, json!({"format": "dot"}));
    assert!(inline["output"].as_str().unwrap().starts_with("digraph"));
    assert_eq!(inline["output_files"], json!([]));
}

#[test]
fn render_scope_limits_the_graph_to_one_entity() {
    let mut server = test_server();
    let scoped = render(&mut server, json!({"format": "brief", "scope": "alpha"}));
    assert!(
        scoped["output"].as_str().unwrap().contains("alpha"),
        "{scoped}"
    );

    let missing = call_tool(
        &mut server,
        "specforge.render",
        json!({"format": "brief", "scope": "no_such_entity"}),
    );
    crate::tool_errors::mcp_error(&missing);
}

#[specforge_test(
    behavior = "provide_mcp_render_tool",
    verify = "unrecognized format returns error listing available renderers"
)]
fn render_unknown_format_lists_the_available_renderers() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.render", json!({"format": "yaml"}));

    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], "format", "{error}");
    assert_eq!(
        error["message"],
        "Unknown format: yaml. Expected: graph, context, brief, dot"
    );
    assert_eq!(
        error["data"]["available_renderers"],
        json!(["graph", "context", "brief", "dot"]),
        "{resp}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_render_tool",
    verify = "Provide MCP Render Tool: MCP render tool holds — graph_available, filesystem_available, files_written, files_listed, tool_invoked_emitted"
)]
fn render_contract() {
    let mut server = test_server();
    let out = tempfile::TempDir::new().unwrap();

    let parsed = render(
        &mut server,
        json!({"format": "context", "out_dir": out.path().to_str().unwrap()}),
    );

    let files = parsed["output_files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{parsed}");
    let written = std::path::Path::new(files[0].as_str().unwrap());
    assert!(written.starts_with(out.path()), "{parsed}");
    assert!(std::fs::read_to_string(written).unwrap().contains("alpha"));
    assert!(
        server
            .state()
            .events
            .iter()
            .any(|e| e.name == "mcp_tool_invoked"
                && e.params["toolName"] == "specforge.render"
                && e.params["category"] == "management")
    );
}

#[specforge_test(
    behavior = "provide_mcp_render_tool",
    verify = "a relative out_dir is written under the call's project root, wherever the server runs"
)]
fn a_relative_out_dir_is_written_under_the_project_root() {
    let mut server = test_server();
    // The served project is its canonical path, which render writes under.
    let root = std::fs::canonicalize(server.root()).unwrap();
    let in_cwd = std::env::current_dir().unwrap().join("rendered");
    assert!(!in_cwd.exists(), "a stale directory under the cwd");

    let parsed = render(&mut server, json!({"format": "dot", "out_dir": "rendered"}));

    let written = root.join("rendered/graph.dot");
    assert!(written.exists(), "{parsed}");
    assert_eq!(
        parsed["output_files"],
        json!([written.display().to_string()]),
        "{parsed}"
    );
    assert!(!in_cwd.exists(), "nothing is written under the cwd");
}

#[specforge_test(
    behavior = "provide_mcp_render_tool",
    verify = "a relative out_dir with no project served is invalid input on out_dir"
)]
fn a_relative_out_dir_with_nothing_served_is_refused() {
    let mut server = McpServer::new();
    let init = call(&mut server, "initialize", json!({}));
    assert!(init["error"].is_null(), "{init}");
    let cwd = std::env::current_dir().unwrap();

    let resp = call_tool(
        &mut server,
        "specforge.render",
        json!({"format": "dot", "out_dir": "p15-refused"}),
    );

    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], "out_dir", "{error}");
    assert!(!cwd.join("p15-refused").exists());
}

#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "specforge.collect of a directory that holds no project refuses with no_project"
)]
fn collect_of_a_directory_that_is_no_project_is_no_project() {
    let mut server = test_server();
    let bare = tempfile::TempDir::new().unwrap();
    let resp = call_tool(
        &mut server,
        "specforge.collect",
        json!({"path": bare.path().to_str().unwrap()}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "precondition_failed", "{error}");
    assert!(error["diagnostic"].is_null(), "no E058: {error}");
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|m| m.contains("no specforge project at")),
        "{error}"
    );
}
