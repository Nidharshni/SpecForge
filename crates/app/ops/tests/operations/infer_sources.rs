//! The files inference covers (ADR 0113): walked by the shared discovery policy, held inside the
//! project root, listed once. The `#[ignore]`d tests pin today's defects; the ticket named in
//! each reason flips one.

use std::path::Path;

use serde_json::json;
use specforge_ops::read::infer::{lint, progress};
use specforge_protocol_types::{AnalyzerDescriptor, ExtensionDeclaration, HandshakeResponse};
use specforge_test::prelude::*;
use tempfile::TempDir;

use crate::view_support::{Assembled, Viewed};

/// An extension `name` analyzing `.rs` files and never scanning `excluded`.
fn analyzer(name: &str, excluded: &[&str]) -> ExtensionDeclaration {
    ExtensionDeclaration {
        handshake: HandshakeResponse {
            name: name.into(),
            version: "1.0.0".into(),
            ..Default::default()
        },
        analyzers: vec![AnalyzerDescriptor {
            language: "rust".into(),
            file_extensions: vec![".rs".into()],
            scan_export: "scan".into(),
            classify_export: "classify".into(),
            map_export: "map".into(),
            excluded_dirs: excluded.iter().map(|d| (*d).to_string()).collect(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// A project with `src/lib.rs`, whose compile loaded the rust analyzer (it declares `target`).
fn project() -> Assembled {
    let project = Assembled::new().declarations(vec![analyzer("@acme/rust", &["target"])]);
    write(project.root(), "src/lib.rs", "pub fn inside() {}\n");
    project
}

fn write(root: &Path, name: &str, text: &str) {
    let file = root.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// A directory beside the project's root, holding `secret.rs`.
fn outside(project: &Assembled) -> TempDir {
    let dir = TempDir::new_in(project.root().parent().unwrap()).unwrap();
    write(dir.path(), "secret.rs", "pub fn secret_outside() {}\n");
    dir
}

fn manifest(project: &Assembled, text: serde_json::Value) {
    write(project.root(), "specforge-infer.json", &text.to_string());
}

fn unanalyzed(project: &Assembled) -> Vec<String> {
    progress(&project.view()).unwrap().unanalyzed
}

#[cfg(unix)]
fn link(target: &Path, name: &Path) {
    std::fs::create_dir_all(name.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(target, name).unwrap();
}

#[cfg(unix)]
#[specforge_test(
    behavior = "discover_inference_sources",
    verify = "no symlinked directory is entered, so a symlink cycle adds no source"
)]
#[ignore = "flipped by T6"]
fn a_symlink_cycle_adds_no_source() {
    let project = project();
    link(Path::new(".."), &project.root().join("src/loop"));
    let progress = progress(&project.view()).unwrap();
    assert_eq!(progress.summary.files_total, 1, "{progress:?}");
    assert_eq!(progress.unanalyzed, ["src/lib.rs"]);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "discover_inference_sources",
    verify = "no symlinked directory is entered, so a symlink cycle adds no source"
)]
#[ignore = "flipped by T6"]
fn a_symlinked_directory_is_not_entered() {
    let project = project();
    let outside = outside(&project);
    link(outside.path(), &project.root().join("src/ext"));
    assert_eq!(unanalyzed(&project), ["src/lib.rs"]);
}

#[specforge_test(
    behavior = "discover_inference_sources",
    verify = "an analyzer's excluded_dirs add to the default skipped directories"
)]
#[ignore = "flipped by T6"]
fn an_analyzers_excluded_dirs_add_to_the_skipped_directories() {
    let project = Assembled::new().declarations(vec![
        analyzer("@acme/rust", &["target"]),
        analyzer("@acme/gen", &["gen"]),
    ]);
    let root = project.root();
    for file in [
        "node_modules/d/x.rs",
        "vendor/c/y.rs",
        "__pycache__/p.rs",
        "build/b.rs",
        "target/z.rs",
        "gen/g.rs",
        "src/lib.rs",
    ] {
        write(root, file, "pub fn f() {}\n");
    }
    assert_eq!(unanalyzed(&project), ["src/lib.rs"]);
}

#[specforge_test(
    behavior = "discover_inference_sources",
    verify = "a file two source roots reach is one source"
)]
#[ignore = "flipped by T6"]
fn a_file_two_source_roots_reach_is_one_source() {
    let project = project();
    manifest(
        &project,
        json!({"version": 1, "source_roots": ["src", "."]}),
    );
    let progress = progress(&project.view()).unwrap();
    assert_eq!(progress.summary.files_total, 1, "{progress:?}");
    assert_eq!(progress.unanalyzed, ["src/lib.rs"]);
}

#[specforge_test(
    behavior = "discover_inference_sources",
    verify = "no inference source lands outside the project root"
)]
#[ignore = "flipped by T6"]
fn a_source_root_outside_the_project_is_not_walked() {
    let project = project();
    let outside = outside(&project);
    let name = outside.path().file_name().unwrap().to_string_lossy();
    manifest(
        &project,
        json!({
            "version": 1,
            "source_roots": [format!("../{name}"), outside.path().to_string_lossy()],
        }),
    );
    let progress = progress(&project.view()).unwrap();
    assert_eq!(progress.summary.files_total, 0, "{progress:?}");
    assert!(progress.unanalyzed.is_empty(), "{progress:?}");
}

#[cfg(unix)]
#[specforge_test(
    behavior = "discover_inference_sources",
    verify = "no inference source lands outside the project root"
)]
#[ignore = "flipped by T6"]
fn a_symlinked_file_that_leaves_the_project_is_no_source() {
    let project = project();
    let outside = outside(&project);
    link(
        &outside.path().join("secret.rs"),
        &project.root().join("src/leak.rs"),
    );
    assert_eq!(unanalyzed(&project), ["src/lib.rs"]);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "discover_inference_sources",
    verify = "an indexed path outside the project root is deleted, never read"
)]
#[ignore = "flipped by T7"]
fn an_indexed_path_outside_the_project_is_deleted_and_never_read() {
    let project = project();
    let outside = outside(&project);
    link(outside.path(), &project.root().join("src/ext"));
    let name = outside.path().file_name().unwrap().to_string_lossy();
    let escaping = format!("../{name}/secret.rs");
    let entry = |path: &str| {
        json!({"path": path, "content_hash": "00", "entities_produced": ["x"],
               "analyzed_at": "2026-10-01T00:00:00Z"})
    };
    manifest(
        &project,
        json!({
            "version": 1,
            "source_roots": ["src"],
            "source_index": [entry(&escaping), entry("src/ext/secret.rs")],
        }),
    );
    let progress = progress(&project.view()).unwrap();
    assert!(progress.stale.is_empty(), "{progress:?}");
    assert_eq!(
        progress.deleted,
        [escaping, "src/ext/secret.rs".to_string()]
    );
    let diagnostics = lint(&project.view());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[specforge_test(
    behavior = "discover_inference_sources",
    verify = "a nested project's files are not the outer project's inference sources"
)]
#[ignore = "flipped by T8"]
fn a_nested_projects_files_are_no_inference_sources() {
    let project = project();
    write(project.root(), "sub/specforge.json", "{}");
    write(project.root(), "sub/n.rs", "pub fn nested() {}\n");
    assert_eq!(unanalyzed(&project), ["src/lib.rs"]);
}

#[cfg(unix)]
#[test]
fn a_symlinked_file_inside_the_project_is_a_source() {
    let project = project();
    link(Path::new("lib.rs"), &project.root().join("src/alias.rs"));
    assert_eq!(unanalyzed(&project), ["src/alias.rs", "src/lib.rs"]);
}

#[test]
fn hidden_directories_are_not_walked() {
    let project = project();
    write(project.root(), ".cache/h.rs", "pub fn hidden() {}\n");
    assert_eq!(unanalyzed(&project), ["src/lib.rs"]);
}
