//! Where `specforge migrate` writes the format version header: where the parser reads it.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_graph::FieldForms;
use specforge_migrate::migrate_project;
use specforge_parser::{CURRENT_FORMAT_VERSION, detect_format_version};
use specforge_test_macros::test as specforge_test;
use std::path::Path;
use tempfile::TempDir;

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "a header below leading blank lines is rewritten where it stands"
)]
fn a_header_below_blank_lines_is_rewritten_where_it_stands() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("spec")).unwrap();
    let a = dir.path().join("spec/a.spec");
    std::fs::write(
        &a,
        "\n// specforge-format: 0.9\nbehavior alpha \"Alpha\" {\n}\n",
    )
    .unwrap();

    let summary = migrate_project(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        std::slice::from_ref(&a),
        &CURRENT_FORMAT_VERSION,
        &|path: &Path| std::fs::read_to_string(path),
        &FieldForms::default(),
    );

    assert_eq!(summary.migrated_count, 1, "{summary:?}");
    let text = summary.rewrites[0].after.clone();
    assert_eq!(
        text,
        "\n// specforge-format: 1.0\nbehavior alpha \"Alpha\" {\n}\n"
    );
    assert!(!text.contains("0.9"), "{text}");
    assert_eq!(
        detect_format_version(&text, "a.spec"),
        (CURRENT_FORMAT_VERSION, vec![])
    );
}
