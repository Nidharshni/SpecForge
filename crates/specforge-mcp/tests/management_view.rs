//! The management operations over MCP (architecture plan 05): the
//! extensions listing, doctor, remove, validate and stats over the served
//! project, and the inference tools without one.

use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;

use crate::fake_extension::{self, FakeExtension};
use crate::support::{TestProject, call, call_tool, files_under};
use crate::tool_errors::mcp_error;

const GREET: &str = "@sdk/greet";

/// A tool's JSON answer.
fn answer(resp: &Value) -> Value {
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text block: {resp}"));
    serde_json::from_str(text).unwrap_or_else(|e| panic!("not JSON ({e}): {resp}"))
}

/// A server initialized over a temp project whose `specforge.json` is
/// `config` as written (valid JSON or not), with one behavior; the project's
/// directory, which lives as long as the test holds it.
fn server_over_text(config: &str) -> (McpServer, tempfile::TempDir) {
    let dir = TestProject::new()
        .file("test.spec", "behavior alpha \"Alpha\" {\n}\n")
        .into_dir();
    std::fs::write(dir.path().join("specforge.json"), config).unwrap();
    let mut server = McpServer::new();
    let reply = call(
        &mut server,
        "initialize",
        json!({"projectRoot": dir.path().to_str().unwrap()}),
    );
    assert!(reply["error"].is_null(), "initialize: {reply}");
    (server, dir)
}

/// [`server_over_text`] over a config written from JSON.
fn server_over(config: Value) -> (McpServer, tempfile::TempDir) {
    server_over_text(&config.to_string())
}

fn greet_blob() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/greet-extension/greet.wasm")
}

/// A served project with `@sdk/greet` installed from its local blob: a lock
/// entry, the binary under `.specforge/extensions/` and a `specforge.json`
/// entry.
fn server_with_greet_installed() -> (McpServer, tempfile::TempDir) {
    let (mut server, root) =
        server_over(json!({"name": "t", "version": "0.1.0", "extensions": []}));
    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [greet_blob().to_str().unwrap()]}),
    );
    assert_eq!(resp["result"]["isError"], false, "install failed: {resp}");
    assert!(
        root.path()
            .join(".specforge/extensions")
            .join(GREET)
            .exists()
    );
    (server, root)
}

/// Doctor's findings, without the z3 probe's (it depends on PATH).
fn project_findings(report: &Value) -> Vec<Value> {
    report["findings"]
        .as_array()
        .unwrap_or_else(|| panic!("no findings: {report}"))
        .iter()
        .filter(|f| f["code"] != "z3_missing")
        .cloned()
        .collect()
}

/// The codes of [`project_findings`].
fn project_codes(report: &Value) -> Vec<String> {
    project_findings(report)
        .iter()
        .map(|f| f["code"].as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "provide_mcp_extensions_tool",
    verify = "specforge.extensions lists all installed extensions"
)]
fn extensions_lists_the_lock_entries_and_the_kinds_in_the_graph() {
    let config = json!({"name": "t", "version": "0.1.0", "extensions": ["@specforge/software"]});

    // No lock: no lock entries; the graph's one behavior.
    let (mut server, _root) = server_over(config.clone());
    let listed = answer(&call_tool(&mut server, "specforge.extensions", json!({})));
    assert_eq!(listed["lock_file_entries"], json!([]), "{listed}");
    assert_eq!(
        listed["entity_kinds_in_graph"],
        json!(["behavior"]),
        "{listed}"
    );

    // A lock: each entry as {name, version}, in lock order.
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    std::fs::write(
        dir.path().join("test.spec"),
        "behavior alpha \"Alpha\" {\n}\ninvariant beta \"Beta\" {\n}\n",
    )
    .unwrap();
    let lock = json!({"lockfile_version": 1, "entries": [
        {"name": "@acme/zeta", "version": "2.0.0", "source": "registry", "wasm_hash": "h1"},
        {"name": "@acme/alpha", "version": "1.0.0", "source": "registry", "wasm_hash": "h2"},
    ]});
    std::fs::write(dir.path().join("specforge.lock"), lock.to_string()).unwrap();
    let mut server = McpServer::new();
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"projectRoot": dir.path().to_str().unwrap()}});
    server.handle_message(&req.to_string());

    let listed = answer(&call_tool(&mut server, "specforge.extensions", json!({})));
    assert_eq!(
        listed["lock_file_entries"],
        json!([
            {"name": "@acme/zeta", "version": "2.0.0"},
            {"name": "@acme/alpha", "version": "1.0.0"},
        ]),
        "{listed}"
    );
    assert_eq!(
        listed["entity_kinds_in_graph"],
        json!(["behavior", "invariant"]),
        "{listed}"
    );
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "specforge stats --format json and specforge.stats answer the same document"
)]
fn stats_and_validate_count_the_served_projects_surface_conflicts() {
    let (mut server, _ext, _dir) = fake_extension::initialized(
        FakeExtension::new().with_output("mcp__check", json!({"checked": true})),
    );
    let reported = server.state().diagnostics();
    let infos = reported
        .iter()
        .filter(|d| d.severity == specforge_common::Severity::Info)
        .count();
    assert!(reported.iter().any(|d| d.code == "I017"), "{reported:?}");

    let stats = answer(&call_tool(&mut server, "specforge.stats", json!({})));
    assert_eq!(stats["diagnostic_summary"]["infos"], infos, "{stats}");

    let validated = answer(&call_tool(&mut server, "specforge.validate", json!({})));
    let codes: Vec<&str> = validated
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"I017"), "{validated}");
}

// R2 (plan 05): remove_extension with an unreadable specforge.json refuses
// before it writes anything (it used to uninstall the binary and empty the
// lock first).
#[specforge_test(
    behavior = "remove_extension",
    verify = "a removal with an unreadable specforge.json is config_invalid and changes nothing"
)]
fn remove_extension_with_an_unreadable_config_changes_nothing() {
    let (mut server, root) = server_with_greet_installed();
    std::fs::write(
        root.path().join("specforge.json"),
        r#"{ "extensions": ["@sdk/greet",  }"#,
    )
    .unwrap();
    let before = files_under(root.path());

    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"name": GREET}),
    );

    let error = mcp_error(&resp);
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("is not valid JSON"),
        "{error}"
    );
    assert_eq!(files_under(root.path()), before);
}

// One refusal for an unusable specforge.json: add_extension and
// remove_extension (MCP has no update tool) answer the same schema_mismatch
// with the reason E069 gives, and change nothing.
#[specforge_test(
    behavior = "management_operations_over_the_project_view",
    verify = "add, update and remove refuse an unusable specforge.json with one refusal, before they write"
)]
fn add_and_remove_extension_refuse_an_unusable_config_alike() {
    for config in [
        r#"{ "extensions": ["@specforge/product",  }"#,
        "[1,2]",
        r#"{"extensions": "@specforge/product"}"#,
    ] {
        let (mut server, root) = server_over_text(config);
        let before = files_under(root.path());
        let validated = answer(&call_tool(&mut server, "specforge.validate", json!({})));
        let reason = validated[0]["message"].as_str().unwrap().to_string();

        let mut refusals = Vec::new();
        for (tool, arguments) in [
            (
                "specforge.add_extension",
                json!({"specifiers": ["@specforge/product"]}),
            ),
            (
                "specforge.add_extension",
                json!({"specifiers": ["@specforge/product"], "dry_run": true}),
            ),
            (
                "specforge.remove_extension",
                json!({"name": "@specforge/product"}),
            ),
        ] {
            let resp = call_tool(&mut server, tool, arguments.clone());

            let error = mcp_error(&resp);
            assert_eq!(
                error["code"], "schema_mismatch",
                "{config}: {tool}: {error}"
            );
            assert!(
                reason.contains(error["message"].as_str().unwrap()),
                "{config}: {tool}: E069 says `{reason}`, the refusal `{error}`"
            );
            assert_eq!(
                files_under(root.path()),
                before,
                "{config}: {tool} {arguments} wrote"
            );
            refusals.push(error["message"].clone());
        }
        assert!(
            refusals.windows(2).all(|pair| pair[0] == pair[1]),
            "{refusals:?}"
        );
    }
}

/// With no project served, the infer tools are the no-project refusal
/// (precondition_failed, no argument), as the operations under them refuse a
/// view without a root.
fn assert_no_project_refusal(tool: &str) {
    let mut server = McpServer::new();
    server.handle_message(
        &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}).to_string(),
    );
    let resp = call_tool(&mut server, tool, json!({}));
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "precondition_failed", "{error}");
    assert!(error["argument"].is_null(), "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .starts_with("no project is served: start the server in a project"),
        "{error}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_infer_progress_tool",
    verify = "with no project served, infer_progress is the no-project refusal"
)]
fn infer_progress_without_a_project_is_refused() {
    assert_no_project_refusal("specforge.infer_progress");
}

#[specforge_test(
    behavior = "provide_mcp_infer_gaps_tool",
    verify = "with no project served, infer_gaps is the no-project refusal"
)]
fn infer_gaps_without_a_project_is_refused() {
    assert_no_project_refusal("specforge.infer_gaps");
}

// R3 (plan 05): an unusable specforge.json is E069 on MCP too: validate's
// verdict fails, and doctor lists it as an error finding. It used to be
// served as "no extensions configured", and doctor found nothing.
#[specforge_test(
    behavior = "provide_mcp_doctor_tool",
    verify = "specforge.doctor reports an unusable specforge.json (E069) as a finding"
)]
fn validate_and_doctor_report_an_unusable_config() {
    let (mut server, _root) = server_over_text(r#"{ "extensions": ["#);

    let resp = call_tool(&mut server, "specforge.validate", json!({}));
    let validated = answer(&resp);
    let codes: Vec<&str> = validated
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["E069", "I002"], "{validated}");
    assert_eq!(
        resp["result"]["_meta"]["specforge/check"],
        json!({"ok": false, "errors": 1, "warnings": 0, "infos": 1, "shown": 2}),
        "{resp}"
    );

    let report = answer(&call_tool(&mut server, "specforge.doctor", json!({})));
    let findings: Vec<(&str, &str)> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] != "z3_missing")
        .map(|f| (f["code"].as_str().unwrap(), f["status"].as_str().unwrap()))
        .collect();
    assert_eq!(findings, [("E069", "error"), ("I002", "info")], "{report}");
}

// A lock file that is there and cannot be read: the served project's
// environment reads it (and reloads when it changes), and doctor lists it
// as an error finding naming E033.
#[specforge_test(
    behavior = "run_doctor_check",
    verify = "a lock file that cannot be read is an error finding naming E033"
)]
fn doctor_reports_a_corrupt_lock_as_an_error_finding() {
    let (mut server, root) =
        server_over(json!({"name": "t", "version": "0.1.0", "extensions": []}));
    assert_eq!(
        project_codes(&answer(&call_tool(
            &mut server,
            "specforge.doctor",
            json!({})
        ))),
        ["I002"]
    );

    std::fs::write(root.path().join("specforge.lock"), "not valid json {{{").unwrap();

    let report = answer(&call_tool(&mut server, "specforge.doctor", json!({})));
    let findings = project_findings(&report);
    // The lock, then the I002 of a project that enables nothing.
    assert_eq!(
        project_codes(&report),
        ["lock_unreadable", "I002"],
        "{report}"
    );
    assert_eq!(findings[0]["code"], "lock_unreadable", "{report}");
    assert_eq!(findings[0]["status"], "error", "{report}");
    assert!(
        findings[0]["check"]
            .as_str()
            .unwrap()
            .contains("corrupt lock file at"),
        "{report}"
    );
    // The listing reads the same lock: it knows nothing is locked.
    let listed = answer(&call_tool(&mut server, "specforge.extensions", json!({})));
    assert_eq!(listed["lock_file_entries"], json!([]), "{listed}");
}

/// A directory with no `specforge.json` is no project, so the server serves
/// nothing there (plan 12 T8) and doctor, which acts on the served project,
/// is refused as no project. The `config_missing` finding is the ops and CLI
/// doctor's (`doctor in a directory without specforge.json ...`).
#[test]
fn doctor_over_a_directory_that_is_no_project_is_refused_as_no_project() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("main.spec"), "behavior b \"B\" {\n}\n").unwrap();
    let mut server = McpServer::with_project_root(dir.path().to_path_buf());
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}});
    server.handle_message(&req.to_string());
    assert_eq!(server.state().project_root(), None);

    let resp = call_tool(&mut server, "specforge.doctor", json!({}));

    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "precondition_failed", "{error}");
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor lists the extensions the extensions listing lists, each with its version, source and status"
)]
fn doctor_names_a_wasm_file_entry_by_its_file() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::copy(greet_blob(), dir.path().join("greet.wasm")).unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        json!({"name": "p", "version": "0.1.0",
            "extensions": ["@specforge/software", "greet.wasm"]})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("main.spec"),
        "greeting hello \"Hello\" {\n  style warm\n}\n",
    )
    .unwrap();
    let mut server = McpServer::new();
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"projectRoot": dir.path().to_str().unwrap()}});
    server.handle_message(&req.to_string());

    let report = answer(&call_tool(&mut server, "specforge.doctor", json!({})));
    let listed = answer(&call_tool(&mut server, "specforge.extensions", json!({})));

    let source_in = |doc: &Value, name: &str| {
        doc["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == name)
            .unwrap_or_else(|| panic!("{name} not listed: {doc}"))["source"]
            .clone()
    };
    assert_eq!(source_in(&report, GREET), "file:greet.wasm", "{report}");
    for extension in report["extensions"].as_array().unwrap() {
        let name = extension["name"].as_str().unwrap();
        assert_eq!(source_in(&listed, name), extension["source"], "{name}");
    }
}
