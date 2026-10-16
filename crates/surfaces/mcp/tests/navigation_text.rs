//! Navigation answers from the text the project was compiled from, and only
//! for the files it compiled (ADR 0116).

use crate::support::*;
use serde_json::{Value, json};
use specforge_test::prelude::*;

/// A file that is not one of the project's sources is `file_not_found`
/// though it exists: a config file, the root, a directory, an excluded file.
#[ignore = "flipped by T8"]
#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "a file that is not one of the project's sources is file_not_found, though it exists"
)]
fn outline_answers_only_the_projects_sources() {
    let mut server = TestProject::new()
        .config(|c| c["exclude"] = json!(["drafts/**"]))
        .file("main.spec", "behavior a \"A\" {\n}\n")
        .file("drafts/d.spec", "behavior d \"D\" {\n}\n")
        .file("empty.spec", "// nothing yet\n")
        .serve(&[TestExtension::software()]);

    for file in ["specforge.json", ".", "drafts", "drafts/d.spec"] {
        let resp = call_tool(&mut server, "specforge.outline", json!({ "file": file }));
        let error = crate::tool_errors::mcp_error(&resp);
        assert_eq!(error["code"], "file_not_found", "{file}: {error}");
        assert_eq!(error["argument"], "file", "{file}: {error}");
        assert_eq!(error["file"], file, "{file}: {error}");
    }

    let main = call_tool(
        &mut server,
        "specforge.outline",
        json!({"file": "main.spec"}),
    );
    let entries: Value =
        serde_json::from_str::<Value>(&tool_text(&main)).unwrap()["entries"].clone();
    assert_eq!(entries[0]["entity_id"], "a", "{entries}");

    for file in ["empty.spec", "./empty.spec"] {
        let resp = call_tool(&mut server, "specforge.outline", json!({ "file": file }));
        let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
        assert_eq!(parsed, json!({"entries": []}), "{file}");
    }
}
