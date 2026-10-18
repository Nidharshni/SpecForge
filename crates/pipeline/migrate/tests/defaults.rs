//! The `defaults` migration step: the reference lists a file's blocks repeat move into its `defaults` (ADR 0149).
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_migrate::migrate_project;
use specforge_parser::CURRENT_FORMAT_VERSION;
use specforge_test_macros::test as specforge_test;
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
    migrate_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        std::slice::from_ref(&a),
        &CURRENT_FORMAT_VERSION,
        false,
        true,
    );
    std::fs::read_to_string(&a).unwrap()
}

/// `behavior <id> "T" { <body> }`, `body` being whole lines.
fn block(id: &str, body: &str) -> String {
    format!("behavior {id} \"T\" {{\n{body}}}\n")
}

/// Blocks `a`..: each with the body `body(index)`, separated by blank lines.
fn blocks(count: usize, body: impl Fn(usize) -> String) -> String {
    (0..count)
        .map(|i| block(&((b'a' + i as u8) as char).to_string(), &body(i)))
        .collect::<Vec<_>>()
        .join("\n")
}

const BOTH: &str = "  ports [R]\n  features [F]\n";

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "a list every block writes alike is hoisted"
)]
fn a_list_every_block_writes_alike_is_hoisted() {
    let after = migrated(&blocks(3, |_| BOTH.to_string()));
    assert_eq!(
        after,
        format!(
            "defaults behavior {{\n  ports    [R]\n  features [F]\n}}\n\n{}",
            blocks(3, |_| String::new())
        )
    );
}

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "the most common list is hoisted and the others keep theirs"
)]
fn the_most_common_list_is_hoisted_and_the_others_keep_theirs() {
    let text = blocks(4, |i| {
        format!(
            "  ports [{}]\n  features [F]\n",
            if i < 3 { "R" } else { "S" }
        )
    });
    let after = migrated(&text);
    assert!(after.starts_with("defaults behavior {\n  ports    [R]\n  features [F]\n}\n"));
    assert!(
        after.contains("behavior d \"T\" {\n  ports [S]\n}\n"),
        "{after}"
    );
    assert!(!after.contains("ports [R]"), "{after}");
}

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "a kind where a block lacks the field is left as written"
)]
fn a_kind_where_a_block_lacks_the_field_is_left_as_written() {
    let text = blocks(6, |i| {
        if i < 5 {
            "  ports [R]\n  features [F]\n".to_string()
        } else {
            "  ports [R]\n".to_string()
        }
    });
    let after = migrated(&text);
    assert!(
        after.starts_with("defaults behavior {\n  ports [R]\n}\n"),
        "{after}"
    );
    assert_eq!(after.matches("features [F]").count(), 5, "{after}");
}

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "an identifier, a quoted or a multi-line list is never hoisted"
)]
fn an_identifier_a_quoted_or_a_multi_line_list_is_never_hoisted() {
    for body in [
        "  category command\n",
        "  types [\"R\"]\n",
        "  ports [\n    R,\n    S,\n  ]\n",
        "  ports []\n",
    ] {
        let text = blocks(6, |_| body.to_string());
        assert_eq!(migrated(&text), text, "{body}");
    }
}

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "a line with a comment on or above it is left as written"
)]
fn a_line_with_a_comment_on_or_above_it_is_left_as_written() {
    for body in ["  ports [R] // why\n", "  // why\n  ports [R]\n"] {
        let text = blocks(6, |_| body.to_string());
        assert_eq!(migrated(&text), text, "{body}");
    }
}

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "a kind that has defaults is left as written"
)]
fn a_kind_that_has_defaults_is_left_as_written() {
    let text = format!(
        "defaults behavior {{\n  features [F]\n}}\n\n{}",
        blocks(6, |_| "  ports [R]\n".to_string())
    );
    assert_eq!(migrated(&text), text);
}

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "a hoist that saves no line is not written"
)]
fn a_hoist_that_saves_no_line_is_not_written() {
    let text = blocks(2, |_| "  ports [R]\n".to_string());
    assert_eq!(migrated(&text), text);
    let four = blocks(4, |_| "  ports [R]\n".to_string());
    assert_eq!(migrated(&four), four);
    assert!(
        migrated(&blocks(5, |_| "  ports [R]\n".to_string())).starts_with("defaults behavior {")
    );
}

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "the block goes after the imports or before the first form"
)]
fn the_block_goes_after_the_imports_or_before_the_first_form() {
    let body = blocks(3, |_| BOTH.to_string());
    let defaults = "defaults behavior {\n  ports    [R]\n  features [F]\n}\n";
    let bare = blocks(3, |_| String::new());

    assert_eq!(
        migrated(&format!("use \"x\"\n\n{body}")),
        format!("use \"x\"\n\n{defaults}\n{bare}")
    );
    assert_eq!(
        migrated(&format!("// File.\n\n// Doc.\n{body}")),
        format!("// File.\n\n{defaults}\n// Doc.\n{bare}")
    );
}

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "hoisting twice changes nothing"
)]
fn hoisting_twice_changes_nothing() {
    let once = migrated(&blocks(4, |_| BOTH.to_string()));
    assert_eq!(migrated(&once), once);
}

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "a file with a syntax error is left alone"
)]
fn a_file_with_a_syntax_error_is_left_alone() {
    let text = format!(
        "{}behavior z \"T\" {{\n  ports [R\n",
        blocks(4, |_| BOTH.to_string())
    );
    assert_eq!(migrated(&text), text);
}
