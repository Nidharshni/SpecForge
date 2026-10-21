//! A project view reads every project file at its root through the containment it carries: the schema
//! cache joins the recorded report (ADR 0139, ADR 0146, ADR 0183).

use crate::view_support::{Assembled, Viewed};
use specforge_ops::read::export::{Request, export, export_recorded};
use specforge_test::prelude::*;
use std::path::{Path, PathBuf};

const SECRET: &str = "OUTSIDE-SCHEMA-SECRET";

fn project() -> Assembled {
    Assembled::over(crate::view_support::registries(&["behavior"], &[]))
        .source("behavior b \"B\" {\n  contract \"The system MUST work\"\n}\n")
}

fn cache_file(project: &Assembled) -> PathBuf {
    project.root().join(".specforge/schema-cache.json")
}

/// `.specforge/` under the project, and a file outside it holding `text`.
fn outside_file(project: &Assembled, outside: &Path, text: &str) -> PathBuf {
    std::fs::create_dir_all(cache_file(project).parent().unwrap()).unwrap();
    let file = outside.join("secret.json");
    std::fs::write(&file, text).unwrap();
    file
}

fn secret_record() -> String {
    format!(r#"{{"schema": "{SECRET}", "content_hash": "x"}}"#)
}

/// The notices the export of a view carries.
fn notices_of(view: &specforge_ops::view::ProjectView<'_>) -> Vec<specforge_common::Diagnostic> {
    export(view, &Request::default())
        .unwrap()
        .notices()
        .to_vec()
}

#[cfg(unix)]
#[ignore = "flipped by the ticket that reads the schema cache through the view's disk"]
#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "over MCP, a schema cache that leads outside the project or has more than one name is not read"
)]
fn a_bounded_view_versions_against_no_schema_cache_that_links_outside() {
    let project = project();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside_file(&project, outside.path(), &secret_record());
    std::os::unix::fs::symlink(&secret, cache_file(&project)).unwrap();

    let view = project.view().contained();
    assert_eq!(view.versioned_schema().version().major, 1);
    let notices = notices_of(&view);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0].code, "I016");
    assert!(
        notices[0].message.contains("outside"),
        "{}",
        notices[0].message
    );
    assert!(
        !notices[0].message.contains(SECRET),
        "{}",
        notices[0].message
    );
}

#[cfg(unix)]
#[ignore = "flipped by the ticket that reads the schema cache through the view's disk"]
#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "over MCP, a schema cache that leads outside the project or has more than one name is not read"
)]
fn a_bounded_view_versions_against_no_hard_linked_schema_cache() {
    let project = project();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside_file(&project, outside.path(), &secret_record());
    std::fs::hard_link(&secret, cache_file(&project)).unwrap();

    let view = project.view().contained();
    assert_eq!(view.versioned_schema().version().major, 1);
    let notices = notices_of(&view);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0].code, "I016");
    assert!(
        notices[0].message.contains("2 names"),
        "{}",
        notices[0].message
    );
    assert!(
        !notices[0].message.contains(SECRET),
        "{}",
        notices[0].message
    );
}

#[cfg(unix)]
#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "the CLI and the LSP read the schema cache where its link points"
)]
fn an_unbounded_view_versions_against_a_linked_schema_cache() {
    let project = project();
    // A real record, steered to 7.3.1, kept outside the project.
    let _ = export_recorded(&project.view(), &Request::default());
    let mut record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(cache_file(&project)).unwrap()).unwrap();
    record["schema"]["schema_version"] = serde_json::json!({"major": 7, "minor": 3, "patch": 1});
    std::fs::remove_file(cache_file(&project)).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let file = outside_file(&project, outside.path(), &record.to_string());
    std::os::unix::fs::symlink(&file, cache_file(&project)).unwrap();

    assert_eq!(project.view().versioned_schema().version().major, 7);
}

/// ADR 0183 D4: ops opens file content only through `containment/`, but for the collector machine's
/// approvals store (a user file, not a project file).
#[ignore = "flipped by the ticket that reads the schema cache through the view's disk"]
#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "over MCP, a schema cache that leads outside the project or has more than one name is not read"
)]
fn ops_opens_file_content_only_through_the_containment() {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);
    let mut found = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let skipped = rel.starts_with("containment/")
            || rel == "analysis/collect/process.rs"
            || rel.contains("/tests/")
            || rel.starts_with("registry/testing/")
            || name == "tests.rs"
            || name == "testing.rs"
            || name.ends_with("_tests.rs");
        if skipped {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        let production = text.split("#[cfg(test)]").next().unwrap();
        for (n, line) in production.lines().enumerate() {
            let line = line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            if ["std::fs::read(", "std::fs::read_to_string(", "File::open("]
                .iter()
                .any(|call| line.contains(call))
            {
                found.push(format!("src/{rel}:{}", n + 1));
            }
        }
    }
    assert!(
        found.is_empty(),
        "file content opened outside containment/: {found:?}"
    );
}
