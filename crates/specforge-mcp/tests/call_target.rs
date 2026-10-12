//! Which project an MCP call acts on, and whether it is current with disk
//! (architecture plan 01): a call's optional `path` and its tool's target
//! decide it before the handler runs.

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_mcp::target::TargetSpec;
use specforge_test::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

fn initialize(server: &mut McpServer, root: &Path) {
    let resp = call(
        server,
        "initialize",
        json!({"projectRoot": root.to_str().unwrap()}),
    );
    assert!(resp["error"].is_null(), "{resp}");
}

/// A server that serves no project: initialize names none and there is no
/// default root.
fn serving_nothing() -> McpServer {
    let mut server = McpServer::new();
    let resp = call(&mut server, "initialize", json!({}));
    assert!(resp["error"].is_null(), "{resp}");
    server
}

/// A project with `specforge.json` listing `extensions` and one spec file.
fn project(extensions: &[&str], spec: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    write_config(dir.path(), extensions);
    fs::write(dir.path().join("main.spec"), spec).unwrap();
    dir
}

fn write_config(root: &Path, extensions: &[&str]) {
    let config = json!({"name": "served", "version": "0.1.0", "extensions": extensions});
    fs::write(root.join("specforge.json"), config.to_string()).unwrap();
    specforge_installed::testing::install_configured(root, &specforge_project::builtins());
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The diagnostic codes a `specforge.validate` result reports.
fn validate_codes(server: &mut McpServer, arguments: Value) -> Vec<String> {
    let resp = call_tool(server, "specforge.validate", arguments);
    let diagnostics: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    diagnostics
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap().to_string())
        .collect()
}

/// Whether `specforge.query` finds `id` in the graph the server serves.
fn finds(server: &mut McpServer, id: &str) -> bool {
    let resp = call_tool(server, "specforge.query", json!({"entity_id": id}));
    let payload: Value = resp["result"]["content"][0]["text"]
        .as_str()
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or(Value::Null);
    payload["nodes"]
        .as_array()
        .is_some_and(|nodes| nodes.iter().any(|n| n["id"] == id))
}

/// The root the server serves, canonical.
fn served_root(server: &McpServer) -> Option<PathBuf> {
    server.state().project_root().map(canonical)
}

/// A project whose Rust tests `@specforge/cargo-test` collects, with the
/// report an earlier `cargo test` wrote.
fn collect_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let config = json!({
        "name": "c",
        "version": "0.1.0",
        "extensions": ["@specforge/software", "@specforge/testing", "@specforge/cargo-test"]
    });
    fs::write(root.join("specforge.json"), config.to_string()).unwrap();
    fs::write(
        root.join("app.spec"),
        "behavior alpha \"Alpha\" {\n  verify unit \"works\"\n}\n",
    )
    .unwrap();
    fs::write(root.join("Cargo.toml"), "").unwrap();
    fs::create_dir_all(root.join("target/specforge")).unwrap();
    fs::write(
        root.join("target/specforge/t.json"),
        json!({"entries": [{"entity_id": "alpha", "test_name": "works", "status": "pass"}]})
            .to_string(),
    )
    .unwrap();
    dir
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a path while no project is served serves that project, for every tool that takes a path"
)]
fn validate_with_a_path_while_nothing_is_served_serves_it() {
    let other = project(&[], "behavior adopted \"Adopted\" {\n}\n");
    let mut server = serving_nothing();
    assert_eq!(served_root(&server), None);

    let resp = call_tool(
        &mut server,
        "specforge.validate",
        json!({"path": other.path().to_str().unwrap()}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");

    assert!(finds(&mut server, "adopted"));
    assert_eq!(served_root(&server), Some(canonical(other.path())));
}

#[test]
fn collect_with_another_path_keeps_serving_this_one() {
    let served = project(&[], "behavior login \"Login\" {\n}\n");
    let other = collect_project();
    let mut server = McpServer::new();
    initialize(&mut server, served.path());

    let resp = call_tool(
        &mut server,
        "specforge.collect",
        json!({"path": other.path().to_str().unwrap()}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    assert!(other.path().join("specforge-report.json").is_file());

    assert_eq!(served_root(&server), Some(canonical(served.path())));
    assert!(server.state().graph().node("login").is_some());
    assert!(server.state().graph().node("alpha").is_none());
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation on another project does not reload the served one"
)]
fn format_with_another_path_leaves_the_served_spans() {
    // Blank lines inside each block: formatting removes them.
    let unformatted = "behavior login \"Login\" {\n\n\n\n}\n";
    let served = project(&[], unformatted);
    let other = project(&[], "behavior other \"Other\" {\n\n\n\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, served.path());
    let span = |server: &McpServer| {
        server
            .state()
            .graph()
            .node("login")
            .map(|n| n.source_span.clone())
    };
    let before = span(&server);

    let resp = call_tool(
        &mut server,
        "specforge.format",
        json!({"path": other.path().to_str().unwrap()}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    let formatted = fs::read_to_string(other.path().join("main.spec")).unwrap();
    assert_ne!(
        formatted.lines().count(),
        5,
        "format rewrote the other project"
    );

    assert_eq!(
        fs::read_to_string(served.path().join("main.spec")).unwrap(),
        unformatted
    );
    assert_eq!(span(&server), before);
    assert!(server.state().graph().node("other").is_none());
}

#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "validate with use_cached=true returns existing diagnostics without recompilation"
)]
fn use_cached_serves_the_last_compile_when_diagnostics_exist() {
    // No extension: the compile reports I002, so the served project has
    // diagnostics to serve.
    let dir = project(&[], "behavior login \"Login\" {\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());
    assert!(validate_codes(&mut server, json!({})).contains(&"I002".to_string()));

    // A dangling reference written since: a compile reports E003.
    fs::write(
        dir.path().join("main.spec"),
        "behavior login \"Login\" {\n  invariants [missing_one]\n}\n",
    )
    .unwrap();
    let cached = validate_codes(&mut server, json!({"use_cached": true}));
    assert!(!cached.contains(&"E003".to_string()), "{cached:?}");

    let fresh = validate_codes(&mut server, json!({}));
    assert!(fresh.contains(&"E003".to_string()), "{fresh:?}");
}

#[specforge_test(
    behavior = "provide_mcp_doctor_tool",
    verify = "specforge.doctor compiles the project afresh unless use_cached is set"
)]
fn doctor_use_cached_reports_the_last_compile() {
    let dir = project(&[], "behavior login \"Login\" {\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());

    // An extension enabled since, which is not installed: E028.
    write_config(dir.path(), &["@acme/missing"]);
    let load_failures = |server: &mut McpServer, arguments: Value| -> Vec<Value> {
        let resp = call_tool(server, "specforge.doctor", arguments);
        let report: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["about"] == "load")
            .cloned()
            .collect()
    };
    let cached = load_failures(&mut server, json!({"use_cached": true}));
    assert!(cached.is_empty(), "{cached:?}");

    let fresh = load_failures(&mut server, json!({}));
    assert!(fresh.iter().any(|f| f["code"] == "E028"), "{fresh:?}");
}

#[test]
fn extension_tool_without_a_project_is_refused() {
    // Serving nothing, no extension tool is listed: a call names an
    // unknown tool.
    let mut server = serving_nothing();
    assert!(server.state().surfaces().tools().is_empty());
    let resp = call_tool(&mut server, "specforge.cmds.check", json!({}));
    assert_eq!(resp["error"]["code"], -32602, "{resp}");
}

fn tool_names(server: &mut McpServer) -> Vec<String> {
    let resp = call(server, "tools/list", json!({}));
    resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a tool call serves files written since the last call, without watch"
)]
fn a_tool_call_serves_files_written_since_the_last_call() {
    let dir = project(&[], "behavior login \"Login\" {\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());
    assert!(finds(&mut server, "login"));

    // Written after initialize, with no watch running.
    fs::write(
        dir.path().join("added.spec"),
        "feature fresh \"Fresh\" {\n  behaviors [login]\n}\n",
    )
    .unwrap();
    assert!(
        finds(&mut server, "fresh"),
        "the call serves what is on disk"
    );

    // A file deleted since: its entities go.
    fs::remove_file(dir.path().join("added.spec")).unwrap();
    assert!(!finds(&mut server, "fresh"), "a deleted file's entities go");
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a prompt reads the project as it is on disk"
)]
fn a_prompt_reads_the_project_as_it_is_on_disk() {
    let dir = project(&[], "behavior login \"Login\" {\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());

    fs::write(
        dir.path().join("added.spec"),
        "behavior fresh \"Fresh\" {\n}\n",
    )
    .unwrap();
    let resp = call(
        &mut server,
        "prompts/get",
        json!({"name": "specforge://prompts/context", "arguments": {"entity_id": "fresh"}}),
    );
    assert!(resp["error"].is_null(), "{resp}");
    assert!(resp["result"]["messages"].is_array(), "{resp}");
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a resource read serves files written since the last call, without watch"
)]
fn a_resource_read_serves_files_written_since_the_last_call() {
    let dir = project(&[], "behavior login \"Login\" {\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());

    fs::write(
        dir.path().join("added.spec"),
        "behavior fresh \"Fresh\" {\n}\n",
    )
    .unwrap();
    let resp = call(
        &mut server,
        "resources/read",
        json!({"uri": "specforge://graph/fresh"}),
    );
    assert!(resp["error"].is_null(), "{resp}");
    let text = resp["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(text.contains("\"fresh\""), "{text}");
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "an environment change on disk updates the extension tools listed"
)]
fn an_environment_change_on_disk_updates_the_extension_tools_listed() {
    use crate::fake_extension::{self, EXT, FakeExtension};

    let ext = std::sync::Arc::new(FakeExtension::new());
    let dir = project(&[], "");
    let mut server = fake_extension::server_with(&ext);
    initialize(&mut server, dir.path());
    let before = tool_names(&mut server);
    assert!(
        !before.iter().any(|t| t.starts_with("specforge.cmds.")),
        "{before:?}"
    );

    // The project enables the extension: listing the tools alone sees it.
    write_config(dir.path(), &[EXT]);
    let after = tool_names(&mut server);
    for tool in ["specforge.cmds.check", "specforge.cmds.report"] {
        assert!(
            after.iter().any(|t| t == tool),
            "{tool} is not listed after the change: {after:?}"
        );
    }
    // And the auto-promoted tool dispatches: the reload built its entry,
    // not only its descriptor.
    let entry = server.state().surfaces().tool("specforge.cmds.report");
    assert!(
        matches!(
            entry.map(|e| &e.kind),
            Some(specforge_mcp::surface_table::ToolKind::Command(_))
        ),
        "{entry:?}"
    );

    // Listing again changes nothing: each tool is listed once.
    let again = tool_names(&mut server);
    let reports = again
        .iter()
        .filter(|t| *t == "specforge.cmds.report")
        .count();
    assert_eq!(reports, 1, "{again:?}");
}

#[specforge_test(
    invariant = "mcp_tool_idempotency",
    verify = "read-only tools return equivalent results for identical inputs"
)]
fn an_unchanged_project_is_not_rebuilt_by_a_call() {
    let dir = project(&[], "behavior login \"Login\" {\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());
    let generation = server.state().session_generation();

    let first = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "login"}),
    );
    let second = call_tool(
        &mut server,
        "specforge.query",
        json!({"entity_id": "login"}),
    );
    assert_eq!(first, second);
    assert_eq!(
        server.state().session_generation(),
        generation,
        "nothing changed on disk: nothing was rebuilt"
    );
}

/// What each tool that takes a `path` is called with to act on a project
/// with `behavior alpha` (its path added by the caller).
fn path_tool_calls() -> Vec<(&'static str, Value)> {
    vec![
        ("specforge.validate", json!({})),
        ("specforge.analyze", json!({"pass": "contracts"})),
        ("specforge.collect", json!({})),
        ("specforge.format", json!({"check": true})),
        (
            "specforge.rename",
            json!({"entity_id": "alpha", "new_name": "gamma"}),
        ),
        (
            "specforge.add_extension",
            json!({"specifiers": ["@specforge/software"], "dry_run": true}),
        ),
        (
            "specforge.remove_extension",
            json!({"name": "@specforge/software", "dry_run": true}),
        ),
        ("specforge.migrate", json!({"dry_run": true})),
    ]
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a path while no project is served serves that project, for every tool that takes a path"
)]
fn a_path_while_nothing_is_served_serves_it_for_every_tool() {
    // Every core tool whose schema takes a project path is covered, init
    // (which creates its project) below.
    let mut covered: Vec<&str> = path_tool_calls().iter().map(|(name, _)| *name).collect();
    covered.push("specforge.init");
    covered.sort_unstable();
    let mut with_path: Vec<&str> = specforge_mcp::tools::CORE_TOOLS
        .iter()
        .filter(|tool| tool.input_schema()["properties"].get("path").is_some())
        .map(|tool| tool.name())
        .collect();
    with_path.sort_unstable();
    assert_eq!(covered, with_path);

    for (tool, mut arguments) in path_tool_calls() {
        let other = project(&["@specforge/software"], "behavior alpha \"Alpha\" {\n}\n");
        arguments["path"] = Value::from(other.path().to_str().unwrap());
        let mut server = serving_nothing();

        call_tool(&mut server, tool, arguments);

        assert_eq!(
            served_root(&server),
            Some(canonical(other.path())),
            "{tool} did not serve the project its path names"
        );
        if tool == "specforge.rename" {
            // R1b: the rename acted on the project the path names.
            assert!(server.state().graph().node("gamma").is_some(), "{tool}");
            let text = fs::read_to_string(other.path().join("main.spec")).unwrap();
            assert!(text.contains("behavior gamma"), "{text}");
        } else {
            assert!(server.state().graph().node("alpha").is_some(), "{tool}");
        }
    }

    // init with nothing served serves the project it created.
    let parent = TempDir::new().unwrap();
    let created = parent.path().join("created");
    let mut server = serving_nothing();
    let resp = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": created.to_str().unwrap(), "name": "created"}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    assert_eq!(served_root(&server), Some(canonical(&created)));
}

#[specforge_test(
    invariant = "mcp_structured_error_responses",
    verify = "a path that does not exist is a file_not_found error on path"
)]
fn a_path_that_does_not_exist_is_file_not_found() {
    let served = project(&[], "behavior login \"Login\" {\n}\n");
    let missing = served.path().join("no/such/dir");
    for (tool, mut arguments) in path_tool_calls() {
        let mut server = McpServer::new();
        initialize(&mut server, served.path());
        arguments["path"] = Value::from(missing.to_str().unwrap());

        let resp = call_tool(&mut server, tool, arguments);

        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["code"], "file_not_found", "{tool}: {error}");
        assert_eq!(error["argument"], "path", "{tool}: {error}");
        assert_eq!(error["tool"], tool, "{error}");
    }
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation on another project does not reload the served one"
)]
fn a_mutation_on_another_project_does_not_reload_the_served_one() {
    let served = project(&[], "behavior login \"Login\" {\n}\n");
    let calls: Vec<(&str, Value)> = vec![
        (
            "specforge.rename",
            json!({"entity_id": "alpha", "new_name": "omega"}),
        ),
        ("specforge.format", json!({})),
        (
            "specforge.add_extension",
            json!({"specifiers": ["@specforge/software"]}),
        ),
        (
            "specforge.remove_extension",
            json!({"name": "@specforge/software"}),
        ),
        ("specforge.migrate", json!({})),
    ];
    for (tool, mut arguments) in calls {
        let other = project(
            &["@specforge/software"],
            "behavior alpha \"Alpha\" {\n\n\n}\n",
        );
        let mut server = McpServer::new();
        initialize(&mut server, served.path());
        // An edit the served project has not seen: reloading it would
        // serve it.
        fs::write(
            served.path().join("added.spec"),
            "behavior unseen \"U\" {\n}\n",
        )
        .unwrap();
        let generation = server.state().session_generation();
        arguments["path"] = Value::from(other.path().to_str().unwrap());

        let resp = call_tool(&mut server, tool, arguments);
        assert!(resp["error"].is_null(), "{tool}: {resp}");

        assert_eq!(
            server.state().session_generation(),
            generation,
            "{tool} reloaded the served project"
        );
        assert!(server.state().graph().node("unseen").is_none(), "{tool}");
        assert_eq!(served_root(&server), Some(canonical(served.path())));
        fs::remove_file(served.path().join("added.spec")).unwrap();
    }

    // R6: init of a new project elsewhere.
    let mut server = McpServer::new();
    initialize(&mut server, served.path());
    fs::write(
        served.path().join("added.spec"),
        "behavior unseen \"U\" {\n}\n",
    )
    .unwrap();
    let generation = server.state().session_generation();
    let elsewhere = TempDir::new().unwrap();
    let dir = elsewhere.path().join("fresh");
    let resp = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": dir.to_str().unwrap(), "name": "fresh"}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    assert_eq!(server.state().session_generation(), generation);
    assert!(server.state().graph().node("unseen").is_none());
}

/// A tool of the served project only takes no path to another project.
#[test]
fn a_tool_of_the_served_project_refuses_another_projects_path() {
    let served = project(&[], "behavior login \"Login\" {\n}\n");
    let other = project(&[], "behavior other \"Other\" {\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, served.path());

    let resp = call_tool(
        &mut server,
        "specforge.doctor",
        json!({"path": other.path().to_str().unwrap()}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], "path", "{error}");

    // Its own project's path is the served project.
    let resp = call_tool(
        &mut server,
        "specforge.doctor",
        json!({"path": served.path().to_str().unwrap()}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a path inside the served project names the served project"
)]
fn a_path_inside_the_served_project_names_the_served_project() {
    let dir = TempDir::new().unwrap();
    let config = json!({"name": "a", "version": "0.1.0", "spec_root": "spec",
        "extensions": ["@specforge/software"]});
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    fs::create_dir_all(dir.path().join("spec")).unwrap();
    fs::write(
        dir.path().join("spec/main.spec"),
        "behavior login \"Login\" {\n  category command\n  invariants [missing]\n}\n",
    )
    .unwrap();
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());
    let generation = server.state().session_generation();

    let inside = validate_codes(
        &mut server,
        json!({"path": dir.path().join("spec").to_str().unwrap()}),
    );
    let served = validate_codes(&mut server, json!({}));
    assert_eq!(inside, served);
    // Compiled as a project of its own, spec/ would load no extension.
    assert!(!inside.contains(&"I002".to_string()), "{inside:?}");
    assert!(inside.contains(&"E003".to_string()), "{inside:?}");
    assert_eq!(server.state().session_generation(), generation);
}

#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "validate with use_cached=true returns existing diagnostics without recompilation"
)]
fn use_cached_is_honoured_when_the_project_has_no_diagnostics() {
    let dir = project(&["@specforge/software"], "");
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());
    assert!(validate_codes(&mut server, json!({"use_cached": true})).is_empty());

    // R5: a project with no diagnostics was recompiled whatever use_cached
    // said.
    fs::write(
        dir.path().join("added.spec"),
        "behavior added \"Added\" {\n  category command\n  invariants [nope]\n}\n",
    )
    .unwrap();
    let cached = validate_codes(&mut server, json!({"use_cached": true}));
    assert!(cached.is_empty(), "{cached:?}");
    assert!(server.state().graph().node("added").is_none());

    let fresh = validate_codes(&mut server, json!({}));
    assert!(fresh.contains(&"E003".to_string()), "{fresh:?}");
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "analyzing another project leaves the served project untouched"
)]
fn analyze_of_another_project_runs_its_extensions_in_one_runtime() {
    use crate::fake_extension::{self, EXT, FakeExtension};

    let ext = FakeExtension::new().with_passes(&["audit"]).with_output(
        "__pass_audit",
        json!({"diagnostics": [{"code": "W900", "severity": "Warning", "message": "careful"}]}),
    );
    let (mut server, ext, _served) = fake_extension::initialized(ext);
    let generation = server.state().session_generation();
    let other = project(&[EXT], "");
    let loads = ext.handshakes();

    let resp = call_tool(
        &mut server,
        "specforge.analyze",
        json!({"path": other.path().to_str().unwrap(), "pass": "@test/cmds:audit"}),
    );
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(
        parsed["passes"][0]["findings"][0]["code"], "W900",
        "{parsed}"
    );

    // The other project's environment loaded once, and its pass ran in the
    // runtime it loaded in; the served project was not touched.
    assert_eq!(ext.handshakes() - loads, 1);
    assert!(
        ext.calls()
            .iter()
            .any(|(_, export, _)| export == "__pass_audit"),
        "{:?}",
        ext.calls()
    );
    assert_eq!(server.state().session_generation(), generation);
}

/// A rename on another project reports what `specforge check` reports for
/// it afterwards, in order, and the served project is untouched.
#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation on another project brings it up to date with what it wrote and reports what a fresh compile reports"
)]
fn a_mutation_on_another_project_reports_what_a_fresh_compile_reports() {
    use crate::fake_extension::{self, FakeExtension};

    let (mut server, ext, _served) = fake_extension::initialized(FakeExtension::new());
    let generation = server.state().session_generation();
    let other = fake_extension::project();
    fs::write(
        other.path().join("main.spec"),
        "behavior alpha \"A\" {\n}\n",
    )
    .unwrap();
    let loads = ext.handshakes();

    let resp = call_tool(
        &mut server,
        "specforge.rename",
        json!({"path": other.path().to_str().unwrap(), "entity_id": "alpha", "new_name": "gamma"}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    assert_eq!(
        ext.handshakes() - loads,
        1,
        "loaded once: the rename changed no environment input"
    );

    let payload: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    let runtime = ext.runtime();
    let fresh = specforge_project::CompiledProject::compile(other.path(), Some(runtime.clone()));
    let expected =
        serde_json::to_value(specforge_common::diagnostics_json(&fresh.diagnostics())).unwrap();
    assert_eq!(payload["diagnostics"], expected);
    assert_eq!(server.state().session_generation(), generation);
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation on another project brings it up to date with what it wrote and reports what a fresh compile reports"
)]
fn an_extension_added_to_another_project_is_loaded_when_it_is_brought_up_to_date() {
    let served = project(&[], "behavior login \"Login\" {\n}\n");
    let mut server = McpServer::new();
    initialize(&mut server, served.path());
    let generation = server.state().session_generation();
    let other = project(&[], "behavior alpha \"A\" {\n}\n");
    let path = other.path().to_str().unwrap();

    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"path": path, "specifiers": ["@specforge/software"]}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    assert_eq!(server.state().session_generation(), generation);

    let resp = call_tool(
        &mut server,
        "specforge.rename",
        json!({"path": path, "entity_id": "alpha", "new_name": "omega"}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    let payload: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    let fresh = specforge_project::CompiledProject::compile(other.path(), Some(Arc::new(runtime)));
    let expected =
        serde_json::to_value(specforge_common::diagnostics_json(&fresh.diagnostics())).unwrap();
    assert_eq!(payload["diagnostics"], expected);
    assert!(
        fresh.diagnostics().iter().all(|d| d.code != "I002"),
        "the added extension is loaded: {:?}",
        fresh.diagnostics()
    );
    assert_eq!(server.state().session_generation(), generation);
}

#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "a file under the spec root with no entities has an empty outline"
)]
fn outline_finds_an_empty_file_under_the_spec_root() {
    let dir = TempDir::new().unwrap();
    let config = json!({"name": "o", "version": "0.1.0", "spec_root": "spec"});
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    fs::create_dir_all(dir.path().join("spec")).unwrap();
    fs::write(
        dir.path().join("spec/main.spec"),
        "term alpha \"Alpha\" {\n}\n",
    )
    .unwrap();
    fs::write(dir.path().join("spec/empty.spec"), "// nothing yet\n").unwrap();
    let mut server = McpServer::new();
    initialize(&mut server, dir.path());

    let resp = call_tool(
        &mut server,
        "specforge.outline",
        json!({"file": "main.spec"}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    let resp = call_tool(
        &mut server,
        "specforge.outline",
        json!({"file": "empty.spec"}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    assert_eq!(tool_json(&resp), json!({"entries": []}));
    // A file that is not there is still not found.
    let resp = call_tool(
        &mut server,
        "specforge.outline",
        json!({"file": "gone.spec"}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "file_not_found", "{error}");
}

/// With nothing served no file is a project's: outline refuses as no project
/// (`precondition_failed`, naming the file), whatever the server's working
/// directory holds. Nextest runs this binary in the package root, where
/// `Cargo.toml` exists.
#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "with no project served, outline is the no-project refusal"
)]
fn outline_with_nothing_served_is_no_project() {
    assert!(
        Path::new("Cargo.toml").is_file(),
        "run from the package root"
    );
    let mut server = serving_nothing();

    for file in ["Cargo.toml", "absent.spec"] {
        let resp = call_tool(&mut server, "specforge.outline", json!({"file": file}));
        assert_eq!(resp["result"]["isError"], true, "{file}: {resp}");
        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["code"], "precondition_failed", "{file}: {error}");
        assert!(error["argument"].is_null(), "{file}: {error}");
        let message = error["message"].as_str().unwrap();
        assert!(
            message.starts_with(&format!(
                "no project is served, so '{file}' is no project's file"
            )),
            "{file}: {message}"
        );
    }
}

/// With nothing served, a read that names a file or an entity is the
/// no-project refusal, in a tool, a prompt and a resource; a read of the
/// whole project answers over the empty session.
#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "with no project served, a read naming a file or an entity is the no-project refusal, an aggregate read answers over the empty session"
)]
fn a_read_naming_something_with_nothing_served_is_no_project() {
    let mut server = serving_nothing();

    for (tool, arguments) in [
        ("specforge.inspect", json!({"entity_id": "x"})),
        ("specforge.find_definition", json!({"entity_id": "x"})),
        ("specforge.export", json!({"scope": "x"})),
    ] {
        let resp = call_tool(&mut server, tool, arguments);
        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["code"], "precondition_failed", "{tool}: {error}");
        assert_eq!(error["tool"], tool, "{error}");
        assert!(error["argument"].is_null(), "{tool}: {error}");
    }
    // The entity it was asked about stays in the refusal.
    let resp = call_tool(&mut server, "specforge.inspect", json!({"entity_id": "x"}));
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["entity_id"], "x", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .starts_with("no project is served, so entity 'x' is in no project"),
        "{error}"
    );

    // A prompt: an internal error (-32603), the refusal as its data.
    let resp = get_prompt(
        &mut server,
        "specforge://prompts/context",
        json!({"entity_id": "x"}),
    );
    assert_eq!(resp["error"]["code"], -32603, "{resp}");
    assert_eq!(
        resp["error"]["data"]["code"], "precondition_failed",
        "{resp}"
    );

    // A resource that names an entity.
    let resp = read_resource(&mut server, "specforge://graph/x");
    assert_eq!(resp["error"]["code"], -32603, "{resp}");
    assert_eq!(
        resp["error"]["data"]["code"], "precondition_failed",
        "{resp}"
    );
    assert_eq!(resp["error"]["data"]["entity_id"], "x", "{resp}");

    // The aggregates answer over the empty session.
    for (tool, arguments) in [
        ("specforge.list", json!({})),
        ("specforge.stats", json!({})),
    ] {
        let resp = call_tool(&mut server, tool, arguments);
        assert_eq!(resp["result"]["isError"], false, "{tool}: {resp}");
    }
    let resp = read_resource(&mut server, "specforge://graph");
    assert!(resp["error"].is_null(), "{resp}");
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "analyze with no project served and no path is a no-project error"
)]
fn analyze_without_a_project_is_refused() {
    let mut server = serving_nothing();
    let resp = call_tool(&mut server, "specforge.analyze", json!({}));
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "precondition_failed", "{error}");
    assert_eq!(error["tool"], "specforge.analyze", "{error}");
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "rename with a path to another project edits that project only and keeps serving this one"
)]
fn rename_on_another_project_edits_that_project_only() {
    let served = project(&[], "behavior alpha \"A\" {\n}\n");
    let other = project(
        &[],
        "behavior alpha \"B\" {\n}\nbehavior beta \"Beta\" {\n}\n",
    );
    let mut server = McpServer::new();
    initialize(&mut server, served.path());

    let resp = call_tool(
        &mut server,
        "specforge.rename",
        json!({"path": other.path().to_str().unwrap(), "entity_id": "alpha", "new_name": "gamma"}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    // The returned diagnostics are the other project's, compiled after the
    // edit: no dangling reference.
    let payload: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert!(
        payload["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["code"] != "E003")
    );

    assert!(
        fs::read_to_string(other.path().join("main.spec"))
            .unwrap()
            .contains("behavior gamma")
    );
    assert!(
        fs::read_to_string(served.path().join("main.spec"))
            .unwrap()
            .contains("behavior alpha")
    );
    assert_eq!(served_root(&server), Some(canonical(served.path())));
    assert!(server.state().graph().node("beta").is_none());
    assert!(server.state().graph().node("alpha").is_some());
}

/// What an entry kind a probe calls.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Entry {
    Tool,
    Prompt,
    Resource,
}

/// What a call with nothing served got.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Class {
    /// It answered (a prompt or resource result, or a tool result that is
    /// no error).
    Answers,
    /// It was asked about a file or an entity and refused as no project
    /// (ADR 0025): `no project is served, so ...`.
    Names,
    /// It refused as no project (`no project is served: ...`), naming `path`
    /// when the entry takes one.
    Refuses { path: bool },
}

/// Every core tool but init, prompt and resource, with nothing served: its
/// kind, name or URI, arguments as JSON text.
const NOTHING_SERVED_PROBES: &[(Entry, &str, &str)] = &[
    (Entry::Tool, "specforge.query", r#"{"entity_id":"x"}"#),
    (Entry::Tool, "specforge.validate", "{}"),
    (Entry::Tool, "specforge.analyze", "{}"),
    (Entry::Tool, "specforge.export", "{}"),
    (Entry::Tool, "specforge.trace", r#"{"entity_id":"x"}"#),
    (Entry::Tool, "specforge.search", r#"{"query":"x"}"#),
    (Entry::Tool, "specforge.explain", r#"{"code":"E003"}"#),
    (Entry::Tool, "specforge.schema", "{}"),
    (Entry::Tool, "specforge.model", "{}"),
    (Entry::Tool, "specforge.outline_extensions", "{}"),
    (Entry::Tool, "specforge.coverage", "{}"),
    (Entry::Tool, "specforge.stats", "{}"),
    (Entry::Tool, "specforge.list", "{}"),
    (Entry::Tool, "specforge.inspect", r#"{"entity_id":"x"}"#),
    (
        Entry::Tool,
        "specforge.find_definition",
        r#"{"entity_id":"x"}"#,
    ),
    (
        Entry::Tool,
        "specforge.find_references",
        r#"{"entity_id":"x"}"#,
    ),
    (Entry::Tool, "specforge.outline", r#"{"file":"a.spec"}"#),
    (Entry::Tool, "specforge.suggest_fixes", "{}"),
    (Entry::Tool, "specforge.format", r#"{"check":true}"#),
    (
        Entry::Tool,
        "specforge.rename",
        r#"{"entity_id":"x","new_name":"y","dry_run":true}"#,
    ),
    (
        Entry::Tool,
        "specforge.add_extension",
        r#"{"specifiers":["@specforge/software"],"dry_run":true}"#,
    ),
    (
        Entry::Tool,
        "specforge.remove_extension",
        r#"{"name":"@specforge/software","dry_run":true}"#,
    ),
    (Entry::Tool, "specforge.migrate", r#"{"dry_run":true}"#),
    (Entry::Tool, "specforge.extensions", "{}"),
    (Entry::Tool, "specforge.providers", "{}"),
    (Entry::Tool, "specforge.doctor", "{}"),
    (Entry::Tool, "specforge.collect", "{}"),
    (Entry::Tool, "specforge.render", r#"{"format":"brief"}"#),
    (Entry::Tool, "specforge.infer_progress", "{}"),
    (Entry::Tool, "specforge.infer_gaps", "{}"),
    (
        Entry::Tool,
        "specforge.infer_session",
        r#"{"action":"start"}"#,
    ),
    (
        Entry::Tool,
        "specforge.find_implementation",
        r#"{"entity_id":"x"}"#,
    ),
    (
        Entry::Tool,
        "specforge.find_spec_for_source",
        r#"{"file_path":"src/lib.rs"}"#,
    ),
    (
        Entry::Prompt,
        "specforge://prompts/context",
        r#"{"entity_id":"x"}"#,
    ),
    (Entry::Prompt, "specforge://prompts/review", "{}"),
    (
        Entry::Prompt,
        "specforge://prompts/trace",
        r#"{"entity_id":"x"}"#,
    ),
    (Entry::Prompt, "specforge://prompts/explore", "{}"),
    (Entry::Prompt, "specforge://prompts/infer", "{}"),
    (Entry::Resource, "specforge://graph", "{}"),
    (Entry::Resource, "specforge://schema", "{}"),
    (Entry::Resource, "specforge://context", "{}"),
    (Entry::Resource, "specforge://context/x", "{}"),
    (Entry::Resource, "specforge://brief", "{}"),
    (Entry::Resource, "specforge://diagnostics", "{}"),
    (Entry::Resource, "specforge://graph/x", "{}"),
    (Entry::Resource, "specforge://entities/feature", "{}"),
];

/// Call one probe against a server serving nothing, and the reply.
fn probe(server: &mut McpServer, entry: Entry, name: &str, arguments: &str) -> Value {
    let arguments: Value = serde_json::from_str(arguments).unwrap();
    match entry {
        Entry::Tool => call_tool(server, name, arguments),
        Entry::Prompt => get_prompt(server, name, arguments),
        Entry::Resource => read_resource(server, name),
    }
}

/// The class of a reply to a call made with nothing served.
fn class_of(entry: Entry, resp: &Value) -> Class {
    let refusal = match entry {
        Entry::Tool => {
            if resp["result"]["isError"] != true {
                return Class::Answers;
            }
            crate::tool_errors::mcp_error(resp)
        }
        Entry::Prompt | Entry::Resource => {
            if resp["error"].is_null() {
                return Class::Answers;
            }
            resp["error"]["data"].clone()
        }
    };
    assert_eq!(refusal["code"], "precondition_failed", "{resp}");
    let message = refusal["message"].as_str().unwrap();
    if message.starts_with("no project is served, so ") {
        Class::Names
    } else if message.starts_with("no project is served: ") {
        Class::Refuses {
            path: refusal["argument"] == "path",
        }
    } else {
        panic!("not a no-project refusal: {resp}")
    }
}

/// The target a probe's entry declares: a tool's own; every core prompt and
/// resource reads the view.
fn declared_target(entry: Entry, name: &str) -> TargetSpec {
    match entry {
        Entry::Tool => specforge_mcp::tools::core_tool(name)
            .unwrap_or_else(|| panic!("{name} is no core tool"))
            .target(),
        Entry::Prompt | Entry::Resource => TargetSpec::SERVED_VIEW,
    }
}

/// With nothing served, what a call gets is what its entry's target says: an
/// entry that reads only the project view answers (or, naming a file or an
/// entity, is the no-project refusal); one that acts on the project is
/// refused as no project, naming `path` where it takes one.
#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "with no project served, every core tool, prompt and resource answers or refuses as its target declares"
)]
fn every_core_entry_with_nothing_served_answers_or_refuses_as_its_target_declares() {
    let mut server = serving_nothing();
    for &(entry, name, arguments) in NOTHING_SERVED_PROBES {
        let target = declared_target(entry, name);
        let resp = probe(&mut server, entry, name, arguments);
        let class = class_of(entry, &resp);
        if target.answers_without_project() {
            assert!(
                matches!(class, Class::Answers | Class::Names),
                "{name} answers without a project: {class:?} {resp}"
            );
        } else {
            assert_eq!(
                class,
                Class::Refuses {
                    path: target.takes_path()
                },
                "{name}: {resp}"
            );
        }
    }

    // The probes cover every core tool but init, every prompt and every core
    // resource.
    let probed = |entry: Entry| -> Vec<&str> {
        NOTHING_SERVED_PROBES
            .iter()
            .filter(|probe| probe.0 == entry)
            .map(|probe| probe.1)
            .collect()
    };
    let mut tools: Vec<&str> = specforge_mcp::tools::CORE_TOOLS
        .iter()
        .map(|tool| tool.name())
        .filter(|name| *name != "specforge.init")
        .collect();
    tools.sort();
    let mut probed_tools = probed(Entry::Tool);
    probed_tools.sort();
    assert_eq!(probed_tools, tools);
    let prompts: Vec<&str> = specforge_mcp::prompts::CORE_PROMPTS
        .iter()
        .map(|prompt| prompt.name)
        .collect();
    assert_eq!(probed(Entry::Prompt), prompts);
    for resource in specforge_mcp::resources::CORE_RESOURCES {
        assert!(
            probed(Entry::Resource)
                .iter()
                .any(|uri| resource.names(uri)),
            "{} is not probed",
            resource.uri
        );
    }
}

/// With nothing served, an entry that acts on the project is refused as no
/// project before its arguments are read: an argument it does not declare or
/// cannot read is not the refusal.
#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "with no project served, an entry that acts on the project is refused before its arguments are read"
)]
fn a_call_that_needs_a_project_is_refused_before_its_arguments_are_read() {
    let mut server = serving_nothing();

    let resp = call_tool(&mut server, "specforge.extensions", json!({"bogus": 1}));
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "precondition_failed", "{error}");
    assert!(error["argument"].is_null(), "{error}");

    let resp = call_tool(
        &mut server,
        "specforge.validate",
        json!({"severity_filter": "nope"}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "precondition_failed", "{error}");
    assert_eq!(error["argument"], "path", "{error}");

    let resp = call_tool(
        &mut server,
        "specforge.rename",
        json!({"entity_id": "x", "new_name": "y", "bogus": 1}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "precondition_failed", "{error}");
    assert_eq!(error["argument"], "path", "{error}");
    assert_eq!(error["data"]["files_written"], json!([]), "{error}");
}

/// A directory with a spec file and no `specforge.json`.
fn loose_directory() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.spec"), "behavior l \"L\" {\n}\n").unwrap();
    dir
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a server whose root lies in no project serves nothing, and init then serves the project it creates"
)]
fn a_root_in_no_project_serves_nothing_and_init_serves_what_it_creates() {
    let loose = loose_directory();
    let mut server = McpServer::with_project_root(loose.path().to_path_buf());
    let resp = call(&mut server, "initialize", json!({}));
    assert!(resp["error"].is_null(), "{resp}");
    assert_eq!(served_root(&server), None);

    let nested = loose.path().join("sub");
    let resp = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": nested.to_str().unwrap()}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    assert!(nested.join("specforge.json").exists());
    assert_eq!(served_root(&server), Some(canonical(&nested)));
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a server started inside a project serves that project"
)]
fn a_root_inside_a_project_serves_that_project() {
    let p = project(&[], "behavior ok \"Ok\" {\n}\n");
    let docs = p.path().join("docs");
    fs::create_dir(&docs).unwrap();
    let mut server = McpServer::new();
    initialize(&mut server, &docs);
    assert_eq!(served_root(&server), Some(canonical(p.path())));

    let resp = call_tool(
        &mut server,
        "specforge.stats",
        json!({"path": p.path().to_str().unwrap()}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
}

/// The project `specforge.json` enables software for; its spec root `spec/`
/// holds a `specforge.spec` (a `spec` block) and `a.spec`.
fn project_with_a_spec_block() -> TempDir {
    let p = project(&["@specforge/software"], "");
    fs::remove_file(p.path().join("main.spec")).unwrap();
    fs::create_dir(p.path().join("spec")).unwrap();
    fs::write(p.path().join("spec/specforge.spec"), "spec \"p\" {}\n").unwrap();
    fs::write(p.path().join("spec/a.spec"), "behavior ok \"Ok\" {\n}\n").unwrap();
    p
}

#[specforge_test(
    behavior = "locate_project_root",
    verify = "a server started in a spec root holding specforge.spec serves the project around it"
)]
fn a_server_started_in_a_spec_root_holding_specforge_spec_serves_the_project_around_it() {
    let p = project_with_a_spec_block();
    let mut server = McpServer::new();
    initialize(&mut server, &p.path().join("spec"));
    assert_eq!(served_root(&server), Some(canonical(p.path())));
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a path to a directory that is no project is read for that call and never served"
)]
fn a_loose_path_is_opened_for_its_call_and_never_served() {
    let loose = loose_directory();
    let mut server = serving_nothing();

    let resp = call_tool(
        &mut server,
        "specforge.validate",
        json!({"path": loose.path().to_str().unwrap()}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    // The loose directory has no extensions installed, so its sources are
    // read with default configuration (I002).
    let diagnostics: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert!(
        diagnostics
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "I002"),
        "{diagnostics}"
    );
    assert_eq!(served_root(&server), None);

    let nested = loose.path().join("sub");
    let resp = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": nested.to_str().unwrap()}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    assert_eq!(served_root(&server), Some(canonical(&nested)));
}
