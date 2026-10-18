//! The `titles` migration step: a block's doc comment becomes its title (ADR 0148).
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_migrate::migrate_project;
use specforge_parser::CURRENT_FORMAT_VERSION;
use tempfile::TempDir;

#[test]
#[ignore = "flipped by T4"]
fn a_doc_comment_becomes_the_title() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("spec")).unwrap();
    let a = dir.path().join("spec/a.spec");
    std::fs::write(&a, "// Command payload.\ntype C {\n  id string\n}\n").unwrap();

    migrate_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        std::slice::from_ref(&a),
        &CURRENT_FORMAT_VERSION,
        false,
        true,
    );

    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        "type C \"Command payload.\" {\n  id string\n}\n"
    );
}
