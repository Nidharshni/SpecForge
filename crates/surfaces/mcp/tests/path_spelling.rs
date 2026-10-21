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

const LIMIT: &str = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";
const LOGOUT: &str = "behavior logout \"Logout\" {\n  invariants [sesion_limit]\n}\n";

/// Files named by a request are read into their source key (ADR 0195).
fn named_files() -> Served {
    TestProject::new()
        .enabling(&["@specforge/software"])
        .config(|c| c["spec_root"] = json!("spec"))
        .file("spec/limit.spec", LIMIT)
        .file("spec/login.spec", LOGOUT)
        .serve_components()
}

#[test]
#[ignore = "flipped by T3"]
fn edit_reads_two_spellings_of_one_new_file_as_one_file() {
    let mut server = named_files();
    let create = |id: &str, file: &str| json!({"op": "create", "kind": "type", "id": id, "title": id, "file": file});
    let reply = tool(
        &mut server,
        "specforge.edit",
        json!({"edits": [create("alpha", "./n.spec"), create("beta", "n.spec")]}),
    );
    assert_eq!(reply["affected_files"], json!(["n.spec"]), "{reply}");
    let written = std::fs::read_to_string(server.root().join("spec/n.spec")).unwrap();
    assert!(
        written.contains("type alpha") && written.contains("type beta"),
        "{written}"
    );
}

#[test]
#[ignore = "flipped by T4"]
fn outline_reads_an_absolute_spelling_of_a_source() {
    let mut server = named_files();
    let absolute = server.root().join("spec/login.spec");
    let by_key = tool(
        &mut server,
        "specforge.outline",
        json!({"file": "login.spec"}),
    );
    let by_path = tool(
        &mut server,
        "specforge.outline",
        json!({"file": absolute.to_str().unwrap()}),
    );
    assert_eq!(by_path, by_key);
}

#[test]
#[ignore = "flipped by T5"]
fn suggest_fixes_reads_a_spelling_of_its_file() {
    let mut server = named_files();
    let absolute = server.root().join("spec/login.spec");
    let fixes = |server: &mut Served, file: &str| {
        tool(
            server,
            "specforge.suggest_fixes",
            json!({"file_path": file}),
        )
    };
    let by_key = fixes(&mut server, "login.spec");
    assert_eq!(by_key["fixes"].as_array().unwrap().len(), 2, "{by_key}");
    assert_eq!(fixes(&mut server, "./login.spec"), by_key);
    assert_eq!(fixes(&mut server, absolute.to_str().unwrap()), by_key);
}
