//! A keyword or field completion's snippet is the block (or member) a typed create writes (ADR 0191).

use crate::session::Session;
use serde_json::Value;
use specforge_ops::authoring::format::Place;
use specforge_ops::view::ProjectView;
use specforge_project::testing::TestProject;
use specforge_test_macros::test as spec;

/// A keyword, the id and title to fill it with, its fields' values, and the block `new` writes for them.
type Row<'a> = (&'a str, &'a str, &'a str, &'a [(&'a str, &'a str)], &'a str);

const EXTENSIONS: [&str; 3] = [
    "@specforge/software",
    "@specforge/governance",
    "@specforge/formal",
];

/// The completion items at (`line`, `character`) of `text`, in a project that formats with `indent_width = 4`.
async fn items_at(text: &str, positions: &[(u32, u32)]) -> Vec<Vec<Value>> {
    let (mut session, uri, dir) = Session::with_extensions(&EXTENSIONS, "test.spec", text).await;
    std::fs::write(dir.path().join(".specforgefmt.toml"), "indent_width = 4\n").unwrap();
    let mut found = Vec::new();
    for &(line, character) in positions {
        let response = session.completion(&uri, line, character).await;
        found.push(response["result"].as_array().cloned().unwrap_or_default());
    }
    found
}

fn item<'a>(items: &'a [Value], label: &str) -> &'a Value {
    items
        .iter()
        .find(|i| i["label"] == label)
        .unwrap_or_else(|| panic!("no {label} in {items:?}"))
}

/// `line` with its tabstops (`$n` or `${n:default}`) replaced by `values`, in order.
fn fill_line(line: &str, values: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = line;
    let mut values = values.iter();
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        let end = if tail.starts_with('{') {
            tail.find('}').map_or(tail.len(), |close| close + 1)
        } else {
            tail.chars().take_while(char::is_ascii_digit).count()
        };
        out.push_str(values.next().copied().unwrap_or(""));
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// `snippet` filled in: the id and the title in its header, each field's tabstop by the field's name, the `$0` line dropped.
fn filled(snippet: &str, id: &str, title: &str, values: &[(&str, &str)]) -> String {
    let mut lines = snippet.lines();
    let mut out = vec![fill_line(lines.next().unwrap(), &[id, title])];
    for line in lines {
        if line.contains("$0") {
            continue;
        }
        let word = line.split_whitespace().next().unwrap_or("");
        match values.iter().find(|(field, _)| *field == word) {
            Some((_, value)) => out.push(fill_line(line, &[value])),
            None => out.push(line.to_string()),
        }
    }
    out.join("\n")
}

#[spec(
    behavior = "complete_keywords",
    verify = "a filled keyword snippet is the block new writes"
)]
#[tokio::test]
async fn a_filled_keyword_snippet_is_the_block_new_writes() {
    let rows: [Row; 3] = [
        (
            "decision",
            "use_rust",
            "Use Rust",
            &[("status", "accepted"), ("context", "c"), ("decision", "d")],
            "decision use_rust \"Use Rust\" {\n    status   accepted\n    context  \"c\"\n    decision \"d\"\n}",
        ),
        (
            "process",
            "p1",
            "P",
            &[("alphabet", "a"), ("initial_state", "s")],
            "process p1 \"P\" {\n    alphabet      [\"a\"]\n    initial_state \"s\"\n}",
        ),
        (
            "behavior",
            "login",
            "Login",
            &[("contract", "c")],
            "behavior login \"Login\" {\n    contract \"c\"\n}",
        ),
    ];
    let found = items_at("\n", &[(0, 0)]).await;
    for (label, id, title, values, expected) in rows {
        let snippet = item(&found[0], label)["insertText"].as_str().unwrap();
        assert_eq!(
            filled(snippet, id, title, values),
            expected,
            "{label}: {snippet}"
        );
    }
}

#[spec(
    behavior = "complete_field_names",
    verify = "a field snippet is the member set writes, laid out with the file's indent"
)]
#[tokio::test]
async fn a_field_snippet_is_the_member_set_writes() {
    let text = "failure_mode disk_full \"Disk full\" {\n    severity high\n    cause    \"c\"\n    effect   \"e\"\n    \n}\n\nprocess p1 \"P\" {\n    alphabet      [\"a\"]\n    initial_state \"s\"\n    \n}\n";
    let found = items_at(text, &[(4, 4), (10, 4)]).await;

    let block = item(&found[0], "post_mitigation");
    assert_eq!(
        block["insertText"], "post_mitigation {\n    $1\n}",
        "{block}"
    );
    assert_eq!(block["insertTextMode"], 2, "{block}");
    assert_eq!(item(&found[0], "rpn")["insertText"], "rpn ${1:0}");
    assert_eq!(item(&found[0], "cause")["insertText"], "cause \"$1\"");
    assert_eq!(
        item(&found[1], "alphabet")["insertText"],
        "alphabet [\"$1\"]"
    );
}

#[spec(
    behavior = "complete_keywords",
    verify = "a keyword snippet's tabstops follow its text"
)]
fn tabstops_follow_the_text_order() {
    let dir = TestProject::new()
        .config(|c| c["name"] = "snippets".into())
        .enabling(&["@specforge/software", "@specforge/governance"])
        .files(&[
            ("test.spec", "\n"),
            (".specforgefmt.toml", "indent_width = 4\n"),
        ]);
    let project = dir.loaded();
    let view = ProjectView::of(&project);
    let place = dir.root().join("test.spec");
    let found = crate::completion::complete_at("\n", 0, 0, &view, Place::File(&place)).1;
    let decision = item_of(&found, "decision");
    assert_eq!(
        decision.insert_text.as_deref(),
        Some(
            "decision ${1:id} \"${2:Title}\" {\n    status   $3\n    context  \"$4\"\n    decision \"$5\"\n    $0\n}"
        )
    );
}

fn item_of<'a>(
    items: &'a [tower_lsp::lsp_types::CompletionItem],
    label: &str,
) -> &'a tower_lsp::lsp_types::CompletionItem {
    items.iter().find(|i| i.label == label).unwrap()
}
