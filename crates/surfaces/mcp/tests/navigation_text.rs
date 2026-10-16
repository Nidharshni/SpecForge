//! Navigation answers from the text the project was compiled from, and only
//! for the files it compiled (ADR 0116).

use crate::support::*;
use serde_json::{Value, json};
use specforge_test::prelude::*;

/// A file that is not one of the project's sources is `file_not_found`
/// though it exists: a config file, the root, a directory, an excluded file.
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

/// `old()` is 2020-01-01: old enough that a file's stamp carries no content hash
/// (`GRANULARITY`), so an edit that keeps length and mtime is invisible to it.
fn old() -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_577_836_800)
}

const LIMIT: &str = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";
const LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";

fn set_mtime(path: &std::path::Path) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(old()).unwrap();
}

/// A project of `files` with `@specforge/software`, every file's mtime old
/// before it is served, so its stamps carry no content hash.
fn served_old(files: &[(&str, &str)]) -> Served {
    let project = files.iter().fold(
        TestProject::new().enabling(&["@specforge/software"]),
        |project, (file, text)| project.file(file, text),
    );
    set_mtime(&project.root().join("specforge.json"));
    for (file, _) in files {
        set_mtime(&project.root().join(file));
    }
    project.serve_components()
}

/// Rewrite `file` in place with `text` of the same length and restore its
/// mtime: an edit the stamps cannot see (ADR 0100).
fn unseen_edit(served: &Served, file: &str, text: &str) {
    let path = served.root().join(file);
    let old = std::fs::read_to_string(&path).unwrap();
    assert_eq!(old.len(), text.len(), "an unseen edit keeps the length");
    std::fs::write(&path, text).unwrap();
    set_mtime(&path);
}

#[specforge_test(
    behavior = "provide_mcp_find_definition_tool",
    verify = "a definition is a position in the text the project was compiled from, not the file on disk now"
)]
fn a_definition_is_a_position_in_the_compiled_text() {
    let mut server = served_old(&[("limit.spec", LIMIT)]);
    unseen_edit(
        &server,
        "limit.spec",
        "invariant  session_limit \"Limt\" {\n  guarantee \"x\"\n}\n",
    );

    let resp = call_tool(
        &mut server,
        "specforge.find_definition",
        json!({"entity_id": "session_limit"}),
    );
    let parsed: Value = serde_json::from_str(&tool_text(&resp)).unwrap();
    let span = &parsed["name_span"];
    assert_eq!(
        (
            span["start_line"].clone(),
            span["start_col"].clone(),
            span["end_line"].clone(),
            span["end_col"].clone()
        ),
        (json!(1), json!(11), json!(1), json!(24)),
        "{parsed}"
    );
    assert_eq!(parsed["column"], 11, "{parsed}");
}

#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "a rename whose file changed on disk since the compile is refused as conflict, and nothing is written"
)]
fn rename_refuses_a_file_changed_since_the_compile() {
    let padded = format!("{LOGIN}//{}\n", "c".repeat(46));
    let mut server = served_old(&[("limit.spec", LIMIT), ("login.spec", &padded)]);
    unseen_edit(
        &server,
        "login.spec",
        "behavior login \"Login\" {\n  invariants [session_limit]\n}\n\
         behavior x2 \"X\" {\n  invariants [session_limit]\n}\n",
    );
    let before = crate::support::files_under(server.root());

    let resp = call_tool(
        &mut server,
        "specforge.rename",
        json!({"entity_id": "session_limit", "new_name": "session_cap"}),
    );

    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["code"], "conflict", "{error}");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(message.contains("login.spec"), "{error}");
    let after = crate::support::files_under(server.root());
    assert!(
        crate::support::changed_files(server.root(), &before, &after).is_empty(),
        "nothing is written"
    );
}
