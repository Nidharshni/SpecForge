//! The files inference covers, over a served project (ADR 0113): none outside the project is read
//! or listed. The `#[ignore]`d tests pin today's defects; the ticket named in each reason flips
//! one.

use crate::support::*;
use serde_json::{Value, json};
use specforge_extension_sdk::prelude::*;
use specforge_test::prelude::*;
use tempfile::TempDir;

/// The rust analyzer, whose scanner names one item per file: the word after `fn `.
fn rust(c: &mut ContributionsBuilder) {
    c.analyzer("rust", |a| {
        a.file_extensions(&[".rs"])
            .excluded_dirs(&["target"])
            .scan(|request| ScanResponse {
                items: request
                    .content
                    .split("fn ")
                    .nth(1)
                    .and_then(|rest| rest.split('(').next())
                    .map(|name| ScannedItem {
                        name: name.to_string(),
                        item_kind: "function".into(),
                        line: 1,
                        visibility: None,
                        signature: None,
                    })
                    .into_iter()
                    .collect(),
                language: None,
            });
    });
}

/// A served project with `src/lib.rs`, and links to a directory and a file outside it.
fn scene() -> (TempDir, Served) {
    let project = TestProject::new();
    let outside = TempDir::new().unwrap();
    std::fs::write(
        outside.path().join("secret.rs"),
        "pub fn secret_outside() {}\n",
    )
    .unwrap();
    let src = project.root().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("lib.rs"), "pub fn inside() {}\n").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), src.join("ext")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.rs"), src.join("leak.rs")).unwrap();
        std::os::unix::fs::symlink("..", src.join("loop")).unwrap();
    }
    let served = project.serve(&[TestExtension::named("@specforge/rust").declaring(rust)]);
    (outside, served)
}

fn answer(server: &mut Served, tool: &str, arguments: Value) -> Value {
    let reply = call_tool(server, tool, arguments);
    serde_json::from_str(&tool_text(&reply)).unwrap()
}

#[cfg(unix)]
#[specforge_test(
    behavior = "discover_inference_sources",
    verify = "no inference source lands outside the project root"
)]
#[ignore = "flipped by T6"]
fn infer_gaps_lists_no_file_outside_the_served_project() {
    let (_outside, mut server) = scene();
    let gaps = answer(&mut server, "specforge.infer_gaps", json!({}));
    assert!(!gaps.to_string().contains("secret_outside"), "{gaps}");
    assert_eq!(gaps["total_pub_items"], 1, "{gaps}");
}

#[cfg(unix)]
#[specforge_test(
    behavior = "discover_inference_sources",
    verify = "an indexed path outside the project root is deleted, never read"
)]
#[ignore = "flipped by T7"]
fn infer_progress_and_the_inferred_lint_read_no_indexed_file_outside_the_served_project() {
    let project = TestProject::new();
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("secret.rs");
    std::fs::write(&secret, "pub fn secret_outside() {}\n").unwrap();
    let src = project.root().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::os::unix::fs::symlink(outside.path(), src.join("ext")).unwrap();
    let entry = |path: &str| {
        json!({"path": path, "content_hash": "00", "entities_produced": ["x"],
               "analyzed_at": "2026-10-01T00:00:00Z"})
    };
    std::fs::write(
        project.root().join("specforge-infer.json"),
        json!({
            "version": 1,
            "source_roots": ["src"],
            "source_index": [entry("src/ext/secret.rs"), entry(&secret.to_string_lossy())],
        })
        .to_string(),
    )
    .unwrap();
    let mut server = project.serve(&[TestExtension::named("@specforge/rust").declaring(rust)]);

    let progress = answer(&mut server, "specforge.infer_progress", json!({}));
    assert_eq!(progress["stale"], json!([]), "{progress}");

    let validate = answer(
        &mut server,
        "specforge.validate",
        json!({"lint": ["inferred"]}),
    );
    let text = validate.to_string();
    assert!(
        !text.contains("I200") && !text.contains("I202"),
        "{validate}"
    );
}
