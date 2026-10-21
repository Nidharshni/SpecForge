//! What `init` does inside a project that already exists above its directory (ADR 0197): a nested
//! project ends the enclosing project's sources (ADR 0080), so init judges what that project reads.

use std::path::Path;

use specforge_ops::authoring::init::{self, InitField};
use specforge_ops::{OpErrorKind, Refusal};
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

use crate::writes::files_under;

/// An enclosing project at `dir` with a spec root of `spec`, and `files` under `dir`.
fn enclosing(dir: &Path, files: &[(&str, &str)]) {
    std::fs::write(
        dir.join("specforge.json"),
        r#"{"name":"outer","version":"0.1.0","extensions":[],"spec_root":"spec"}"#,
    )
    .unwrap();
    for (path, text) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

fn init_at(dir: &Path) -> Result<init::Outcome, Refusal<InitField>> {
    init::init(
        &init::Request {
            dir,
            name: Some("nested"),
            version: init::DEFAULT_VERSION,
            extensions: &[],
            caller: specforge_ops::testing::caller(),
        },
        &InProcessRuntime::new(),
    )
}

const OUTER: &str = "spec \"outer\" {\n  version \"0.1.0\"\n}\n";

#[specforge_test_macros::test(
    behavior = "scaffold_new_project",
    verify = "init refuses a directory whose files the enclosing project reads as its sources, naming them, writing nothing"
)]
#[ignore = "flipped by T3"]
fn init_refuses_a_directory_whose_files_the_enclosing_project_reads() {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path();
    enclosing(
        root,
        &[("spec/a.spec", OUTER), ("spec/sub/t.spec", "// t\n")],
    );

    let refusal = init_at(&root.join("spec/sub")).unwrap_err();

    assert_eq!(refusal.error.code, "inside_project_sources");
    assert_eq!(refusal.error.kind, OpErrorKind::Conflict);
    assert_eq!(refusal.subject, Some(InitField::Dir));
    let message = &refusal.error.message;
    assert!(message.contains("t.spec"), "{message}");
    assert!(
        message.contains(&root.canonicalize().unwrap().display().to_string()),
        "{message}"
    );
    assert_eq!(files_under(&root.join("spec/sub")).len(), 1);
}

#[specforge_test_macros::test(
    behavior = "scaffold_new_project",
    verify = "init refuses the directory the enclosing project reads its sources from, which would read the new starter as its own"
)]
#[ignore = "flipped by T3"]
fn init_refuses_the_enclosing_projects_spec_root() {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path();
    enclosing(root, &[("spec/a.spec", OUTER)]);

    let refusal = init_at(&root.join("spec")).unwrap_err();

    assert_eq!(refusal.error.code, "inside_project_sources");
    assert_eq!(refusal.subject, Some(InitField::Dir));
    assert!(
        refusal.error.message.contains("spec/hello.spec"),
        "{}",
        refusal.error.message
    );
    assert!(!root.join("spec/specforge.json").exists());
}
