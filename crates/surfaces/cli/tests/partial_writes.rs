//! Writes that fail part-way, end to end (ADR 0128). A write past `ulimit -f` fails with `EFBIG` after a prefix is
//! written, which no in-process fault gives: it fails a file's *restore* after its forward write succeeded.
//!
//! The sizes are preconditions, asserted: `ulimit -f 2` is 1024 bytes under `sh` in POSIX mode and 2048 under bash,
//! so a text that must not fit is over 2048 bytes and one that must fit is under 1024.

use crate::support::{Cli, TestProject, mcp_process};
use specforge_test_macros::test as specforge_test;
use std::fs;

const LIMIT_BLOCKS: u32 = 2;
const ID: &str = "session_limit_for_every_authenticated_user_in_the_tenant_xx";

/// `a.spec`: the invariant and 22 behaviors that reference it.
fn referencing_source(id: &str) -> String {
    let mut text = format!("invariant {id} \"Limit\" {{\n  guarantee \"x\"\n}}\n");
    for n in 1..=22 {
        text.push_str(&format!(
            "behavior b{n} \"B{n}\" {{\n  invariants [{id}]\n}}\n"
        ));
    }
    text
}

/// A run warmed once without the limit, so the limit never meets a compile-cache write.
fn warm(project: &TestProject) {
    Cli::on(project).run(&["check"]);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "provide_mcp_rename_tool",
    verify = "a rename that cannot put a file back names it in files_written"
)]
fn a_rename_that_cannot_put_a_file_back_names_it_in_files_written() {
    use std::os::unix::fs::PermissionsExt;
    let old = referencing_source(ID);
    let renamed = referencing_source("ab");
    assert!(old.len() > 2048, "the old text must not fit: {}", old.len());
    assert!(
        renamed.len() < 1024,
        "the new text must fit: {}",
        renamed.len()
    );
    let project = TestProject::new()
        .enabling(&["@specforge/software"])
        .file("a.spec", &old)
        .file(
            "b.spec",
            format!("behavior other \"Other\" {{\n  invariants [{ID}]\n}}\n"),
        );
    let locked = project.root().join("b.spec");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o444)).unwrap();
    if fs::OpenOptions::new().write(true).open(&locked).is_ok() {
        return; // permissions don't bind this user (root)
    }
    warm(&project);

    let requests = [
        mcp_process::initialize(0),
        mcp_process::request(
            1,
            "tools/call",
            serde_json::json!({"name": "specforge.rename",
                "arguments": {"entity_id": ID, "new_name": "ab"}}),
        ),
    ];
    let responses = Cli::on(&project)
        .under_file_size_limit(LIMIT_BLOCKS)
        .mcp_session(&requests);

    let response = mcp_process::find_response(&responses, 1)
        .unwrap_or_else(|| panic!("no reply to the rename: {responses:?}"));
    let error = mcp_process::tool_error(response);
    assert_eq!(
        error["data"]["files_written"],
        serde_json::json!(["a.spec"]),
        "the file the rename could not put back is named: {error}"
    );
    assert_eq!(
        fs::read_to_string(project.root().join("a.spec")).unwrap(),
        renamed,
        "a.spec holds the renamed text whole, not a prefix of either"
    );
}

#[cfg(unix)]
#[specforge_test(
    behavior = "format_spec_files",
    verify = "a file whose write fails part-way is left whole and is not in files_written"
)]
fn a_format_whose_write_fails_part_way_leaves_the_source_whole() {
    let mut source = String::new();
    for n in 1..=60 {
        source.push_str(&format!("behavior b{n} \"B{n}\" {{ contract \"c\" }}\n"));
    }
    let project = TestProject::new().enabling(&["@specforge/software"]);
    fs::create_dir_all(project.root().join("spec")).unwrap();
    let file = project.root().join("spec").join("a.spec");
    fs::write(&file, &source).unwrap();
    warm(&project);

    let document = Cli::on(&project)
        .under_file_size_limit(LIMIT_BLOCKS)
        .document(&["format"]);

    assert_eq!(document.code, 1, "{}", document.json);
    assert_eq!(
        document.json["failures"][0]["operation"], "write",
        "{}",
        document.json
    );
    assert_eq!(
        document.json["files_written"],
        serde_json::json!([]),
        "{}",
        document.json
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        source,
        "a failed write leaves the source as it was"
    );
}
