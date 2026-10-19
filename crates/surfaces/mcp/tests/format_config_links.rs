//! The format configuration a call reads, and the files a format run reads, stay inside the project
//! (ADR 0169). Over MCP a `.specforgefmt.toml` that is a link to a file outside, or has more than one
//! name, is a W141 naming why (never its content) and the defaults apply; a bounded format run
//! reads no file with more than one name.

use serde_json::json;
use specforge_mcp::testing::{Served, rooted_server, rpc};
use specforge_project::testing::TestProject;
use std::fs;
use tempfile::TempDir;

/// A project with one behavior in `spec/main.spec`.
fn project() -> TestProject {
    TestProject::new().enabling(&["@specforge/software"]).file(
        "spec/main.spec",
        "behavior alpha \"Alpha\" {\n  category query\n}\n",
    )
}

/// A file outside every project, holding `text`.
fn outside(text: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("outside.toml");
    fs::write(&file, text).unwrap();
    (dir, file)
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "no format configuration a call reads leaves the project, and none with more than one name is read"
)]
fn format_reports_a_configuration_linked_outside_without_its_content() {
    let project = project();
    let (_keep, file) = outside("SECRET-TOKEN = = oops\n");
    std::os::unix::fs::symlink(&file, project.root().join(".specforgefmt.toml")).unwrap();
    let mut served = Served::rooted(project);

    let reply = served.tool("specforge.format", json!({"check": true}));

    assert!(!reply.to_string().contains("SECRET-TOKEN"), "{reply}");
    let diagnostics = reply["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{reply}");
    assert_eq!(diagnostics[0]["code"], "W141", "{reply}");
    let message = diagnostics[0]["message"].as_str().unwrap();
    assert!(message.contains("outside"), "{message}");
    assert!(message.contains(file.to_str().unwrap()), "{message}");
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "no format configuration a call reads leaves the project, and none with more than one name is read"
)]
fn format_reports_a_hard_linked_configuration_without_its_content() {
    let project = project();
    let (_keep, file) = outside("SECRET-TOKEN = = oops\n");
    fs::hard_link(&file, project.root().join(".specforgefmt.toml")).unwrap();
    let mut served = Served::rooted(project);

    let reply = served.tool("specforge.format", json!({"check": true}));

    assert!(!reply.to_string().contains("SECRET-TOKEN"), "{reply}");
    let diagnostics = reply["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{reply}");
    assert_eq!(diagnostics[0]["code"], "W141", "{reply}");
    let message = diagnostics[0]["message"].as_str().unwrap();
    assert!(message.contains("2 names"), "{message}");
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "no format configuration a call reads leaves the project, and none with more than one name is read"
)]
fn edit_lays_a_block_out_with_the_defaults_when_the_configuration_links_outside() {
    let project = project();
    let (_keep, file) = outside("indent_width = 8\n");
    std::os::unix::fs::symlink(&file, project.root().join(".specforgefmt.toml")).unwrap();
    let mut served = Served::rooted(project);

    let reply = served.tool(
        "specforge.edit",
        json!({"edits": [{"op": "set", "entity_id": "alpha", "field": "category", "value": "command"}], "dry_run": true}),
    );

    let diff = reply["diff"].as_str().unwrap();
    assert!(diff.contains("+  category command"), "{diff}");
    assert!(!diff.contains("+        category"), "{diff}");
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "no format configuration a call reads leaves the project, and none with more than one name is read"
)]
fn init_formats_its_starter_with_the_defaults_when_the_configuration_links_outside() {
    let workspace = TestProject::new().without_config();
    fs::create_dir_all(workspace.root().join("created")).unwrap();
    let (_keep, file) = outside("indent_width = 8\n");
    std::os::unix::fs::symlink(&file, workspace.root().join("created/.specforgefmt.toml")).unwrap();
    // `Served` needs a project; the workspace init creates one in is a plain directory.
    let mut server = rooted_server(workspace.root());
    let reply = rpc::call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");

    let reply = rpc::call_tool(&mut server, "specforge.init", json!({"path": "created"}));

    assert!(reply["result"]["isError"] != true, "{reply}");
    let starter = fs::read_to_string(workspace.root().join("created/spec/hello.spec")).unwrap();
    assert!(starter.contains("\n  version"), "{starter}");
    assert!(!starter.contains("\n        version"), "{starter}");
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "format reads no file with more than one name: it is a failed file, named, without its content"
)]
fn format_names_a_hard_linked_source_a_failed_file_without_its_content() {
    let project = project();
    let (_keep, file) = outside("spec \"p\" {\n      version \"SECRET-SRC\"\n}\n");
    fs::hard_link(&file, project.root().join("spec/leak.spec")).unwrap();
    let mut served = Served::rooted(project);

    let resp = served.call_tool("specforge.format", json!({"diff": true}));

    assert!(!resp.to_string().contains("SECRET-SRC"), "{resp}");
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(
        error["data"]["failed_files"],
        json!(["spec/leak.spec"]),
        "{error}"
    );
}
