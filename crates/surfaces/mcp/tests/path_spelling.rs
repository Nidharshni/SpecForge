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

#[specforge_test_macros::test(
    behavior = "provide_mcp_edit_tool",
    verify = "specforge.edit writes two spellings of one new file as one file"
)]
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

#[specforge_test_macros::test(
    behavior = "provide_mcp_edit_tool",
    verify = "specforge.edit refuses a new file named from the project root, naming its spec-root spelling"
)]
fn edit_refuses_a_new_file_named_from_the_project_root() {
    let mut server = named_files();
    let reply = call_tool(
        &mut server,
        "specforge.edit",
        json!({"edits": [{"op": "create", "kind": "type", "id": "alpha", "title": "alpha", "file": "spec/n.spec"}]}),
    );
    let error = crate::tool_errors::mcp_error(&reply);
    assert_eq!(error["file"], "spec/n.spec", "{error}");
    assert_eq!(
        error["data"]["suggestion"],
        "did you mean 'n.spec'? a spec file is named from the spec root",
        "{error}"
    );
    assert!(!server.root().join("spec/spec").exists());
}

#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "outline reads every spelling of a source under the spec root as that source"
)]
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

#[specforge_test_macros::test(
    behavior = "provide_mcp_suggest_fixes_tool",
    verify = "file_path reads every spelling of a source as that source"
)]
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

#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "outline refuses a project-root spelling of a source naming its spec-root spelling"
)]
fn outline_refuses_a_project_root_spelling_as_the_view_does() {
    let mut server = named_files();
    let refused = |server: &mut Served, tool: &str, argument: &str| {
        let mut arguments = json!({});
        arguments[argument] = json!("spec/login.spec");
        crate::tool_errors::mcp_error(&call_tool(server, tool, arguments))
    };
    let outline = refused(&mut server, "specforge.outline", "file");
    let fixes = refused(&mut server, "specforge.suggest_fixes", "file_path");
    assert_eq!(outline["code"], "file_not_found", "{outline}");
    assert_eq!(outline["argument"], "file", "{outline}");
    assert_eq!(outline["file"], "spec/login.spec", "{outline}");
    assert_eq!(
        outline["data"]["suggestion"],
        "did you mean 'login.spec'? a spec file is named from the spec root",
        "{outline}"
    );
    // One refusal, one wording: the view's.
    assert_eq!(outline["message"], fixes["message"], "{outline} / {fixes}");
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_suggest_fixes_tool",
    verify = "a file_path that names no source is refused naming the file"
)]
fn suggest_fixes_names_the_file_it_refuses() {
    let mut server = named_files();
    for file in ["nope.spec", "spec/login.spec", "../x.spec"] {
        let reply = call_tool(
            &mut server,
            "specforge.suggest_fixes",
            json!({"file_path": file}),
        );
        let error = crate::tool_errors::mcp_error(&reply);
        assert_eq!(error["argument"], "file_path", "{file}: {error}");
        assert_eq!(error["file"], file, "{file}: {error}");
    }
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_suggest_fixes_tool",
    verify = "with no project served, a file_path is the no-project refusal naming the file"
)]
fn a_named_file_with_nothing_served_is_no_projects_file() {
    let mut server = unserved();
    for (tool, argument) in [
        ("specforge.outline", "file"),
        ("specforge.suggest_fixes", "file_path"),
    ] {
        let mut arguments = json!({});
        arguments[argument] = json!("spec/a.spec");
        let error = crate::tool_errors::mcp_error(&call_tool(&mut server, tool, arguments));
        assert_eq!(error["code"], "precondition_failed", "{tool}: {error}");
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .starts_with("no project is served, so 'spec/a.spec' is no project's file"),
            "{tool}: {error}"
        );
        assert_eq!(error["file"], "spec/a.spec", "{tool}: {error}");
    }
}
