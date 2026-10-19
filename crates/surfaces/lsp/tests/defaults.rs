//! A `defaults <kind> { … }` block as the LSP serves it (ADR 0149): it completes, hovers and navigates as the
//! kind it names, and an inherited reference is one location and one rename edit.

use specforge_extension_sdk::prelude::*;
use specforge_lsp::{LspState, answers};
use specforge_test_macros::test as spec;
use tower_lsp::lsp_types::{CompletionResponse, GotoDefinitionResponse, Position, Url};

use crate::served::Served;

const SOURCE: &str = "defaults behavior {\n  ports [Repo]\n}\n\n\
                      behavior a \"A\" {\n}\n\nbehavior b \"B\" {\n}\n\nbehavior c \"C\" {\n}\n\n\
                      port Repo \"Repo\" {\n}\n";

fn declare(c: &mut ContributionsBuilder) {
    c.kind("behavior", |k| {
        k.field("ports", |f| {
            f.field_type(FieldType::ReferenceList).target_kind("port");
        });
        k.field("features", |f| {
            f.field_type(FieldType::ReferenceList)
                .target_kind("feature");
        });
    });
    c.kind("port", |_| {});
    c.kind("feature", |_| {});
}

fn served(source: &str) -> Served {
    Served::new(&[("main.spec", source)])
        .extension("@t/defaults", declare)
        .open(&["main.spec"])
}

fn labels(response: Option<CompletionResponse>) -> Vec<String> {
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("completion answers a list");
    };
    items.into_iter().map(|i| i.label).collect()
}

fn complete(served: &Served, line: u32, character: u32) -> Vec<String> {
    labels(answers::completion(
        &served.state(),
        &served.uri("main.spec"),
        Position::new(line, character),
    ))
}

fn edits(state: &LspState, uri: &Url, at: Position) -> Vec<String> {
    let edit = answers::rename(state, uri, at, "Store").unwrap().unwrap();
    let mut out: Vec<String> = edit
        .changes
        .unwrap()
        .into_values()
        .flatten()
        .map(|e| {
            format!(
                "{}:{}-{}",
                e.range.start.line, e.range.start.character, e.range.end.character
            )
        })
        .collect();
    out.sort();
    out
}

#[spec(
    behavior = "apply_file_defaults",
    verify = "a defaults block completes, hovers and navigates as the kind it names"
)]
fn fields_of_the_named_kind_complete_inside_a_defaults_block() {
    let served = served("defaults behavior {\n  \n}\n");
    let names = complete(&served, 1, 2);
    assert!(names.contains(&"ports".to_string()), "{names:?}");
    assert!(names.contains(&"features".to_string()), "{names:?}");
}

#[test]
fn defaults_completes_as_a_top_level_keyword() {
    let served = served("def\n");
    assert!(complete(&served, 0, 3).contains(&"defaults".to_string()));
}

#[test]
fn hover_on_a_defaults_field_shows_its_help() {
    let served = served(SOURCE);
    let text = served.hover("main.spec", 1, 3).expect("a hover");
    assert!(text.contains("`ports`"), "{text}");
    assert!(text.contains("→ **port**"), "{text}");
}

#[test]
fn definition_from_a_default_reaches_the_entity() {
    let served = served(SOURCE);
    let Some(GotoDefinitionResponse::Scalar(location)) = answers::definition(
        &served.state(),
        &served.uri("main.spec"),
        Position::new(1, 10),
    ) else {
        panic!("a location");
    };
    assert_eq!(
        (location.range.start.line, location.range.start.character),
        (13, 5)
    );
}

#[spec(
    behavior = "apply_file_defaults",
    verify = "the editor shows an inherited reference's location once"
)]
fn references_show_an_inherited_reference_once() {
    let served = served(SOURCE);
    let (state, uri) = (served.state(), served.uri("main.spec"));
    let at = Position::new(1, 10);
    let only = answers::references(&state, &uri, at, false).unwrap();
    assert_eq!(only.len(), 1, "{only:?}");
    let with_declaration = answers::references(&state, &uri, at, true).unwrap();
    assert_eq!(with_declaration.len(), 2, "{with_declaration:?}");
}

#[test]
fn prepare_rename_answers_on_a_default_and_rename_edits_it_once() {
    let served = served(SOURCE);
    let (state, uri) = (served.state(), served.uri("main.spec"));
    let at = Position::new(1, 10);
    assert!(answers::prepare_rename(&state, &uri, at).unwrap().is_some());
    assert_eq!(edits(&state, &uri, at), ["13:5-9", "1:9-13"]);
}

#[test]
fn the_kind_a_defaults_block_names_is_no_reference() {
    let served = served(SOURCE);
    let state = served.state();
    let doc = state.document(&served.uri("main.spec")).unwrap();
    let tokens = doc.tokens(&state.view());
    let at = |line: u32| {
        tokens
            .iter()
            .find(|t| t.line == line && t.text == "behavior")
            .unwrap_or_else(|| panic!("no behavior token on line {line}"))
            .token_type
    };
    // The name a defaults header gives is coloured as the kind keyword of a block, not as an entity name.
    assert_eq!(at(0), at(4));
}

/// A default no block takes: no behavior in the file (W161).
const UNTAKEN: &str = "defaults behavior {\n  ports [Repo]\n}\n\nport Repo \"Repo\" {\n}\n";

#[test]
#[ignore = "flipped by T5"]
fn references_show_a_default_no_block_takes() {
    let served = served(UNTAKEN);
    let (state, uri) = (served.state(), served.uri("main.spec"));
    let at = Position::new(4, 6);
    let found = answers::references(&state, &uri, at, true).unwrap();
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(edits(&state, &uri, at), ["1:9-13", "4:5-9"]);
}
