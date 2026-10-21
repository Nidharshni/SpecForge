//! A document is written in the field forms of its own project, found from its place.

use specforge_lsp::answers;
use tempfile::TempDir;
use tower_lsp::lsp_types::{FormattingOptions, Url};

use crate::contracts::apply_edits;
use crate::served::Served;

const TAGGED: &str =
    "behavior bb \"B\" {\n  contract \"The system MUST x\"\n  category \"auth\"\n}\n";
const SOFTWARE: &str = r#"{"extensions": ["@specforge/software"]}"#;
const NONE: &str = r#"{"extensions": []}"#;

fn project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (name, text) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

/// `text` as the editor's formatting answer for `uri` rewrites it.
fn formatted(served: &Served, uri: &Url, text: &str) -> String {
    let options = FormattingOptions {
        tab_size: 2,
        insert_spaces: true,
        ..Default::default()
    };
    let answer =
        answers::formatting(&served.state(), uri, &options, None).expect("an open document");
    let edits = serde_json::to_value(&answer.edits).unwrap();
    apply_edits(text, edits.as_array().unwrap())
}

#[test]
#[ignore = "flipped by T3"]
fn a_nested_projects_document_is_written_in_its_own_projects_forms() {
    let dir = project(&[
        ("specforge.json", NONE),
        ("inner/specforge.json", SOFTWARE),
        ("inner/spec/a.spec", TAGGED),
    ]);
    let served = Served::at(dir).open(&["inner/spec/a.spec"]);
    let uri = served.uri("inner/spec/a.spec");
    let text = formatted(&served, &uri, TAGGED);
    assert!(text.contains("\n  category auth\n"), "{text}");
}

#[test]
#[ignore = "flipped by T3"]
fn a_document_formatted_before_its_project_opens_is_written_in_its_projects_forms() {
    let dir = project(&[("specforge.json", SOFTWARE), ("spec/a.spec", TAGGED)]);
    let served = Served::at(dir).restore(&[("spec/a.spec", TAGGED)]);
    let uri = served.uri("spec/a.spec");
    let text = formatted(&served, &uri, TAGGED);
    assert!(text.contains("\n  category auth\n"), "{text}");
}

#[test]
#[ignore = "flipped by T3"]
fn a_document_outside_any_project_is_written_with_no_forms() {
    let outside = project(&[("loose.spec", TAGGED)]);
    assert!(
        specforge_common::ProjectRoot::enclosing(outside.path()).is_none(),
        "a specforge.json at or above the temp root"
    );
    let dir = project(&[("specforge.json", SOFTWARE), ("spec/a.spec", TAGGED)]);
    let mut served = Served::at(dir).open(&["spec/a.spec"]);
    let uri = Url::from_file_path(outside.path().join("loose.spec")).unwrap();
    served.open_document(&uri, TAGGED);
    assert_eq!(formatted(&served, &uri, TAGGED), TAGGED);
}

#[test]
#[ignore = "flipped by T3"]
fn a_document_of_another_project_is_written_in_that_projects_forms() {
    let dir = project(&[
        ("specforge.json", NONE),
        ("spec/ob.spec", TAGGED),
        ("inner/specforge.json", SOFTWARE),
    ]);
    let mut served = Served::at(dir).open_in("inner", &[]);
    let uri = served.uri("spec/ob.spec");
    served.open_document(&uri, TAGGED);
    assert_eq!(formatted(&served, &uri, TAGGED), TAGGED);
}
