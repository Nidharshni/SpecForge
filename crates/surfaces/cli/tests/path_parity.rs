//! One run prints the same bytes on both surfaces (ADR 0102): the documents are compared with no redaction of the
//! root, the CLI reaching its project through a symlink from a third directory and MCP serving its project
//! canonically.

use crate::support::TestProject;
use serde_json::{Value, json};
use specforge_mcp::testing::{rooted_server, rpc};
use specforge_test_macros::test as specforge_test;
use std::fs;

/// A cargo-test project with a report on disk, one proved entity and one unformatted file.
fn project() -> TestProject {
    let project = TestProject::new().raw_config(
        r#"{"name":"col","spec_root":"spec","extensions":["@specforge/software","@specforge/testing","@specforge/cargo-test"]}"#,
    );
    let root = project.root();
    fs::create_dir_all(root.join("spec")).unwrap();
    fs::create_dir_all(root.join("target/specforge")).unwrap();
    fs::write(
        root.join("spec/a.spec"),
        "behavior alpha \"Alpha\" {\n  verify unit \"works\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("spec/messy.spec"),
        "behavior messy \"Messy\" {\ncategory core\ncontract \"The system MUST work\"\n}\n",
    )
    .unwrap();
    fs::write(root.join("Cargo.toml"), "").unwrap();
    fs::write(
        root.join("target/specforge/demo.json"),
        json!({"entries": [
            {"entity_id": "alpha", "test_name": "works", "verify": "works", "status": "passed"},
            {"entity_id": "omega", "test_name": "lost", "status": "passed"}
        ]})
        .to_string(),
    )
    .unwrap();
    project
}

/// What `tool` answered on a server over `project`'s canonical root.
fn mcp_document(project: &TestProject, tool: &str, arguments: Value) -> Value {
    let mut server = rooted_server(&fs::canonicalize(project.root()).unwrap());
    rpc::call(&mut server, "initialize", json!({}));
    let resp = rpc::call_tool(&mut server, tool, arguments);
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{resp}"));
    serde_json::from_str(text).unwrap()
}

#[cfg(unix)]
#[specforge_test(
    invariant = "document_project_paths",
    verify = "the CLI and MCP documents of one run are byte-equal, whatever the working directory or root spelling"
)]
fn one_run_prints_the_same_bytes_on_both_surfaces() {
    let (cli_project, mcp_project) = (project(), project());
    let elsewhere = tempfile::TempDir::new().unwrap();
    let link = elsewhere.path().join("link");
    std::os::unix::fs::symlink(cli_project.root(), &link).unwrap();
    let cli = |args: &[&str]| {
        let mut args = args.to_vec();
        args.extend(["--path", link.to_str().unwrap(), "--format", "json"]);
        crate::support::Cli::on(&cli_project)
            .in_dir(elsewhere.path())
            .bare(&args)
            .json()
    };

    let cli_format = cli(&["format", "--check"]);
    let mcp_format = mcp_document(&mcp_project, "specforge.format", json!({"check": true}));
    assert_eq!(cli_format, mcp_format);

    let cli_collect = cli(&["collect", "--no-run"]);
    let mcp_collect = mcp_document(&mcp_project, "specforge.collect", json!({}));
    assert_eq!(cli_collect, mcp_collect);
}
