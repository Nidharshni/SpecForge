//! What kind of failure MCP answers for an unusable test report, what
//! `specforge.format` and `specforge.migrate` return as their verdict, and
//! what they do with a directory that is no project (plan 02, ADR 0029).
//!
//! The first block was taken before ops decided any of it; the commit that
//! changes a pinned fact flips its pin and says so.

use crate::support::*;
use crate::tool_errors::mcp_error;
use serde_json::{Value, json};
use specforge_test::prelude::*;

const MAIN: &str = concat!(
    "behavior alpha \"Alpha\" {\n",
    "  contract \"The system MUST work\"\n",
    "}\n",
);

const MESSY: &str = "behavior messy \"Messy\" {\ncontract \"The system MUST work\"\n}\n";

/// A fresh served project holding [`MAIN`], `@specforge/software` enabled.
fn served() -> Served {
    TestProject::new()
        .enabling(&["@specforge/software"])
        .file("main.spec", MAIN)
        .serve_components()
}

/// Write the recorded report at the served project's root.
fn report(served: &Served, text: &str) {
    served.write("specforge-report.json", text);
}

/// Make the report unreadable; `false` when this process can still read it
/// (running as root).
#[cfg(unix)]
fn lock(served: &Served) -> bool {
    use std::os::unix::fs::PermissionsExt;
    report(served, r#"{"results":{}}"#);
    let path = served.root().join("specforge-report.json");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    std::fs::read(&path).is_err()
}

/// The `code` and the `diagnostic.code` of the refusal `tool` answers.
fn refusal(served: &mut Served, tool: &str, arguments: Value) -> (String, String) {
    let error = mcp_error(&call_tool(served, tool, arguments));
    (
        error["code"].as_str().unwrap().to_string(),
        error["diagnostic"]["code"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

/// Every tool that reads the recorded report, with arguments that reach it.
fn report_readers() -> Vec<(&'static str, Value)> {
    vec![
        ("specforge.stats", json!({})),
        ("specforge.coverage", json!({})),
        ("specforge.inspect", json!({"entity_id": "alpha"})),
        (
            "specforge.query",
            json!({"entity_id": "alpha", "include_coverage": true}),
        ),
        // `specforge.trace`'s plan check reads no report: tools_core
        // `a_report_the_plan_check_does_not_read_fails_no_plan`.
        ("specforge.analyze", json!({})),
    ]
}

#[test]
fn an_unparsable_report_refuses_every_view() {
    let mut served = served();
    report(&served, "{");

    for (tool, arguments) in report_readers() {
        let (code, diagnostic) = refusal(&mut served, tool, arguments);
        assert_eq!(
            (code.as_str(), diagnostic.as_str()),
            ("schema_mismatch", "E045"),
            "{tool}"
        );
    }
    for (prompt, arguments) in [
        ("review", json!({})),
        (
            "trace",
            json!({"plan": {"entries": [{"entity_id": "alpha"}]}}),
        ),
    ] {
        let reply = get_prompt(
            &mut served,
            &format!("specforge://prompts/{prompt}"),
            arguments,
        );
        let data = &reply["error"]["data"];
        assert_eq!(data["code"], "schema_mismatch", "{prompt}: {reply}");
        assert_eq!(data["diagnostic"]["code"], "E045", "{prompt}: {reply}");
    }
}

#[cfg(unix)]
#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "a report the OS refuses to read is a permission_denied error on every tool that reads it"
)]
fn a_locked_report_is_permission_denied_on_every_tool() {
    let mut served = served();
    if !lock(&served) {
        return;
    }

    for (tool, arguments) in report_readers() {
        let (code, diagnostic) = refusal(&mut served, tool, arguments);
        assert_eq!(
            (code.as_str(), diagnostic.as_str()),
            ("permission_denied", "E045"),
            "{tool}"
        );
    }
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "a test_results file that does not exist is a file_not_found error"
)]
fn a_missing_named_report_is_file_not_found() {
    let mut served = served();
    let missing = served.root().join("none.json");

    let (code, diagnostic) = refusal(
        &mut served,
        "specforge.analyze",
        json!({"test_results": missing.to_str().unwrap()}),
    );

    assert_eq!(
        (code.as_str(), diagnostic.as_str()),
        ("file_not_found", "E045")
    );
}

#[specforge_test(
    behavior = "provide_mcp_analyze_tool",
    verify = "a relative test_results names a file under the project's root"
)]
fn a_relative_test_results_is_under_the_project_root() {
    let mut served = served();
    served.write("r.json", r#"{"results":{}}"#);

    let response = call_tool(
        &mut served,
        "specforge.analyze",
        json!({"test_results": "r.json"}),
    );

    // The test process's cwd is the crate directory, not the project.
    assert_ne!(response["result"]["isError"], true, "{response}");
}

/// A region the formatter cannot parse and keeps as written (W142).
const REGION: &str = "behavior broken \"Broken\" {\n  @@@ ]]\n}\n";

/// What `specforge.format` answers for `arguments` over a fresh project
/// holding `text` as `messy.spec`: `(isError, ok, all_clean)`.
fn format_verdict(text: &str, arguments: Value) -> (bool, Value, Value) {
    let mut served = served();
    served.write("messy.spec", text);
    let response = call_tool(&mut served, "specforge.format", arguments);
    let failed = response["result"]["isError"] == true;
    let result = match failed {
        true => mcp_error(&response)["data"].clone(),
        false => tool_json(&response),
    };
    (failed, result["ok"].clone(), result["all_clean"].clone())
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "ok is the verdict specforge format exits by, in every mode"
)]
fn format_ok_is_the_cli_verdict() {
    // --check over a change: the CLI exits 1.
    assert_eq!(
        format_verdict(MESSY, json!({"check": true})),
        (false, json!(false), json!(false))
    );
    // --diff alone over a change: exit 0.
    assert_eq!(
        format_verdict(MESSY, json!({"diff": true})),
        (false, json!(true), json!(false))
    );
    // A write of a change: exit 0.
    assert_eq!(
        format_verdict(MESSY, json!({})),
        (false, json!(true), json!(false))
    );
    // A clean file under --check: exit 0.
    let clean = "behavior messy \"Messy\" {\n  contract \"The system MUST work\"\n}\n";
    assert_eq!(
        format_verdict(clean, json!({"check": true})),
        (false, json!(true), json!(true))
    );
    // A region left unformatted (W142): exit 1 in every mode.
    for arguments in [json!({}), json!({"check": true}), json!({"diff": true})] {
        let (_, ok, clean) = format_verdict(REGION, arguments.clone());
        assert_eq!((ok, clean), (json!(false), json!(false)), "{arguments}");
    }
}

#[cfg(unix)]
#[test]
fn format_ok_is_false_when_a_file_cannot_be_read() {
    use std::os::unix::fs::PermissionsExt;
    let mut served = served();
    served.write("messy.spec", MESSY);
    let path = served.root().join("messy.spec");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&path).is_ok() {
        return; // running as root
    }

    let response = call_tool(&mut served, "specforge.format", json!({}));

    assert_eq!(mcp_error(&response)["data"]["ok"], false, "{response}");
}

/// A directory with one unformatted `a.spec` and no project config.
fn loose(text: &str, name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(name), text).unwrap();
    dir
}

#[specforge_test(
    behavior = "provide_mcp_format_tool",
    verify = "a directory that is no project is formatted with the defaults, as specforge format formats it"
)]
fn a_directory_that_is_no_project_is_formatted_with_the_defaults() {
    let mut served = served();
    let dir = loose(MESSY, "a.spec");

    let result = tool(
        &mut served,
        "specforge.format",
        json!({"path": dir.path().to_str().unwrap(), "check": true}),
    );

    assert_eq!(result["ok"], false, "{result}");
    let changed = result["changed_files"].as_array().unwrap();
    assert!(
        changed.len() == 1 && changed[0].as_str().unwrap().ends_with("a.spec"),
        "{result}"
    );
    // Nothing was written by a check.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.spec")).unwrap(),
        MESSY
    );

    // A write formats it.
    let written = tool(
        &mut served,
        "specforge.format",
        json!({"path": dir.path().to_str().unwrap()}),
    );
    assert_eq!(written["ok"], true, "{written}");
    assert_ne!(
        std::fs::read_to_string(dir.path().join("a.spec")).unwrap(),
        MESSY
    );
}

#[specforge_test(
    behavior = "provide_mcp_migrate_tool",
    verify = "a directory that is no project is migrated as specforge migrate migrates it"
)]
fn a_directory_that_is_no_project_is_migrated() {
    let mut served = served();
    let dir = loose(
        "// specforge-format: 0.1\nbehavior old_one \"Old\" {\n}\n",
        "old.spec",
    );

    let result = tool(
        &mut served,
        "specforge.migrate",
        json!({"path": dir.path().to_str().unwrap(), "dry_run": true}),
    );

    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["files_migrated"], 1, "{result}");
    assert!(
        result["results"][0]["file_path"]
            .as_str()
            .is_some_and(|path| path.ends_with("old.spec")),
        "{result}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_migrate_tool",
    verify = "a failed migration's error names its cause: a hook, a changed graph, a project that does not compile, or files that could not be migrated"
)]
fn a_migration_that_cannot_read_a_file_is_a_schema_mismatch() {
    let mut served = served();
    served.write(
        "bad.spec",
        "// specforge-format: 99.0\nbehavior bad \"Bad\" {\n}\n",
    );

    let error = mcp_error(&call_tool(&mut served, "specforge.migrate", json!({})));

    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .starts_with("1 file(s) could not be migrated: "),
        "{error}"
    );
    assert_eq!(error["data"]["files_failed"], 1, "{error}");
    assert_eq!(error["data"]["ok"], false, "{error}");
}

#[specforge_test(
    behavior = "provide_mcp_migrate_tool",
    verify = "ok is the verdict specforge migrate exits by, and a failed migration's kind is the operation's"
)]
fn migrate_ok_is_the_cli_verdict() {
    // A migration that applies passes: exit 0.
    let mut applying = served_with_old();
    let applied = tool(&mut applying, "specforge.migrate", json!({}));
    assert_eq!(applied["ok"], true, "{applied}");

    // A dry run of the same passes.
    let mut preview = served_with_old();
    let dry = tool(&mut preview, "specforge.migrate", json!({"dry_run": true}));
    assert_eq!(dry["ok"], true, "{dry}");

    // Nothing to migrate passes.
    let mut current = served();
    let nothing = tool(&mut current, "specforge.migrate", json!({}));
    assert_eq!(nothing["ok"], true, "{nothing}");

    // A file that cannot migrate fails: exit 1, schema_mismatch, ok false.
    let mut bad = served();
    bad.write(
        "bad.spec",
        "// specforge-format: 99.0\nbehavior bad \"Bad\" {\n}\n",
    );
    let error = mcp_error(&call_tool(&mut bad, "specforge.migrate", json!({})));
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert_eq!(error["data"]["ok"], false, "{error}");
}

fn served_with_old() -> Served {
    let served = served();
    served.write(
        "old.spec",
        "// specforge-format: 0.1\nbehavior old_one \"Old\" {\n}\n",
    );
    served
}
