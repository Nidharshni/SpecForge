use crate::support::*;
use serde_json::json;
use specforge_mcp::McpServer;
use specforge_test::prelude::*;
use std::fs;
use tempfile::TempDir;

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "with nothing served, init refuses a directory whose files an enclosing project reads, as a conflict on path, before anything is written"
)]
#[ignore = "flipped by T3"]
fn init_with_nothing_served_refuses_a_directory_an_enclosing_project_reads() {
    let scratch = TempDir::new().unwrap();
    let workspace = scratch.path().join("workspace");
    let sub = workspace.join("outer/spec/sub");
    fs::create_dir_all(&sub).unwrap();
    fs::write(
        workspace.join("outer/specforge.json"),
        r#"{"name":"outer","version":"0.1.0","extensions":[],"spec_root":"spec"}"#,
    )
    .unwrap();
    fs::write(sub.join("t.spec"), "// t\n").unwrap();
    let mut server = McpServer::with_project_root(workspace);
    let reply = call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");

    let refused = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": "outer/spec/sub", "name": "subproj"}),
    );

    let error = crate::tool_errors::mcp_error(&refused);
    assert_eq!(error["code"], "conflict", "{error}");
    assert_eq!(error["argument"], "path", "{error}");
    assert!(!sub.join("specforge.json").exists());
    assert!(server.state().project_root().is_none());
}
