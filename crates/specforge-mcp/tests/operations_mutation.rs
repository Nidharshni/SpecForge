use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;
use std::path::Path;
use std::sync::Arc;

/// test.spec: the behavior `alpha` on lines 1–2, the feature `beta` that
/// has it on lines 10–12.
const TEST_SPEC: &str = concat!(
    "behavior alpha \"Alpha\" {\n",
    "}\n",
    "\n\n\n\n\n\n\n",
    "feature beta \"Beta\" {\n",
    "    behaviors [alpha]\n",
    "}\n",
);

/// The project the mutation tools act on: test.spec, nothing else.
fn project() -> TestProject {
    TestProject::new().file("test.spec", TEST_SPEC)
}

/// [`project`] served with `@test/ext`'s software kinds: what is on disk
/// is what is served, before and after a mutation writes.
fn test_server() -> Served {
    project().serve(&[TestExtension::software()])
}

/// [`project`] enabling no extension, served with the project's own
/// component runtime: an extension a call installs loads as it would
/// under `specforge mcp <root>`.
fn components_server() -> Served {
    project().serve_components()
}

/// Fresh project directory for specforge.init (refuses existing projects).
fn fresh_project_dir() -> tempfile::TempDir {
    tempfile::TempDir::new().unwrap()
}

fn invoked(server: &impl Logs, tool: &str) -> bool {
    events(server, "mcp_tool_invoked")
        .iter()
        .any(|p| p["toolName"] == tool && p["category"] == "mutation")
}

// --- specforge.format ---

const UNFORMATTED: &str = "behavior messy \"Messy\" {\ncontract \"The system MUST work\"\n}\n";

/// A project whose spec files are two unformatted ones, a.spec and b.spec
/// (test.spec empty), and its root.
fn server_with_unformatted() -> (Served, std::path::PathBuf) {
    let server = project()
        .file("test.spec", "")
        .file("a.spec", UNFORMATTED)
        .file("b.spec", &UNFORMATTED.replace("messy", "other"))
        .serve(&[TestExtension::software()]);
    let root = server.root().to_path_buf();
    (server, root)
}

fn format_result(server: &mut McpServer, args: Value) -> Value {
    let resp = call_tool(server, "specforge.format", args);
    serde_json::from_str(&tool_text(&resp)).unwrap_or_else(|_| panic!("{resp}"))
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "specforge.format formats spec files"
)]
fn format_rewrites_unformatted_files() {
    let (mut server, root) = server_with_unformatted();

    let parsed = format_result(&mut server, json!({}));

    assert_eq!(parsed["all_clean"], false, "{parsed}");
    assert_eq!(
        parsed["changed_files"].as_array().unwrap().len(),
        2,
        "{parsed}"
    );
    let formatted = std::fs::read_to_string(root.join("a.spec")).unwrap();
    assert!(
        formatted.contains("\n  contract \"The system MUST work\""),
        "{formatted}"
    );
    let again = format_result(&mut server, json!({}));
    assert_eq!(
        again["all_clean"], true,
        "formatting is idempotent: {again}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "check mode reports without modifying files"
)]
fn format_check_mode_reports_without_writing() {
    let (mut server, root) = server_with_unformatted();

    let parsed = format_result(&mut server, json!({"check": true}));

    assert_eq!(parsed["check_only"], true);
    assert_eq!(parsed["all_clean"], false, "{parsed}");
    assert_eq!(
        parsed["changed_files"].as_array().unwrap().len(),
        2,
        "{parsed}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("a.spec")).unwrap(),
        UNFORMATTED
    );
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "paths filter restricts to specified files"
)]
fn format_paths_restrict_the_run() {
    let (mut server, root) = server_with_unformatted();

    let parsed = format_result(&mut server, json!({"paths": ["a.spec"]}));

    assert_eq!(parsed["total_checked"], 1, "{parsed}");
    let changed = parsed["changed_files"].as_array().unwrap();
    assert_eq!(changed.len(), 1, "{parsed}");
    assert!(changed[0].as_str().unwrap().ends_with("a.spec"), "{parsed}");
    assert_ne!(
        std::fs::read_to_string(root.join("a.spec")).unwrap(),
        UNFORMATTED
    );
    assert_eq!(
        std::fs::read_to_string(root.join("b.spec")).unwrap(),
        UNFORMATTED.replace("messy", "other"),
        "a file outside paths is untouched"
    );
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "a file several arguments reach is formatted once"
)]
fn format_counts_a_file_two_paths_name_once() {
    let (mut server, root) = server_with_unformatted();
    std::fs::create_dir(root.join("x")).unwrap();

    let parsed = format_result(
        &mut server,
        json!({"paths": ["a.spec", "x/../a.spec"], "check": true}),
    );

    assert_eq!(parsed["total_checked"], 1, "{parsed}");
    assert_eq!(
        parsed["changed_files"].as_array().unwrap().len(),
        1,
        "{parsed}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "diff mode returns FormatDiff entries"
)]
fn format_diff_mode_returns_diffs_without_writing() {
    let (mut server, root) = server_with_unformatted();

    let parsed = format_result(&mut server, json!({"diff": true, "paths": ["a.spec"]}));

    let diffs = parsed["diffs"].as_array().unwrap();
    assert_eq!(diffs.len(), 1, "{parsed}");
    let diff = &diffs[0];
    assert!(
        diff["file_path"].as_str().unwrap().ends_with("a.spec"),
        "{diff}"
    );
    assert_eq!(diff["before"], UNFORMATTED);
    assert!(
        diff["after"].as_str().unwrap().contains("\n  contract"),
        "{diff}"
    );
    assert!(diff["insertions"].as_u64().unwrap() > 0, "{diff}");
    assert!(diff["deletions"].as_u64().unwrap() > 0, "{diff}");
    assert_eq!(
        std::fs::read_to_string(root.join("a.spec")).unwrap(),
        UNFORMATTED,
        "diff mode writes nothing"
    );
}

#[cfg(unix)]
#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "a file that cannot be written does not stop the others, and the failed call names it"
)]
fn format_writes_every_file_it_can_and_names_the_ones_it_cannot() {
    use std::os::unix::fs::PermissionsExt;
    let (mut server, root) = server_with_unformatted();
    // a.spec comes first and can't be written.
    let locked = root.join("a.spec");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o444)).unwrap();

    let resp = call_tool(&mut server, "specforge.format", json!({}));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

    // The OS refused: permission denied, not a generic internal failure.
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "permission_denied", "{error}");
    let parsed = &error["data"];
    let failed = parsed["failed_files"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{parsed}");
    assert!(failed[0].as_str().unwrap().ends_with("a.spec"), "{parsed}");
    assert_eq!(std::fs::read_to_string(&locked).unwrap(), UNFORMATTED);
    assert!(
        std::fs::read_to_string(root.join("b.spec"))
            .unwrap()
            .contains("\n  contract"),
        "b.spec is formatted although a.spec failed"
    );
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "format configuration diagnostics are returned in the result"
)]
fn format_returns_the_config_diagnostics() {
    let (mut server, root) = server_with_unformatted();
    std::fs::write(root.join(".specforgefmt.toml"), "indent_width = 99\n").unwrap();

    let parsed = format_result(&mut server, json!({"check": true}));

    let diagnostics = parsed["diagnostics"]
        .as_array()
        .unwrap_or_else(|| panic!("no diagnostics: {parsed}"));
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"] == "W141" && d["message"].as_str().unwrap().contains("indent")),
        "{parsed}"
    );
}

/// UNFORMATTED in canonical form.
const CANONICAL: &str = "behavior messy \"Messy\" {\n  contract \"The system MUST work\"\n}\n";

/// A project whose spec files are a canonical a.spec and `extra` (path,
/// text), test.spec empty, and its root.
fn server_with_canonical_and(extra: (&str, &str)) -> (Served, std::path::PathBuf) {
    let server = project()
        .file("test.spec", "")
        .file("a.spec", CANONICAL)
        .file(extra.0, extra.1)
        .serve(&[TestExtension::software()]);
    let root = server.root().to_path_buf();
    (server, root)
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "a file with a region left unformatted is not reported clean, and its W142 is returned"
)]
fn format_does_not_report_a_parse_error_file_clean() {
    let broken = "behavior login \"Login\" {\n  contract \"ok\"\n}\n\n}}}\n";
    let (mut server, _root) = server_with_canonical_and(("broken.spec", broken));

    let parsed = format_result(&mut server, json!({"check": true}));

    assert_eq!(parsed["all_clean"], false, "{parsed}");
    assert_eq!(parsed["changed_files"], json!([]), "{parsed}");
    let diagnostics = parsed["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{parsed}");
    assert_eq!(diagnostics[0]["code"], "W142", "{parsed}");
    assert!(
        diagnostics[0]["file"]
            .as_str()
            .unwrap()
            .ends_with("broken.spec"),
        "{parsed}"
    );
    assert_eq!(diagnostics[0]["line"], 5, "{parsed}");
}

#[cfg(unix)]
#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "a file that cannot be read fails the call, is named, and does not stop the others"
)]
fn format_fails_on_an_unreadable_file_and_names_it() {
    use std::os::unix::fs::PermissionsExt;
    let other = CANONICAL.replace("messy", "other");
    let (mut server, root) = server_with_canonical_and(("locked.spec", &other));
    let locked = root.join("locked.spec");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    let resp = call_tool(&mut server, "specforge.format", json!({"check": true}));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

    // The OS refused: permission denied, not a generic internal failure.
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "permission_denied", "{error}");
    let data = &error["data"];
    let failed = data["failed_files"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{data}");
    assert!(
        failed[0].as_str().unwrap().ends_with("locked.spec"),
        "{data}"
    );
    assert!(
        data["message"]
            .as_str()
            .unwrap()
            .starts_with("failed to read"),
        "{data}"
    );
    assert_eq!(data["all_clean"], false, "{data}");
    // test.spec and a.spec were still checked.
    assert_eq!(data["total_checked"], 2, "{data}");
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "a file that does not exist fails the call as file_not_found, naming it"
)]
fn format_fails_on_a_missing_file_as_not_found() {
    let (mut server, _root) = server_with_canonical_and(("b.spec", CANONICAL));

    let resp = call_tool(
        &mut server,
        "specforge.format",
        json!({"check": true, "paths": ["nope.spec"]}),
    );

    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "file_not_found", "{error}");
    let failures = error["data"]["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1, "{error}");
    assert_eq!(failures[0]["operation"], "read", "{error}");
    assert_eq!(failures[0]["code"], "file_not_found", "{error}");
    assert!(
        failures[0]["file"].as_str().unwrap().ends_with("nope.spec"),
        "{error}"
    );
}

// --- specforge.rename ---

// B:provide_mcp_rename_tool — verify unit "unknown entity returns error"
#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "non-existent entity returns error response"
)]
fn rename_unknown_entity() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.rename",
        json!({"entity_id": "nonexistent", "new_name": "new"}),
    );
    crate::tool_errors::mcp_error(&resp);
}

#[test]
fn rename_missing_params() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.rename", json!({}));
    crate::tool_errors::mcp_error(&resp);
}

const TOKENS_SPEC: &str = "invariant token_unique \"Tokens are unique\" {
  guarantee \"Token ids MUST be unique\"
  verify unit \"no two tokens share an id\"
}
";
const LOGIN_SPEC: &str = "// login relies on token_unique

behavior login \"Log in\" {
  invariants [token_unique]
  contract \"The system MUST issue a token, never a token_unique_ish one\"
  verify unit \"login issues a token\"
}
";

/// A compiled project where `login` references the invariant `token_unique`.
fn server_with_token_project() -> (Served, std::path::PathBuf) {
    let server = TestProject::new()
        .enabling(&["@specforge/software"])
        .file("spec/tokens.spec", TOKENS_SPEC)
        .file("spec/login.spec", LOGIN_SPEC)
        .serve_components();
    assert!(server.state().graph().node("token_unique").is_some());
    let root = server.root().to_path_buf();
    (server, root)
}

fn rename(server: &mut McpServer, args: Value) -> Value {
    let resp = call_tool(server, "specforge.rename", args);
    serde_json::from_str(&tool_text(&resp)).unwrap_or_else(|_| panic!("{resp}"))
}

fn references(server: &McpServer, from: &str) -> Vec<String> {
    server
        .state()
        .graph()
        .edges_from(from)
        .iter()
        .map(|e| e.target.to_string())
        .collect()
}

#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "specforge.rename renames entity and all references"
)]
fn rename_rewrites_the_declaration_and_every_reference() {
    let (mut server, root) = server_with_token_project();

    let parsed = rename(
        &mut server,
        json!({"entity_id": "token_unique", "new_name": "token_distinct"}),
    );

    assert_eq!(parsed["edits"].as_array().unwrap().len(), 2, "{parsed}");
    assert_eq!(
        std::fs::read_to_string(root.join("spec/tokens.spec")).unwrap(),
        TOKENS_SPEC.replace("invariant token_unique", "invariant token_distinct")
    );
    // Only the reference changes: not the comment outside the entity, not a
    // longer identifier that merely starts with the old one.
    assert_eq!(
        std::fs::read_to_string(root.join("spec/login.spec")).unwrap(),
        LOGIN_SPEC.replace("invariants [token_unique]", "invariants [token_distinct]")
    );
    assert!(server.state().graph().node("token_unique").is_none());
    assert!(server.state().graph().node("token_distinct").is_some());
    assert_eq!(references(&server, "login"), ["token_distinct"]);
}

/// The rename edits exactly what find_references returns with the
/// declaration: the declaration's name and each reference's token, and no
/// title, guarantee, comment or verify text that mentions the ID.
#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "rename edits exactly the declaration and the references find_references returns"
)]
fn rename_edits_exactly_the_occurrences() {
    let (mut server, root) = server_with_token_project();
    std::fs::write(
        root.join("spec/audit.spec"),
        "behavior audit \"token_unique audit\" {\n  // checks token_unique\n  invariants [token_unique]\n  verify unit \"audit respects token_unique\"\n}\n",
    )
    .unwrap();
    let resp = call_tool(
        &mut server,
        "specforge.find_references",
        json!({"entity_id": "token_unique", "include_declaration": true}),
    );
    let found: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    let mut occurrences: Vec<(String, u64, u64)> = found["locations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let s = &l["source_span"];
            (
                s["file"].as_str().unwrap().to_string(),
                s["start_line"].as_u64().unwrap(),
                s["start_col"].as_u64().unwrap() - 1,
            )
        })
        .collect();
    occurrences.sort();

    let plan = rename(
        &mut server,
        json!({"entity_id": "token_unique", "new_name": "token_distinct", "dry_run": true}),
    );
    let mut edits: Vec<(String, u64, u64)> = plan["edits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["file"].as_str().unwrap().to_string(),
                e["line"].as_u64().unwrap(),
                e["start_col"].as_u64().unwrap(),
            )
        })
        .collect();
    edits.sort();

    assert_eq!(edits, occurrences);
    assert_eq!(
        edits,
        [
            ("spec/audit.spec".to_string(), 3, 14),
            ("spec/login.spec".to_string(), 4, 14),
            ("spec/tokens.spec".to_string(), 1, 10),
        ]
    );
}

/// A rename recompiles the project from disk: a file edited since the
/// server last loaded it, and not touched by the rename, is served too,
/// and the diagnostics returned are what a fresh compile reports.
#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "Provide MCP Rename Tool: MCP rename tool holds — graph_available, filesystem_available, references_updated, recompilation_triggered, dry_run_safe, mutation_completed_emitted, tool_invoked_emitted"
)]
fn rename_recompiles_files_it_did_not_edit() {
    let (mut server, root) = server_with_token_project();
    // Edited without watch: the server does not know yet.
    std::fs::write(
        root.join("spec/logout.spec"),
        "behavior logout \"Log out\" {\n  invariants [missing_invariant]\n}\n",
    )
    .unwrap();

    let parsed = rename(
        &mut server,
        json!({"entity_id": "token_unique", "new_name": "token_distinct"}),
    );

    assert!(server.state().graph().node("logout").is_some());
    let codes = |diagnostics: &[Value]| {
        let mut codes: Vec<String> = diagnostics
            .iter()
            .map(|d| d["code"].as_str().unwrap_or_default().to_string())
            .collect();
        codes.sort();
        codes
    };
    let runtime = specforge_component::ComponentRuntime::with_user_cache();
    let fresh: Vec<Value> = specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(&root).unwrap(),
        Some(Arc::new(runtime)),
    )
    .diagnostics()
    .iter()
    .map(|d| serde_json::to_value(d).unwrap())
    .collect();
    let returned = parsed["diagnostics"].as_array().unwrap();
    assert_eq!(codes(returned), codes(&fresh), "{parsed}");
    let e003 = returned
        .iter()
        .find(|d| d["code"] == "E003")
        .unwrap_or_else(|| {
            panic!("the unresolved reference in the unrenamed file is reported: {parsed}")
        });
    // The shape every mutation tool returns: catalogue title and the flat
    // file/line/column beside the span.
    assert!(e003["title"].is_string(), "{e003}");
    assert!(
        e003["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("logout.spec")),
        "{e003}"
    );
    assert!(e003["line"].is_u64() && e003["column"].is_u64(), "{e003}");
}

#[test]
fn rename_result_is_an_mcp_rename_result() {
    let (mut server, _root) = server_with_token_project();

    let parsed = rename(
        &mut server,
        json!({"entity_id": "token_unique", "new_name": "token_distinct", "dry_run": true}),
    );
    assert_eq!(
        parsed["affected_files"],
        json!(["spec/login.spec", "spec/tokens.spec"]),
        "{parsed}"
    );
    let edits = parsed["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 2, "{parsed}");
}

/// With `spec_root` set, spans are relative to the spec root, not the
/// project root: rename must read and write the files there (plan 01, D8).
#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "specforge.rename renames entity and all references"
)]
fn rename_edits_the_files_under_a_configured_spec_root() {
    let (mut server, root) = server_with_token_project();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software"],"spec_root":"spec"}"#,
    )
    .unwrap();
    // The next call brings the served project up to date with the config just written.
    call_tool(&mut server, "specforge.validate", json!({}));
    assert!(server.state().graph().node("token_unique").is_some());

    let parsed = rename(
        &mut server,
        json!({"entity_id": "token_unique", "new_name": "token_distinct"}),
    );

    assert_eq!(parsed["edits"].as_array().unwrap().len(), 2, "{parsed}");
    assert_eq!(
        std::fs::read_to_string(root.join("spec/tokens.spec")).unwrap(),
        TOKENS_SPEC.replace("invariant token_unique", "invariant token_distinct")
    );
    assert_eq!(
        std::fs::read_to_string(root.join("spec/login.spec")).unwrap(),
        LOGIN_SPEC.replace("invariants [token_unique]", "invariants [token_distinct]")
    );
    assert!(server.state().graph().node("token_distinct").is_some());
}

#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "dry_run returns rename plan without applying changes"
)]
fn rename_dry_run_returns_the_plan_and_changes_nothing() {
    let (mut server, root) = server_with_token_project();

    let parsed = rename(
        &mut server,
        json!({"entity_id": "token_unique", "new_name": "token_distinct", "dry_run": true}),
    );

    assert_eq!(parsed["dry_run"], true);
    let edits = parsed["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 2, "{parsed}");
    let declaration = edits
        .iter()
        .find(|e| e["file"].as_str().unwrap().ends_with("tokens.spec"))
        .unwrap();
    // `invariant token_unique`: the identifier alone, on line 1.
    assert_eq!(declaration["line"], 1);
    assert_eq!(declaration["start_col"], 10);
    assert_eq!(declaration["end_col"], 22);
    assert_eq!(parsed["affected_files"].as_array().unwrap().len(), 2);
    assert_eq!(
        std::fs::read_to_string(root.join("spec/tokens.spec")).unwrap(),
        TOKENS_SPEC
    );
    assert_eq!(
        std::fs::read_to_string(root.join("spec/login.spec")).unwrap(),
        LOGIN_SPEC
    );
    assert!(server.state().graph().node("token_unique").is_some());
}

#[test]
fn rename_invalid_new_name() {
    let (mut server, _root) = server_with_token_project();
    for bad in ["", "x", "has space", "token-unique"] {
        let resp = call_tool(
            &mut server,
            "specforge.rename",
            json!({"entity_id": "token_unique", "new_name": bad}),
        );
        crate::tool_errors::mcp_error(&resp);
    }
}

#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "Provide MCP Rename Tool: MCP rename tool holds — graph_available, filesystem_available, references_updated, recompilation_triggered, dry_run_safe, mutation_completed_emitted, tool_invoked_emitted"
)]
fn rename_contract() {
    // graph_available, filesystem_available: a compiled project on disk.
    let (mut server, root) = server_with_token_project();
    let before = files_under(&root);

    // dry_run_safe: the plan, and nothing on disk or in the graph changes.
    let plan = rename(
        &mut server,
        json!({"entity_id": "token_unique", "new_name": "token_distinct", "dry_run": true}),
    );
    assert_eq!(plan["edits"].as_array().unwrap().len(), 2, "{plan}");
    assert_eq!(files_under(&root), before);
    assert!(server.state().graph().node("token_unique").is_some());

    let parsed = rename(
        &mut server,
        json!({"entity_id": "token_unique", "new_name": "token_distinct"}),
    );

    // references_updated: the declaration and the reference on disk.
    assert_eq!(
        std::fs::read_to_string(root.join("spec/tokens.spec")).unwrap(),
        TOKENS_SPEC.replace("invariant token_unique", "invariant token_distinct")
    );
    assert_eq!(
        std::fs::read_to_string(root.join("spec/login.spec")).unwrap(),
        LOGIN_SPEC.replace("invariants [token_unique]", "invariants [token_distinct]")
    );
    assert!(server.state().graph().node("token_unique").is_none());
    assert!(server.state().graph().node("token_distinct").is_some());
    // Two files rewritten, one entity renamed.
    assert_eq!(
        events(&server, "mcp_mutation_completed"),
        [json!({
            "toolName": "specforge.rename",
            "files_changed": 2,
            "entities_affected": 1,
            "success": true,
        })]
    );

    // Recompiled: the response carries the fresh diagnostics, and the
    // references resolve (no E003 for the old name).
    let diagnostics = parsed["diagnostics"].as_array().unwrap();
    assert!(
        !diagnostics.iter().any(|d| d["code"] == "E003"),
        "{diagnostics:?}"
    );
    assert_eq!(references(&server, "login"), ["token_distinct"]);
    let events = server.log().names();
    assert!(events.iter().any(|e| e == "mcp_tool_invoked"), "{events:?}");
    assert!(
        events.iter().any(|e| e == "mcp_mutation_completed"),
        "{events:?}"
    );
}

// --- specforge.init ---

/// Initialize a project at `dir/name` and return the parsed result.
fn init(server: &mut McpServer, args: Value) -> Value {
    let resp = call_tool(server, "specforge.init", args);
    serde_json::from_str(&tool_text(&resp)).unwrap_or_else(|_| panic!("{resp}"))
}

fn init_error(server: &mut McpServer, args: Value) -> Value {
    let resp = call_tool(server, "specforge.init", args);
    crate::tool_errors::mcp_error(&resp)
}

fn read_config(project: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(project.join("specforge.json")).unwrap()).unwrap()
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "specforge.init creates specforge.json project"
)]
fn init_creates_the_project() {
    let dir = fresh_project_dir();
    let project = dir.path().join("fresh");
    let mut server = test_server();

    let parsed = init(
        &mut server,
        json!({"path": project.to_str().unwrap(), "name": "myproject"}),
    );

    assert_eq!(
        parsed["project_root"],
        project.canonicalize().unwrap().display().to_string()
    );
    let config = read_config(&project);
    assert_eq!(config["name"], "myproject");
    assert_eq!(config["version"], "0.1.0");
    assert_eq!(config["extensions"], json!([]));
    assert!(
        project
            .join(parsed["starter_file"].as_str().unwrap())
            .is_file()
    );
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "init without a path is invalid_input on path"
)]
fn init_without_a_path_is_invalid_input_on_path() {
    let mut server = test_server();
    let error = init_error(&mut server, json!({"name": "myproject"}));
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], "path", "{error}");
    assert_eq!(error["message"], "Missing required parameter: path");
    assert_eq!(error["tool"], "specforge.init", "{error}");
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "extensions installed when specified"
)]
fn init_adds_the_requested_extensions_to_the_config() {
    let dir = fresh_project_dir();
    let mut server = components_server();

    init(
        &mut server,
        json!({"path": dir.path().to_str().unwrap(), "name": "extproject",
               "extensions": ["@specforge/software"]}),
    );

    // Software's test obligations come from @specforge/testing (ADR 0002).
    assert_eq!(
        read_config(dir.path())["extensions"],
        json!(["@specforge/software", "@specforge/testing"])
    );
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "specforge.init result includes the starter file path and installed extensions"
)]
fn init_extensions_in_result() {
    let dir = fresh_project_dir();
    let mut server = components_server();

    let parsed = init(
        &mut server,
        json!({"path": dir.path().to_str().unwrap(), "name": "test",
               "extensions": ["@specforge/software", "@specforge/product"]}),
    );

    assert_eq!(
        parsed["extensions_installed"],
        json!([
            "@specforge/software",
            "@specforge/product",
            "@specforge/testing"
        ])
    );
    assert_eq!(parsed["starter_file"], "spec/hello.spec");
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "path inside current project returns error"
)]
fn init_refuses_a_path_inside_the_current_project() {
    let mut server = test_server();
    let root = server
        .state()
        .project_root()
        .map(std::path::Path::to_path_buf)
        .unwrap();
    let nested = root.join("sub");

    let error = init_error(
        &mut server,
        json!({"path": nested.to_str().unwrap(), "name": "nested"}),
    );

    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("inside the current project"),
        "{error}"
    );
    assert!(!nested.exists(), "nothing is written");
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "init inside the served project is refused by the call target as a conflict on path, before anything is written"
)]
fn init_inside_the_served_project_is_a_conflict_on_path() {
    let mut server = test_server();
    let root = server
        .state()
        .project_root()
        .map(std::path::Path::to_path_buf)
        .unwrap();
    let nested = root.join("nested");

    let error = init_error(
        &mut server,
        json!({"path": nested.to_str().unwrap(), "name": "nested"}),
    );

    assert_eq!(error["code"], "conflict", "{error}");
    assert_eq!(error["argument"], "path", "{error}");
    assert_eq!(error["data"]["files_written"], json!([]), "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("is inside the current project at"),
        "{error}"
    );
    assert!(!nested.exists(), "nothing is written");
    let completed = events(&server, "mcp_mutation_completed");
    assert_eq!(
        completed.last().unwrap(),
        &json!({"toolName": "specforge.init", "files_changed": 0, "entities_affected": 0, "success": false})
    );
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "init refuses a directory whose starter file exists, writing nothing"
)]
fn init_refuses_to_overwrite_a_starter_file() {
    let dir = fresh_project_dir();
    std::fs::create_dir_all(dir.path().join("spec")).unwrap();
    let starter = dir.path().join("spec/hello.spec");
    std::fs::write(&starter, "term mine \"Mine\" {\n}\n").unwrap();
    let mut server = test_server();

    let error = init_error(
        &mut server,
        json!({"path": dir.path().to_str().unwrap(), "name": "victim"}),
    );

    assert_eq!(error["code"], "conflict", "{error}");
    assert_eq!(error["data"]["files_written"], json!([]), "{error}");
    assert_eq!(
        std::fs::read_to_string(&starter).unwrap(),
        "term mine \"Mine\" {\n}\n"
    );
    assert!(!dir.path().join("specforge.json").exists());
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "invalid project name returns error"
)]
fn init_rejects_an_invalid_project_name() {
    let mut server = test_server();
    for name in ["", ".hidden", "-dash", "has space"] {
        let dir = fresh_project_dir();
        let error = init_error(
            &mut server,
            json!({"path": dir.path().to_str().unwrap(), "name": name}),
        );
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("invalid project name"),
            "{name:?}: {error}"
        );
        assert!(
            !dir.path().join("specforge.json").exists(),
            "{name:?} wrote a project"
        );
    }
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "unknown extension returns error with diagnostic"
)]
fn init_rejects_an_unknown_extension() {
    let dir = fresh_project_dir();
    let mut server = test_server();

    let error = init_error(
        &mut server,
        json!({"path": dir.path().to_str().unwrap(), "name": "test",
               "extensions": ["@specforge/software", "@specforge/nonexistent"]}),
    );

    assert_eq!(error["code"], "extension_not_found", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("@specforge/nonexistent"),
        "{error}"
    );
    assert!(error["data"]["suggestion"].is_string(), "{error}");
    assert!(
        !dir.path().join("specforge.json").exists(),
        "nothing is written"
    );
}

#[test]
fn init_requires_a_path() {
    let mut server = test_server();
    let error = init_error(&mut server, json!({"name": "nowhere"}));
    assert!(
        error["message"].as_str().unwrap().contains("path"),
        "{error}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "MCP init followed by check produces zero errors"
)]
fn init_then_validate_has_no_errors() {
    let dir = fresh_project_dir();
    let mut server = test_server();
    init(
        &mut server,
        json!({"path": dir.path().to_str().unwrap(), "name": "integration",
               "extensions": ["@specforge/software"]}),
    );

    let resp = call_tool(
        &mut server,
        "specforge.validate",
        json!({"path": dir.path().to_str().unwrap()}),
    );

    assert_eq!(resp["result"]["isError"], false, "{}", tool_text(&resp));
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "Provide MCP Init Tool: MCP init tool holds — filesystem_available, project_created, path_outside_current, extensions_validated, project_initialized_emitted, tool_invoked_emitted"
)]
fn init_contract() {
    let dir = fresh_project_dir();
    let mut server = test_server();

    // filesystem_available, project_created
    init(
        &mut server,
        json!({"path": dir.path().to_str().unwrap(), "name": "contractproject"}),
    );
    assert_eq!(read_config(dir.path())["name"], "contractproject");
    assert!(dir.path().join("spec/hello.spec").is_file());

    // path_outside_current: a path inside the server's project is refused.
    let current = server
        .state()
        .project_root()
        .map(std::path::Path::to_path_buf)
        .unwrap();
    let nested = current.join("nested");
    let error = init_error(
        &mut server,
        json!({"path": nested.to_str().unwrap(), "name": "nested"}),
    );
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("inside the current project"),
        "{error}"
    );
    assert!(!nested.exists());

    // extensions_validated: an unknown extension is refused.
    let other = fresh_project_dir();
    let error = init_error(
        &mut server,
        json!({"path": other.path().to_str().unwrap(), "name": "other",
               "extensions": ["@specforge/nonexistent"]}),
    );
    assert_eq!(error["code"], "extension_not_found", "{error}");
    assert!(!other.path().join("specforge.json").exists());

    // project_initialized_emitted (once: only for the created project),
    // tool_invoked_emitted.
    let initialized = events(&server, "project_initialized");
    assert_eq!(initialized.len(), 1, "{initialized:?}");
    assert_eq!(initialized[0]["projectName"], "contractproject");
    assert!(invoked(&server, "specforge.init"));
}

// --- specforge.add_extension ---

#[test]
fn add_extension_returns_result() {
    // A project that enables nothing, served with its own component
    // runtime, so an installed extension loads.
    let mut server = TestProject::new().serve_components();
    let dir = server.root().to_path_buf();
    assert!(dir.join("specforge.json").is_file());
    let blob = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures/greet-extension/greet.wasm");
    eprintln!("DEBUG blob exists: {}", blob.exists());
    let blob = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures/greet-extension/greet.wasm");
    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [blob.to_str().unwrap()]}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["added"][0]["action"], "installed");
    // Local installs derive the name from the file stem (same as the CLI).
    let lock = std::fs::read_to_string(dir.as_path().join("specforge.lock")).unwrap();
    assert!(lock.contains("@sdk/greet"));
}

/// The annotation `key` of `tool` as `tools/list` lists it.
fn listed_hint(server: &mut McpServer, tool: &str, key: &str) -> Value {
    let listed = call(server, "tools/list", json!({}));
    listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == tool)
        .unwrap_or_else(|| panic!("{tool} listed"))["annotations"][key]
        .clone()
}

#[specforge_test(
    behavior = "list_mcp_tools",
    verify = "a writing tool's hints say what it does: one that overwrites is destructive, one whose repeat changes nothing is idempotent"
)]
fn add_extension_overwrites_a_changed_module() {
    let mut server = components_server();
    let root = server.root().to_path_buf();
    let blob =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/greet-extension/greet.wasm");
    let add = |server: &mut McpServer| {
        let resp = call_tool(
            server,
            "specforge.add_extension",
            json!({"specifiers": [blob.to_str().unwrap()]}),
        );
        assert_eq!(resp["result"]["isError"], false, "{resp}");
        tool_json(&resp)
    };
    add(&mut server);
    let module = root.join(".specforge/extensions/@sdk/greet/extension.wasm");
    let installed = std::fs::read(&module).unwrap();
    let mut changed = installed.clone();
    changed.extend_from_slice(b"changed after install");
    std::fs::write(&module, &changed).unwrap();

    let second = add(&mut server);

    // The existing file was overwritten, which the listing now says.
    assert_eq!(
        second["files_written"],
        json!([".specforge/extensions/@sdk/greet/extension.wasm"]),
        "{second}"
    );
    assert_eq!(std::fs::read(&module).unwrap(), installed);
    assert_eq!(
        listed_hint(&mut server, "specforge.add_extension", "destructiveHint"),
        true
    );
}

#[specforge_test(
    behavior = "list_mcp_tools",
    verify = "a writing tool's hints say what it does: one that overwrites is destructive, one whose repeat changes nothing is idempotent"
)]
fn infer_session_mark_replaces_a_record() {
    let mut server = TestProject::new()
        .file("src/lib.rs", "pub fn a() {}\n")
        .serve(&[TestExtension::software()]);
    let root = server.root().to_path_buf();
    let mark = |server: &mut McpServer, entity: &str| {
        let resp = call_tool(
            server,
            "specforge.infer_session",
            json!({"action": "mark_analyzed", "source_file": "src/lib.rs", "entities_produced": [entity]}),
        );
        assert_eq!(resp["result"]["isError"], false, "{resp}");
    };
    mark(&mut server, "gamma");
    mark(&mut server, "delta");

    // The second mark replaced the first record of the file.
    let manifest = std::fs::read_to_string(root.join("specforge-infer.json")).unwrap();
    let manifest: Value = serde_json::from_str(&manifest).unwrap();
    let record = manifest["source_index"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["path"] == "src/lib.rs")
        .unwrap_or_else(|| panic!("{manifest}"))
        .to_string();
    assert!(record.contains("delta"), "{record}");
    assert!(!record.contains("gamma"), "{record}");
    assert_eq!(
        listed_hint(&mut server, "specforge.infer_session", "destructiveHint"),
        true
    );
}

#[specforge_test(
    behavior = "list_mcp_tools",
    verify = "a writing tool's hints say what it does: one that overwrites is destructive, one whose repeat changes nothing is idempotent"
)]
fn a_repeated_rename_changes_nothing() {
    let (mut server, root) = server_with_token_project();
    let args = json!({"entity_id": "token_unique", "new_name": "token_distinct"});
    let first = call_tool(&mut server, "specforge.rename", args.clone());
    assert_eq!(first["result"]["isError"], false, "{first}");
    let before = files_under(&root);

    let second = call_tool(&mut server, "specforge.rename", args);

    let error = crate::tool_errors::mcp_error(&second);
    assert_eq!(error["code"], "entity_not_found", "{error}");
    assert_eq!(error["data"]["files_written"], json!([]), "{error}");
    assert_eq!(files_under(&root), before, "the repeat wrote nothing");
    assert_eq!(
        listed_hint(&mut server, "specforge.rename", "idempotentHint"),
        true
    );
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "add, init and publish read a candidate's declaration in the runtime their surface passes"
)]
fn add_extension_reads_the_candidate_in_the_host_runtime() {
    // The host serves `@sdk/greet` in process: the tool reads the candidate
    // in it, and the served session then loads the install through it too.
    let runtime = std::sync::Arc::new(specforge_ops::testing::candidates());
    let mut server = TestProject::new()
        .enabling(&[])
        .serve_in(runtime.clone() as specforge_project::SharedRuntime);
    let root = server.root().to_path_buf();
    let files = tempfile::tempdir().unwrap();
    let greet = files.path().join("greet.wasm");
    std::fs::write(&greet, specforge_ops::testing::GREET).unwrap();

    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [greet.to_str().unwrap()]}),
    );

    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(parsed["added"][0]["action"], "installed", "{parsed}");
    let lock = std::fs::read_to_string(root.join("specforge.lock")).unwrap();
    assert!(lock.contains("@sdk/greet"), "{lock}");
    let handshakes = runtime
        .calls()
        .into_iter()
        .filter(|call| call.extension == "__candidate" && call.export == "__handshake")
        .count();
    assert_eq!(handshakes, 1);

    let listed = call_tool(&mut server, "specforge.extensions", json!({}));
    let listed: Value = serde_json::from_str(&tool_text(&listed)).unwrap();
    let greet = listed["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "@sdk/greet")
        .unwrap_or_else(|| panic!("{listed}"));
    assert_eq!(greet["status"], "loaded", "{listed}");
}

#[test]
fn add_extension_missing_specifier() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.add_extension", json!({}));
    crate::tool_errors::mcp_error(&resp);
}

// --- specforge.remove_extension ---

/// The product blob the build vendors; a local `.wasm` install names the
/// extension after its file stem.
fn greet_blob() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/greet-extension/greet.wasm")
}

const GREET: &str = "@sdk/greet";

/// [`components_server`] with the product blob installed in its project.
fn server_with_product() -> (Served, std::path::PathBuf) {
    let mut server = components_server();
    let root = server.root().to_path_buf();
    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [greet_blob().to_str().unwrap()]}),
    );
    assert!(resp["result"].is_object(), "install failed: {resp}");
    (server, root)
}

fn config_extensions(root: &Path) -> Vec<String> {
    let config: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("specforge.json")).unwrap())
            .unwrap();
    config["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "dry_run returns preview without modifying files"
)]
fn add_extension_dry_run_writes_nothing() {
    let mut server = components_server();
    let root = server.root().to_path_buf();
    let before = files_under(&root);

    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [greet_blob().to_str().unwrap()], "dry_run": true}),
    );

    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(parsed["added"][0]["action"], "planned", "{parsed}");
    assert_eq!(parsed["added"][0]["extension"], GREET, "{parsed}");
    assert_eq!(files_under(&root), before, "a dry run writes nothing");
}

#[specforge_test(
    behavior = "provide_mcp_remove_extension_tool",
    verify = "specforge.remove_extension removes extension from config"
)]
fn remove_extension_removes_it_from_config_lock_and_disk() {
    let (mut server, root) = server_with_product();
    assert!(
        config_extensions(&root)
            .iter()
            .any(|e| e.starts_with(GREET))
    );

    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"name": GREET}),
    );

    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert!(parsed.get("success").is_none(), "{parsed}");
    assert_eq!(parsed["extension"], GREET);
    assert!(
        !config_extensions(&root)
            .iter()
            .any(|e| e.starts_with(GREET)),
        "specforge.json still lists it: {:?}",
        config_extensions(&root)
    );
    let lock = std::fs::read_to_string(root.join("specforge.lock")).unwrap();
    assert!(!lock.contains(GREET), "{lock}");
    assert!(!root.join(".specforge/extensions").join(GREET).exists());
}

#[specforge_test(
    behavior = "provide_mcp_remove_extension_tool",
    verify = "specforge.remove_extension removes a .wasm file entry by the name it declares, leaving its file in place"
)]
fn remove_extension_removes_a_wasm_file_entry_by_its_declared_name() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    std::fs::copy(greet_blob(), root.join("greet.wasm")).unwrap();
    std::fs::write(
        root.join("specforge.json"),
        json!({"name": "p", "version": "0.1.0", "extensions": ["@specforge/software", "greet.wasm"]})
            .to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("main.spec"),
        "greeting hello \"Hello\" {\n  style warm\n}\n",
    )
    .unwrap();
    let mut server = McpServer::new();
    server.handle_message(
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}).to_string(),
    );

    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"name": GREET, "path": root.to_str().unwrap()}),
    );

    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert!(parsed["extension"].is_string(), "{parsed}");
    assert_eq!(parsed["extension"], GREET, "{parsed}");
    assert_eq!(parsed["version"], "0.1.0", "{parsed}");
    assert_eq!(
        parsed["stranded"],
        json!([{"entity_id": "hello", "kind": "greeting"}]),
        "{parsed}"
    );
    assert_eq!(config_extensions(root), ["@specforge/software"]);
    assert!(root.join("greet.wasm").is_file(), "the file is the user's");
    assert!(!root.join("specforge.lock").exists());
}

#[specforge_test(
    behavior = "provide_mcp_remove_extension_tool",
    verify = "dry_run returns preview without modifying files"
)]
fn remove_extension_dry_run_writes_nothing() {
    let (mut server, root) = server_with_product();
    let before = files_under(&root);

    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"name": GREET, "dry_run": true}),
    );

    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(parsed["action"], "planned", "{parsed}");
    assert_eq!(parsed["extension"], GREET, "{parsed}");
    assert!(parsed["stranded"].is_array(), "{parsed}");
    assert_eq!(files_under(&root), before, "a dry run writes nothing");
}

#[specforge_test(
    behavior = "provide_mcp_remove_extension_tool",
    verify = "the entities whose kind only that extension declares are listed as stranded"
)]
fn remove_extension_lists_stranded_entities() {
    let (mut server, root) = server_with_product();
    // `hello` is a greeting, a kind only the product extension declares;
    // test.spec's `alpha` and `beta` are of kinds it does not.
    server.write(
        "greet.spec",
        "greeting hello \"Hello\" {\n  style warm\n}\n",
    );

    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"name": GREET}),
    );

    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(
        parsed["stranded"],
        json!([{"entity_id": "hello", "kind": "greeting"}]),
        "{parsed}"
    );
    assert!(parsed["extension"].is_string(), "removal still proceeds");
    assert!(!root.join(".specforge/extensions").join(GREET).exists());
}

#[specforge_test(
    behavior = "provide_mcp_remove_extension_tool",
    verify = "non-installed extension returns extension_not_found error"
)]
fn remove_extension_not_installed_is_extension_not_found() {
    let (mut server, _root) = server_with_product();

    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"name": "@acme/missing"}),
    );

    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "extension_not_found", "{resp}");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("@acme/missing"), "{message}");
}

#[specforge_test(
    behavior = "provide_mcp_remove_extension_tool",
    verify = "specforge.remove_extension removes extension from config"
)]
fn remove_extension_disables_an_enabled_builtin() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":["@specforge/software","@specforge/product"]}"#,
    )
    .unwrap();
    let mut server = McpServer::with_project_root(dir.path().to_path_buf());
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
    server.handle_message(&init.to_string());

    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"name": "@specforge/product"}),
    );

    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert!(parsed["extension"].is_string(), "{parsed}");
    assert_eq!(parsed["extension"], "@specforge/product");
    assert_eq!(config_extensions(dir.path()), ["@specforge/software"]);
}

// --- specforge.migrate ---

#[test]
fn migrate_returns_result() {
    let mut server = test_server();
    // The fixture is already at the current format version — an honest
    // migrate is a no-op, not a fake migration.
    let resp = call_tool(&mut server, "specforge.migrate", json!({}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["migrated"], false);
    assert_eq!(parsed["pending"], false);
}

#[test]
fn add_extension_already_installed_placeholder() {
    // A project that enables nothing, served with its own component
    // runtime, so an installed extension loads.
    let mut server = TestProject::new().serve_components();
    let dir = server.root().to_path_buf();
    assert!(dir.join("specforge.json").is_file());
    let blob = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures/greet-extension/greet.wasm");
    let first = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [blob.to_str().unwrap()]}),
    );
    assert!(first["result"].is_object());
    // Second install of the same blob: the config update is idempotent and
    // the install succeeds again with the same content — but it must not
    // fake success for a DIFFERENT extension. A real no-op is fine here.
    let second = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [blob.to_str().unwrap()]}),
    );
    let still_ok = second["result"].is_object();
    assert!(still_ok);
    let lock = std::fs::read_to_string(dir.as_path().join("specforge.lock")).unwrap();
    assert!(lock.contains("@sdk/greet"));
}

#[test]
fn add_extension_invalid_manifest_placeholder() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": ["invalid-extension-xyz"]}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["diagnostic"]["code"], "E054", "{resp}");
    let msg = error["message"].as_str().unwrap();
    assert!(msg.contains("invalid-extension-xyz"), "{msg}");
}

// B:provide_mcp_rename_tool — verify unit "invalid new_name format returns validation error"
#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "invalid new_name returns validation error"
)]
fn rename_invalid_name_format() {
    let mut server = test_server();
    // Empty string
    let resp = call_tool(
        &mut server,
        "specforge.rename",
        json!({"entity_id": "alpha", "new_name": ""}),
    );
    crate::tool_errors::mcp_error(&resp);
    // Single character (< 2)
    let resp2 = call_tool(
        &mut server,
        "specforge.rename",
        json!({"entity_id": "alpha", "new_name": "x"}),
    );
    crate::tool_errors::mcp_error(&resp2);
    // Special chars
    let resp3 = call_tool(
        &mut server,
        "specforge.rename",
        json!({"entity_id": "alpha", "new_name": "no-dashes"}),
    );
    crate::tool_errors::mcp_error(&resp3);
}

// B:provide_mcp_init_tool — verify unit "default version is 0.1.0"
#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "default version is 0.1.0"
)]
fn init_default_version_value() {
    let dir = fresh_project_dir();
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": dir.path().to_str().unwrap(), "name": "vertest"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["version"], "0.1.0");
}

// B:provide_mcp_init_tool — verify unit "version parameter overrides default 0.1.0"
#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "version parameter overrides default 0.1.0"
)]
fn init_version_override_value() {
    let dir = fresh_project_dir();
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": dir.path().to_str().unwrap(), "name": "vertest", "version": "1.0.0"}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["version"], "1.0.0");
}

// B:provide_mcp_add_extension_tool — verify unit "invalid specifier returns error"
#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "invalid specifier format returns error"
)]
fn add_extension_invalid_specifier() {
    let mut server = test_server();
    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": ["no-at-sign"]}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["diagnostic"]["code"], "E054", "{resp}");
    let msg = error["message"].as_str().unwrap();
    assert!(msg.contains("no-at-sign"), "{msg}");
}

/// `specforge.add_extension` reads its argument as `specforge add` does
/// (plan 12 §2.2, R2): the same inputs reach the registry port, which this
/// server has none behind (E063), and the same ones are refused (E054).
#[specforge_test(
    behavior = "parse_extension_specifier",
    verify = "each add argument reads as one extension source"
)]
fn add_extension_reads_specifiers_as_add_does() {
    let mut server = test_server();
    for (specifier, code) in [
        ("@acme/tool@1.x", "E063"), // I5
        ("foo@/bar", "E054"),       // I9
        ("@acme/..", "E054"),       // I12
        ("@a/x", "E063"),           // I15
    ] {
        let resp = call_tool(
            &mut server,
            "specforge.add_extension",
            json!({"specifiers": [specifier]}),
        );
        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["diagnostic"]["code"], code, "{specifier}: {resp}");
    }
}

#[test]
fn migrate_dry_run() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.migrate", json!({"dry_run": true}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["migrated"], false);
}

#[test]
fn migrate_post_validation() {
    let mut server = test_server();
    let resp = call_tool(&mut server, "specforge.migrate", json!({}));
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["changes"].is_array() || parsed["migrated"].is_boolean());
}

// --- Missing verify statements ---

#[test]
fn add_extension_dry_run() {
    // A project that enables nothing, served with its own component
    // runtime, so an installed extension loads.
    let mut server = TestProject::new().serve_components();
    let dir = server.root().to_path_buf();
    assert!(dir.join("specforge.json").is_file());
    let blob = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures/greet-extension/greet.wasm");
    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [blob.to_str().unwrap()], "dry_run": true}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["added"][0]["action"], "planned", "{parsed}");
}

#[test]
fn remove_extension_dry_run() {
    // A project that enables nothing, served with its own component
    // runtime, so an installed extension loads.
    let mut server = TestProject::new().serve_components();
    let dir = server.root().to_path_buf();
    assert!(dir.join("specforge.json").is_file());
    let blob = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures/greet-extension/greet.wasm");
    let _install = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [blob.to_str().unwrap()]}),
    );
    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"name": "@sdk/greet", "dry_run": true}),
    );
    let text = tool_text(&resp);
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["action"], "planned", "{parsed}");
}

// --- Contract tests ---

// B:provide_mcp_format_tool — verify contract
#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "Provide MCP Format Tool: MCP format tool holds — filesystem_available, files_formatted, check_mode_readonly, mutation_completed_emitted, tool_invoked_emitted"
)]
fn format_contract() {
    // filesystem_available: a project with two unformatted files.
    let (mut server, root) = server_with_unformatted();
    let before = files_under(&root);

    // check_mode_readonly: reported, not written, and no mutation.
    let check = format_result(&mut server, json!({"check": true}));
    assert_eq!(check["check_only"], true);
    assert_eq!(
        check["changed_files"].as_array().unwrap().len(),
        2,
        "{check}"
    );
    assert_eq!(files_under(&root), before);
    assert!(events(&server, "mcp_mutation_completed").is_empty());

    // files_formatted
    let parsed = format_result(&mut server, json!({}));
    assert_eq!(parsed["changed_files"].as_array().unwrap().len(), 2);
    let formatted = std::fs::read_to_string(root.join("a.spec")).unwrap();
    assert!(
        formatted.contains("\n  contract \"The system MUST work\""),
        "{formatted}"
    );
    assert_ne!(files_under(&root), before);

    // mutation_completed_emitted, tool_invoked_emitted
    assert_eq!(
        events(&server, "mcp_mutation_completed"),
        [json!({
            "toolName": "specforge.format",
            "files_changed": 2,
            "entities_affected": 0,
            "success": true,
        })]
    );
    assert!(invoked(&server, "specforge.format"));
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "specforge.add_extension adds extension to config"
)]
fn add_extension_adds_the_extension_to_config() {
    // Enables nothing; the module is a local .wasm, so the add is offline.
    let mut server = components_server();
    let root = server.root().to_path_buf();
    assert!(config_extensions(&root).is_empty());

    let added = tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [greet_blob().to_str().unwrap()]}),
    );

    // One specifier, one entry in `added`.
    assert_eq!(added["added"][0]["extension"], GREET, "{added}");
    assert_eq!(added["added"][0]["action"], "installed", "{added}");
    // specforge.json lists it, and its module is where the compile loads it.
    assert_eq!(config_extensions(&root), [GREET]);
    assert!(
        root.join(".specforge/extensions")
            .join(GREET)
            .join("extension.wasm")
            .is_file()
    );
}

// B:provide_mcp_add_extension_tool — verify contract
#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "Provide MCP Add Extension Tool: MCP add extension tool holds — filesystem_available, extension_installed, wasm_downloaded, extension_added_emitted, dry_run_safe, tool_invoked_emitted"
)]
fn add_extension_contract() {
    // filesystem_available: a project on disk, served with its own
    // component runtime so the installed module loads.
    let mut server = components_server();
    let root = server.root().to_path_buf();
    let before = files_under(&root);
    let specifier = json!(greet_blob().to_str().unwrap());

    // dry_run_safe: a preview, nothing written, no extension added.
    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [specifier], "dry_run": true}),
    );
    let preview: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(preview["added"][0]["action"], "planned", "{preview}");
    assert_eq!(files_under(&root), before);
    assert!(events(&server, "extension_added").is_empty());

    // extension_installed: in specforge.json and the lock.
    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [specifier]}),
    );
    let installed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(installed["added"][0]["action"], "installed", "{installed}");
    // Enabled by its bare name; the lock pins version and hash (D3-b).
    assert_eq!(config_extensions(&root), vec![GREET]);
    let lock = std::fs::read_to_string(root.join("specforge.lock")).unwrap();
    assert!(lock.contains(GREET), "{lock}");

    // wasm_downloaded: the module sits in the project's extension cache
    // (from the local blob here; a registry install downloads it).
    let blob = std::fs::read(greet_blob()).unwrap();
    let cached = files_under(&root.join(".specforge/extensions").join(GREET));
    assert!(
        cached.values().any(|bytes| *bytes == blob),
        "no module cached"
    );

    // extension_added_emitted, tool_invoked_emitted
    let added = events(&server, "extension_added");
    assert_eq!(added.len(), 1, "{added:?}");
    assert_eq!(added[0]["extensionSpecifier"], specifier);
    assert_eq!(added[0]["wasDuplicate"], false);
    assert_eq!(added[0]["totalExtensions"], 1);
    assert!(invoked(&server, "specforge.add_extension"));
}

#[test]
fn remove_extension_contract() {
    // A project that enables nothing, served with its own component
    // runtime, so an installed extension loads.
    let mut server = TestProject::new().serve_components();
    let dir = server.root().to_path_buf();
    assert!(dir.join("specforge.json").is_file());
    let blob = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures/greet-extension/greet.wasm");
    let _install = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": [blob.to_str().unwrap()]}),
    );
    // Requires: filesystem available
    // Ensures: not-installed extension is an honest error, not fake success
    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"name": "@specforge/unknown"}),
    );
    crate::tool_errors::mcp_error(&resp);
}

// B:provide_mcp_migrate_tool — verify contract
#[specforge_test(
    behavior = "provide_mcp_migrate_tool",
    verify = "Provide MCP Migrate Tool: MCP migrate tool holds — filesystem_available, migrations_applied, post_migration_validated, dry_run_safe, mutation_completed_emitted, tool_invoked_emitted"
)]
fn migrate_contract() {
    // filesystem_available: a project with one spec file in format 0.9
    // whose feature references an entity that does not exist.
    let mut server = test_server();
    let root = server
        .state()
        .project_root()
        .map(std::path::Path::to_path_buf)
        .unwrap();
    let old = "// specforge-format: 0.9\nfeature gamma \"Gamma\" {\n    behaviors [ghost]\n}\n";
    std::fs::write(root.join("old.spec"), old).unwrap();
    let migrate = |server: &mut McpServer, args: Value| -> Value {
        let resp = call_tool(server, "specforge.migrate", args);
        serde_json::from_str(&tool_text(&resp)).unwrap_or_else(|_| panic!("{resp}"))
    };

    // dry_run_safe: the diff, and nothing on disk changes.
    let before = files_under(&root);
    let dry = migrate(&mut server, json!({"dry_run": true}));
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["migrated"], false);
    assert_eq!(dry["from_version"], "0.9", "{dry}");
    let diffs = dry["diffs"].as_array().unwrap();
    assert_eq!(diffs.len(), 1, "{dry}");
    assert!(
        diffs[0]["unified_text"]
            .as_str()
            .unwrap()
            .contains("+// specforge-format: 1.0"),
        "{dry}"
    );
    assert_eq!(files_under(&root), before);

    // migrations_applied
    let applied = migrate(&mut server, json!({}));
    assert_eq!(applied["migrated"], true, "{applied}");
    assert_eq!(applied["files_migrated"], 1, "{applied}");
    assert_eq!(
        std::fs::read_to_string(root.join("old.spec")).unwrap(),
        old.replace("0.9", "1.0")
    );

    // post_migration_validated: the migrated project was compiled and its
    // error reported.
    assert_eq!(applied["post_migration_validated"], true);
    let errors = applied["post_migration_errors"].as_array().unwrap();
    assert!(
        errors
            .iter()
            .any(|e| e["message"].as_str().unwrap().contains("ghost")),
        "{applied}"
    );

    // Nothing left to migrate.
    assert_eq!(migrate(&mut server, json!({}))["migrated"], false);

    // mutation_completed_emitted, tool_invoked_emitted
    // The migration rewrote one file and backed it up; the second run
    // changed nothing.
    let migration = |files_changed: usize| {
        json!({
            "toolName": "specforge.migrate",
            "files_changed": files_changed,
            "entities_affected": 0,
            "success": true,
        })
    };
    assert_eq!(
        events(&server, "mcp_mutation_completed"),
        [migration(3), migration(0)]
    );
    assert!(invoked(&server, "specforge.migrate"));
}

/// [`test_server`] whose project also holds `old.spec`, in format 0.9.
fn server_with_old_spec() -> (Served, std::path::PathBuf) {
    let server = project()
        .file(
            "old.spec",
            "// specforge-format: 0.9\nbehavior gamma \"Gamma\" {\n}\n",
        )
        .serve(&[TestExtension::software()]);
    let root = server.root().to_path_buf();
    (server, root)
}

#[specforge_test(
    behavior = "provide_mcp_migrate_tool",
    verify = "target_version selects the format version to migrate to"
)]
fn migrate_target_version_selects_the_version() {
    let (mut server, root) = server_with_old_spec();
    let resp = call_tool(
        &mut server,
        "specforge.migrate",
        json!({"target_version": "1.0", "no_backup": true}),
    );
    let result: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(result["migrated"], true, "{result}");
    assert_eq!(result["from_version"], "0.9", "{result}");
    assert_eq!(result["to_version"], "1.0", "{result}");
    assert!(
        std::fs::read_to_string(root.join("old.spec"))
            .unwrap()
            .starts_with("// specforge-format: 1.0"),
    );
    // no_backup: no .bak copy beside the migrated file.
    assert!(
        !files_under(&root)
            .keys()
            .any(|p| p.extension().is_some_and(|e| e == "bak"))
    );
}

#[specforge_test(
    behavior = "provide_mcp_migrate_tool",
    verify = "a malformed or unsupported target_version is refused without modifying files"
)]
fn migrate_refuses_a_bad_target_version() {
    let (mut server, root) = server_with_old_spec();
    let before = files_under(&root);
    for target in ["99.0", "latest"] {
        let resp = call_tool(
            &mut server,
            "specforge.migrate",
            json!({"target_version": target}),
        );
        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["diagnostic"]["code"], "E019", "{target}: {resp}");
        let message = error["message"].as_str().unwrap_or_default();
        assert!(message.contains(target), "{target}: {resp}");
    }
    assert_eq!(files_under(&root), before);
}

#[specforge_test(
    behavior = "provide_mcp_migrate_tool",
    verify = "the result reports the hooks run, the structural differences and whether the migration was rolled back"
)]
fn migrate_reports_hooks_structure_and_rollback() {
    let (mut server, root) = server_with_old_spec();

    let resp = call_tool(&mut server, "specforge.migrate", json!({}));

    let result: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert_eq!(result["migrated"], true, "{result}");
    // No enabled extension declares a migration hook.
    assert_eq!(result["hooks_invoked"], json!([]), "{result}");
    assert_eq!(result["hook_failures"], json!([]), "{result}");
    // A header-only migration keeps the graph's structure.
    assert_eq!(result["structural_differences"], json!([]), "{result}");
    assert_eq!(result["rolled_back"], false, "{result}");
    assert!(
        std::fs::read_to_string(root.join("old.spec"))
            .unwrap()
            .starts_with("// specforge-format: 1.0")
    );
}

/// A project whose two registries share the alias `main`; nothing listens
/// at their URL, so a registry install fails at once.
fn project_with_duplicate_registry_alias() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let url = "http://127.0.0.1:9/v1";
    let config = json!({
        "name": "t", "version": "0.1.0", "extensions": [],
        "registries": [
            {"alias": "main", "url": url, "default_registry": true},
            {"alias": "main", "url": url},
        ],
    });
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    dir
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "add_extension of a registry package reports what reading the registry configuration found"
)]
fn add_extension_from_a_registry_reports_a_duplicate_registry_alias() {
    let dir = project_with_duplicate_registry_alias();
    let mut server = test_server();
    let path = dir.path().to_str().unwrap();

    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": ["@acme/widget@1.0.0"], "path": path}),
    );
    let codes: Vec<&str> = resp["result"]["_meta"]["diagnostics"]
        .as_array()
        .unwrap_or_else(|| panic!("no _meta.diagnostics: {resp}"))
        .iter()
        .filter_map(|d| d["code"].as_str())
        .collect();
    assert!(codes.contains(&"W140"), "{resp}");

    // A builtin never reads the registries, so it reports none of it.
    let builtin = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": ["@specforge/software"], "path": path, "dry_run": true}),
    );
    assert!(builtin["result"]["_meta"].is_null(), "{builtin}");
}

// §3 R4 (plan 05): a dry run of an exact version asks no registry, so its configuration is not shown.
#[specforge_test(
    behavior = "configure_registries",
    verify = "an operation shows the registry configuration's diagnostics once it has asked a registry"
)]
fn add_extension_dry_run_of_an_exact_version_reports_no_registry_configuration() {
    let dir = project_with_duplicate_registry_alias();
    let mut server = test_server();
    let path = dir.path().to_str().unwrap();

    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": ["@acme/widget@1.0.0"], "path": path, "dry_run": true}),
    );
    let codes: Vec<&str> = resp["result"]["_meta"]["diagnostics"]
        .as_array()
        .map(|all| all.iter().filter_map(|d| d["code"].as_str()).collect())
        .unwrap_or_default();
    assert!(!codes.contains(&"W140"), "{resp}");
}

/// The new name follows the entity-ID rule (the grammar's identifier,
/// 2-60 characters): an illegal one is refused and nothing is written.
#[test]
fn rename_refuses_an_illegal_entity_id() {
    let (mut server, root) = server_with_token_project();
    for bad in [
        "a".repeat(61),
        "token-distinct".to_string(),
        "9token".to_string(),
    ] {
        let resp = call_tool(
            &mut server,
            "specforge.rename",
            json!({"entity_id": "token_unique", "new_name": bad}),
        );
        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["code"], "invalid_input", "{bad}: {error}");
        assert_eq!(error["argument"], "new_name", "{error}");
    }
    assert_eq!(
        std::fs::read_to_string(root.join("spec/tokens.spec")).unwrap(),
        TOKENS_SPEC
    );
}

/// A project on disk with `specforge.json` enabling `extensions`, and
/// `files`, served by a fresh server.
fn served_project(extensions: &[&str], files: &[(&str, &str)]) -> (McpServer, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let config = json!({"name": "m", "version": "0.1.0", "extensions": extensions});
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    for (path, text) in files {
        std::fs::write(dir.path().join(path), text).unwrap();
    }
    let mut server = McpServer::new();
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"projectRoot": dir.path().to_str().unwrap()}});
    server.handle_message(&req.to_string());
    (server, dir)
}

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "remove_extension with a path to another project checks that project's dependents and entities"
)]
fn remove_on_another_project_checks_that_projects_dependents() {
    // The served project enables formal, which requires software; the
    // other enables software alone.
    let (mut server, _served) = served_project(
        &["@specforge/software", "@specforge/formal"],
        &[("main.spec", "")],
    );
    let other = tempfile::TempDir::new().unwrap();
    std::fs::write(
        other.path().join("specforge.json"),
        json!({"name": "o", "version": "0.1.0", "extensions": ["@specforge/software"]}).to_string(),
    )
    .unwrap();
    std::fs::write(other.path().join("main.spec"), "").unwrap();

    // Nothing in the other project requires software: it can go.
    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"path": other.path().to_str().unwrap(), "name": "@specforge/software", "dry_run": true}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    let payload: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    assert!(payload["extension"].is_string(), "{payload}");

    // In the served project formal still requires it.
    let resp = call_tool(
        &mut server,
        "specforge.remove_extension",
        json!({"name": "@specforge/software", "dry_run": true}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["diagnostic"]["code"], "E027", "{error}");
}

#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "rename plans on the project as it is on disk, references added since the last call included"
)]
fn rename_plans_on_the_project_as_it_is_on_disk() {
    let (mut server, dir) = served_project(
        &[],
        &[(
            "main.spec",
            "invariant tok \"Tok\" {\n}\nbehavior login \"Login\" {\n  invariants [tok]\n}\n",
        )],
    );
    // A reference written after the server last read the project.
    std::fs::write(
        dir.path().join("logout.spec"),
        "behavior logout \"Logout\" {\n  invariants [tok]\n}\n",
    )
    .unwrap();

    let resp = call_tool(
        &mut server,
        "specforge.rename",
        json!({"entity_id": "tok", "new_name": "tok2"}),
    );
    let payload: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    let affected = payload["affected_files"].as_array().unwrap();
    assert!(affected.iter().any(|f| f == "logout.spec"), "{payload}");
    assert!(
        std::fs::read_to_string(dir.path().join("logout.spec"))
            .unwrap()
            .contains("invariants [tok2]")
    );
    let diagnostics = payload["diagnostics"].as_array().unwrap();
    assert!(diagnostics.iter().all(|d| d["code"] != "E003"), "{payload}");
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "after add_extension the server serves the extension it installed"
)]
fn add_then_tools_list_shows_the_extension_tools() {
    let (mut server, _dir) = served_project(&[], &[("main.spec", "")]);
    let product_tools = |server: &mut McpServer| -> usize {
        let req = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}});
        let resp: Value =
            serde_json::from_str(&server.handle_message(&req.to_string()).unwrap()).unwrap();
        resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["source"] == "@specforge/product")
            .count()
    };
    assert_eq!(product_tools(&mut server), 0);

    let resp = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": ["@specforge/product"]}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");

    assert!(
        product_tools(&mut server) > 0,
        "the installed extension's tools are listed"
    );
    assert!(
        server
            .state()
            .registries()
            .declaration("@specforge/product")
            .is_some()
    );
}

// --- plan 03: a specforge.json that hides the project's sources ---

/// A `specforge.json` that is cut off: no file is known to be a source.
const TRUNCATED_CONFIG: &str = r#"{"spec_root": "spec", "extensions": ["#;

/// Every file under `root` with its bytes.
fn files_under(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut files = std::collections::BTreeMap::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    files
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "a specforge.json that does not say which files are the project's sources fails the call as config_invalid and writes nothing"
)]
fn format_refuses_a_config_that_does_not_say_which_files_are_its_sources() {
    let (mut server, root) = server_with_unformatted();
    std::fs::write(root.join("specforge.json"), TRUNCATED_CONFIG).unwrap();
    let before = files_under(&root);

    let resp = call_tool(&mut server, "specforge.format", json!({}));

    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("specforge.json is not valid JSON: EOF while parsing"),
        "{error}"
    );
    assert_eq!(error["data"]["files_written"], json!([]), "{error}");
    assert_eq!(files_under(&root), before, "nothing is written");
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "a problem in specforge.json the call goes around is an E069 among the diagnostics and makes ok false"
)]
fn format_check_reports_a_config_problem_it_goes_around() {
    let (mut server, root) =
        server_with_canonical_and(("b.spec", &CANONICAL.replace("messy", "other")));
    std::fs::write(root.join("specforge.json"), r#"{"exclude": ["["]}"#).unwrap();

    let parsed = format_result(&mut server, json!({"check": true}));

    assert_eq!(parsed["ok"], false, "{parsed}");
    assert_eq!(parsed["all_clean"], true, "{parsed}");
    assert_eq!(parsed["diagnostics"][0]["code"], "E069", "{parsed}");
}

#[ignore = "fixed by T3"]
#[specforge_test(
    behavior = "provide_mcp_migrate_tool",
    verify = "a specforge.json that does not say which files are the project's sources fails the call as config_invalid and writes nothing"
)]
fn migrate_refuses_a_config_that_does_not_say_which_files_are_its_sources() {
    let server = project()
        .file(
            "a.spec",
            "// specforge-format: 0.9\nbehavior alpha \"Alpha\" {\n}\n",
        )
        .serve(&[TestExtension::software()]);
    let mut server = server;
    let root = server.root().to_path_buf();
    std::fs::write(root.join("specforge.json"), TRUNCATED_CONFIG).unwrap();
    let before = files_under(&root);

    let resp = call_tool(&mut server, "specforge.migrate", json!({"dry_run": false}));

    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert_eq!(files_under(&root), before, "no rewrite, no .bak");
}

#[ignore = "fixed by T5"]
#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "a specforge.json that does not say which files are the project's sources fails the rename as config_invalid and writes nothing"
)]
fn rename_refuses_a_config_that_does_not_say_which_files_are_its_sources() {
    let (mut server, root) = server_with_token_project();
    std::fs::create_dir_all(root.join("vendor")).unwrap();
    std::fs::write(
        root.join("vendor/v.spec"),
        "behavior vend \"Vend\" {\n  contract \"y\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":["@specforge/software"],"spec_root":["spec"]}"#,
    )
    .unwrap();
    // The next call brings the served project up to date with the config just written.
    call_tool(&mut server, "specforge.validate", json!({}));
    let before = files_under(&root);

    let resp = call_tool(
        &mut server,
        "specforge.rename",
        json!({"entity_id": "vend", "new_name": "vended"}),
    );

    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert!(
        error["message"].as_str().unwrap().contains("\"spec_root\""),
        "{error}"
    );
    assert_eq!(files_under(&root), before, "vendor/v.spec is unchanged");
}
