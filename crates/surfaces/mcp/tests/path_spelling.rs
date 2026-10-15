//! A reply names each file an operation opened or wrote from the project root, however the call spelled it
//! (ADR 0102).

use crate::support::*;
use serde_json::{Value, json};
use specforge_test_macros::test as specforge_test;

const UNFORMATTED: &str = "behavior messy \"Messy\" {\ncontract \"The system MUST work\"\n}\n";

/// The unformatted `a.spec` and `b.spec`, served.
fn server_with_unformatted() -> Served {
    TestProject::new()
        .file("a.spec", UNFORMATTED)
        .file("b.spec", UNFORMATTED.replace("messy", "other"))
        .serve(&[TestExtension::software()])
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "the format document names each file from the project root"
)]
fn format_reply_names_files_from_the_project_root() {
    let mut server = server_with_unformatted();
    let absolute = server.root().join("a.spec");

    let everything = tool(&mut server, "specforge.format", json!({"diff": true}));
    assert_eq!(
        everything["changed_files"],
        json!(["a.spec", "b.spec"]),
        "{everything}"
    );
    assert_eq!(
        everything["diffs"][0]["file_path"], "a.spec",
        "{everything}"
    );

    for arguments in [
        json!({"paths": ["a.spec"]}),
        json!({"paths": [absolute.to_str().unwrap()]}),
    ] {
        let reply = tool(&mut server, "specforge.format", arguments.clone());
        assert_eq!(reply["changed_files"], json!(["a.spec"]), "{arguments}");
        assert_eq!(reply["changed_files"], reply["files_written"], "{reply}");
        // The file is now canonical: put it back for the next call.
        std::fs::write(&absolute, UNFORMATTED).unwrap();
    }
}

#[specforge_test(
    behavior = "provide_mcp_render_tool",
    verify = "the render document names each output file from the project root"
)]
fn render_output_files_are_named_from_the_project_root() {
    let mut server = TestProject::new()
        .file("test.spec", "behavior alpha \"Alpha\" {\n}\n")
        .serve(&[TestExtension::software()]);

    let reply: Value = tool(
        &mut server,
        "specforge.render",
        json!({"format": "json", "out_dir": "out"}),
    );

    assert_eq!(reply["output_files"], json!(["out/graph.json"]), "{reply}");
}

#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "the collect document names the report from the project root"
)]
fn collect_reply_names_the_report_from_the_project_root() {
    let mut server = TestProject::new()
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
            json!({"entries": [
                {"entity_id": "alpha", "test_name": "works", "status": "passed"}
            ]})
            .to_string(),
        )
        .serve_components();

    let reply = tool(&mut server, "specforge.collect", json!({}));

    assert_eq!(reply["report"], "specforge-report.json", "{reply}");
}
