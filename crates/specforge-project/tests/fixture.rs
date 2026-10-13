//! `TestProject`, the fixture every surface's tests build their project with. These prove the test support.

use serde_json::json;
use specforge_extension_sdk::prelude::*;
use specforge_project::testing::{TestProject, specs};
use specforge_wasm::testing::InProcessRuntime;

const EXT: &str = "@test/note";

fn note_extension() -> InProcessRuntime {
    InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "1.0.0"));
        c.kind("note", |_| {});
        c
    })
}

fn files_in(root: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_new_project_is_a_config_and_nothing_else() {
    let project = TestProject::new();
    assert_eq!(files_in(project.root()), ["specforge.json"]);
    assert_eq!(
        project.config_on_disk(),
        json!({"name": "t", "version": "0.1.0", "extensions": []})
    );
    assert!(project.read("specforge.json").ends_with("}\n"));
}

#[test]
fn enabling_and_config_edits_write_through() {
    let project = TestProject::new()
        .enabling(&["@specforge/software"])
        .config(|c| c["spec_root"] = "spec".into());
    let on_disk = project.config_on_disk();
    assert_eq!(on_disk["extensions"], json!(["@specforge/software"]));
    assert_eq!(on_disk["spec_root"], "spec");
    assert_eq!(project.enabled().unwrap(), ["@specforge/software"]);
    let project = project.file("spec/a.spec", "x");
    assert_eq!(project.read("spec/a.spec"), "x");
}

#[test]
fn raw_config_is_written_verbatim() {
    let project = TestProject::new().raw_config("{not json");
    assert_eq!(project.read("specforge.json"), "{not json");
}

#[test]
fn without_config_leaves_no_file() {
    let project = TestProject::new().without_config().file("a.spec", "x");
    assert_eq!(files_in(project.root()), ["a.spec"]);
}

#[test]
fn home_is_empty_and_apart_from_the_root() {
    let project = TestProject::new();
    assert!(files_in(project.home()).is_empty());
    assert!(!project.home().starts_with(project.root()));
    assert!(!project.root().starts_with(project.home()));
}

#[test]
fn copy_of_leaves_out_derived_directories() {
    let source = TestProject::new()
        .file("spec/a.spec", "a")
        .file(".specforge/cache", "c")
        .file("target/out", "o");
    let copy = TestProject::copy_of(source.root());
    assert_eq!(files_in(copy.root()), ["spec", "specforge.json"]);
    assert_eq!(copy.read("spec/a.spec"), "a");
    // Verbatim until an edit.
    assert_eq!(copy.read("specforge.json"), source.read("specforge.json"));
    let edited = copy.config(|c| c["name"] = "copy".into());
    assert_eq!(edited.config_on_disk()["name"], "copy");
}

#[test]
fn copy_in_places_a_tree_under_the_root() {
    let source = TestProject::new()
        .file("spec/a.spec", "a")
        .file("target/out", "o");
    let project = TestProject::new().copy_in(source.root(), "parts/one");
    assert_eq!(project.read("parts/one/spec/a.spec"), "a");
    assert_eq!(
        project.read("parts/one/specforge.json"),
        source.read("specforge.json")
    );
    assert!(!project.root().join("parts/one/target").exists());
}

#[test]
fn compile_loads_the_builtins_it_enables() {
    let compiled = TestProject::new()
        .enabling(&["@specforge/software"])
        .file("main.spec", specs::SOFTWARE)
        .loaded();
    assert!(
        compiled
            .environment()
            .registries()
            .kinds()
            .get("behavior")
            .is_some()
    );
    assert!(compiled.graph().node("parse_input").is_some());
}

#[test]
fn serving_installs_and_compiles_in_the_served_runtime() {
    let project = TestProject::new()
        .enabling(&[EXT])
        .serving(note_extension())
        .file("main.spec", "note n \"N\" {}\n");
    let compiled = project.loaded();
    assert!(
        compiled
            .environment()
            .registries()
            .kinds()
            .get("note")
            .is_some()
    );
    assert!(compiled.graph().node("n").is_some());
    assert!(!project.served().unwrap().calls().is_empty());
}

#[test]
#[should_panic(expected = "enabled but not loaded")]
fn loaded_names_an_extension_that_did_not_load() {
    TestProject::new()
        .enabling(&["@test/missing=missing.wasm"])
        .loaded();
}

#[test]
fn session_and_compile_agree() {
    let project = TestProject::new()
        .enabling(&[EXT])
        .serving(note_extension())
        .file(
            "a.spec",
            "note n \"N\" {}\nnote m \"M\" {\n    nonsense\n}\n",
        );
    let compiled = project.compile();
    let session = project.session();
    assert_eq!(session.project().diagnostics(), compiled.diagnostics());
}
