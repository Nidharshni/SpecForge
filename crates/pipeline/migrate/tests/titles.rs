//! The `titles` migration step: a block's doc comment becomes its title (ADR 0148).
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_graph::{FieldForms, build_graph, compute_graph_delta};
use specforge_migrate::{beyond_steps, migrate_project};
use specforge_parser::{CURRENT_FORMAT_VERSION, parse};
use specforge_test_macros::test as specforge_test;
use std::path::Path;
use tempfile::TempDir;

/// `text` as a project's one source, migrated to the current version; what the file holds after.
fn migrated(text: &str) -> String {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("spec")).unwrap();
    let a = dir.path().join("spec/a.spec");
    std::fs::write(&a, text).unwrap();
    let summary = migrate_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        std::slice::from_ref(&a),
        &CURRENT_FORMAT_VERSION,
        &|path: &Path| std::fs::read_to_string(path),
        &FieldForms::default(),
    );
    summary
        .rewrites
        .first()
        .map_or_else(|| text.to_string(), |rewrite| rewrite.after.clone())
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a doc comment directly above a block becomes its title"
)]
fn a_doc_comment_becomes_the_title() {
    assert_eq!(
        migrated("// Command payload.\ntype C {\n  id string\n}\n"),
        "type C \"Command payload.\" {\n  id string\n}\n"
    );
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a multi-line doc comment becomes one title"
)]
fn a_multi_line_doc_comment_becomes_one_title() {
    assert_eq!(
        migrated("// One.\n// Two.\ntype C {\n}\n"),
        "type C \"One. Two.\" {\n}\n"
    );
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a union takes its doc comment as its title"
)]
fn a_union_takes_its_doc_comment() {
    assert_eq!(
        migrated("// States.\ntype S = a | b\n"),
        "type S \"States.\" = a | b\n"
    );
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a block with no doc comment is titled with its id in words"
)]
fn no_doc_comment_gives_the_id_in_words() {
    assert_eq!(
        migrated(
            "type CompleteTaskCommand {\n}\n\nevent complete_task {\n}\n\ntype timestamp = string\n"
        ),
        "type CompleteTaskCommand \"Complete Task Command\" {\n}\n\nevent complete_task \"Complete Task\" {\n}\n\ntype timestamp \"Timestamp\" = string\n"
    );
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a comment after a blank line is not lifted"
)]
fn a_comment_after_a_blank_line_is_not_lifted() {
    assert_eq!(
        migrated("// Section.\n\ntype Priority {\n}\n"),
        "// Section.\n\ntype Priority \"Priority\" {\n}\n"
    );
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a trailing comment on the header stays"
)]
fn a_trailing_comment_on_the_header_stays() {
    assert_eq!(
        migrated("type J = string // note\n"),
        "type J \"J\" = string // note\n"
    );
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "the format header is never lifted"
)]
fn the_format_header_is_never_lifted() {
    assert_eq!(
        migrated("// specforge-format: 1.0\ntype X {\n}\n"),
        "// specforge-format: 1.0\ntype X \"X\" {\n}\n"
    );
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "quotes and backslashes in a doc comment are escaped in the title"
)]
fn quotes_and_backslashes_are_escaped() {
    let comment = "say \"hi\" with a \\ here";
    let text = migrated(&format!("// {comment}\ntype Q {{\n}}\n"));
    let file = parse(&text, "a.spec");
    assert!(file.errors.is_empty(), "{:?}", file.errors);
    assert_eq!(file.entities[0].title.as_deref(), Some(comment));
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "titled, define, spec and ref blocks are left alone"
)]
fn a_titled_block_and_define_spec_ref_blocks_are_left_alone() {
    let text = "// kept\nspec \"shop\" {\n}\n\ndefaults behavior {\n  ports [R]\n}\n\n// kept\ntype T \"Titled\" {\n}\n\n// kept\ndefine d {\n}\n\nref github.issue:org/repo#1 \"Issue\"\n";
    assert_eq!(migrated(text), text);
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a file with a syntax error is left alone"
)]
fn a_file_with_a_syntax_error_is_left_alone() {
    let text = "// c\ntype X {\n  id string\n";
    assert_eq!(migrated(text), text);
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "the titles step is idempotent"
)]
fn the_step_is_idempotent() {
    for text in [
        "// Command payload.\ntype C {\n}\n",
        "// States.\ntype S = a | b\n",
        "type CompleteTaskCommand {\n}\n",
        "// Section.\n\ntype Priority {\n}\n",
        "type J = string // note\n",
    ] {
        let once = migrated(text);
        assert_eq!(migrated(&once), once, "{text}");
    }
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "the changes beyond the steps drop only a title the step added"
)]
fn beyond_steps_drops_only_titles_the_step_added() {
    let build = |text: &str| build_graph(&[parse(text, "a.spec")]).0;
    let before = build("type A {\n}\n\ntype B \"Old\" {\n}\n\ntype C {\n  id string\n}\n");
    let after =
        build("type A \"A\" {\n}\n\ntype B \"New\" {\n}\n\ntype C \"C\" {\n  id integer\n}\n");

    let delta = beyond_steps(compute_graph_delta(&before, &after), &before);

    let ids: Vec<&str> = delta.modified_nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["B", "C"], "{delta:?}");
}

#[specforge_test(
    behavior = "add_missing_titles",
    verify = "a header whose name is on its second line takes its title after the name"
)]
fn a_header_whose_name_is_on_its_second_line_takes_its_title_after_the_name() {
    assert_eq!(
        migrated("feature\n  second_auth {\n  status proposed\n}\n"),
        "feature\n  second_auth \"Second Auth\" {\n  status proposed\n}\n"
    );
}
