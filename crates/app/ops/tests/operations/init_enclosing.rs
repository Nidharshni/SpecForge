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
fn init_refuses_a_directory_whose_files_the_enclosing_project_reads() {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path();
    enclosing(
        root,
        &[("spec/a.spec", OUTER), ("spec/sub/t.spec", "// t\n")],
    );

    let refusal = init_at(&root.join("spec/sub")).unwrap_err();

    assert_eq!(refusal.error.code, init::INSIDE_PROJECT_SOURCES);
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
fn init_refuses_the_enclosing_projects_spec_root() {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path();
    enclosing(root, &[("spec/a.spec", OUTER)]);

    let refusal = init_at(&root.join("spec")).unwrap_err();

    assert_eq!(refusal.error.code, init::INSIDE_PROJECT_SOURCES);
    assert_eq!(refusal.subject, Some(InitField::Dir));
    assert!(
        refusal.error.message.contains("spec/hello.spec"),
        "{}",
        refusal.error.message
    );
    assert!(!root.join("spec/specforge.json").exists());
}

#[specforge_test_macros::test(
    behavior = "scaffold_new_project",
    verify = "init names the project it is created inside, and creates a separate one beside its sources"
)]
fn init_names_the_project_it_is_created_inside() {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path();
    enclosing(root, &[("spec/a.spec", OUTER)]);

    let outcome = init_at(&root.join("other")).unwrap();
    assert_eq!(outcome.enclosing, Some(root.canonicalize().unwrap()));

    let alone = TempDir::new().unwrap();
    assert_eq!(init_at(&alone.path().join("p")).unwrap().enclosing, None);
}

#[specforge_test_macros::test(
    behavior = "scaffold_new_project",
    verify = "a directory the enclosing project's exclude leaves out holds none of its sources, and init proceeds there"
)]
fn an_excluded_directory_is_no_source_init_takes() {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path();
    enclosing(root, &[("spec/sub/t.spec", "// t\n")]);
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"outer","version":"0.1.0","extensions":[],"spec_root":"spec","exclude":["sub/"]}"#,
    )
    .unwrap();

    let outcome = init_at(&root.join("spec/sub")).unwrap();

    assert_eq!(outcome.enclosing, Some(root.canonicalize().unwrap()));
}

#[specforge_test_macros::test(
    behavior = "scaffold_new_project",
    verify = "init below the enclosing project's spec root proceeds where that project reads no source"
)]
fn init_below_the_spec_root_in_a_directory_without_sources_proceeds() {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path();
    enclosing(root, &[("spec/a.spec", OUTER), ("spec/empty/.keep", "")]);

    assert!(init_at(&root.join("spec/empty")).is_ok());
    assert!(init_at(&root.join("spec/new/deeper")).is_ok());
}

#[specforge_test_macros::test(
    behavior = "scaffold_new_project",
    verify = "init refuses a directory whose files the enclosing project reads as its sources, naming them, writing nothing"
)]
fn a_refusal_names_the_first_sources_and_counts_the_rest() {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path();
    enclosing(root, &[("spec/a.spec", OUTER)]);
    for n in 1..=7 {
        std::fs::create_dir_all(root.join("spec/sub")).unwrap();
        std::fs::write(root.join(format!("spec/sub/f{n}.spec")), "// t\n").unwrap();
    }

    let message = init_at(&root.join("spec/sub")).unwrap_err().error.message;

    assert!(
        message.contains(
            "sub/f1.spec, sub/f2.spec, sub/f3.spec, sub/f4.spec, sub/f5.spec, and 2 more"
        ),
        "{message}"
    );
}

#[specforge_test_macros::test(
    behavior = "scaffold_new_project",
    verify = "init refuses the directory the enclosing project reads its sources from, which would read the new starter as its own"
)]
fn init_refuses_a_directory_whose_spec_dir_is_the_enclosing_spec_root() {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path();
    enclosing(root, &[]);
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"outer","version":"0.1.0","extensions":[],"spec_root":"x/spec"}"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.join("x/spec")).unwrap();
    std::fs::write(root.join("x/spec/a.spec"), OUTER).unwrap();

    let refusal = init_at(&root.join("x")).unwrap_err();

    assert_eq!(refusal.error.code, init::INSIDE_PROJECT_SOURCES);
}
