//! A quoted reference as the LSP serves it (ADR 0218): its ID, inside the quotes, is the token every request about
//! the entity under the cursor reads.

use specforge_lsp::answers;
use specforge_test_macros::test as spec;
use tower_lsp::lsp_types::{GotoDefinitionResponse, Position, PrepareRenameResponse, Range};

use crate::defaults::served;

/// `ports ["Repo"]` (the fixture extension `@t/defaults`: `behavior.ports` names `port`s): the ID is 1:10-1:14.
const QUOTED: &str = "behavior a \"A\" {\n  ports [\"Repo\"]\n}\n\nport Repo \"Repo\" {\n}\n";

fn range(r: Range) -> String {
    format!(
        "{}:{}-{}:{}",
        r.start.line, r.start.character, r.end.line, r.end.character
    )
}

#[spec(
    behavior = "find_all_references",
    verify = "each reference is the identifier token as written"
)]
fn references_show_a_quoted_reference_inside_its_quotes() {
    let served = served(QUOTED);
    let (state, uri) = (served.state(), served.uri("main.spec"));
    let mut found: Vec<String> = answers::references(&state, &uri, Position::new(4, 6), true)
        .unwrap()
        .into_iter()
        .map(|l| range(l.range))
        .collect();
    found.sort();
    assert_eq!(found, ["1:10-1:14", "4:5-4:9"]);
}

#[spec(
    behavior = "prepare_rename",
    verify = "prepare rename answers on a quoted reference, inside its quotes"
)]
fn prepare_rename_on_a_quoted_reference_is_its_id() {
    let served = served(QUOTED);
    let answer = answers::prepare_rename(
        &served.state(),
        &served.uri("main.spec"),
        Position::new(1, 11),
    )
    .unwrap();
    let Some(PrepareRenameResponse::Range(r)) = answer else {
        panic!("a range: {answer:?}");
    };
    assert_eq!(range(r), "1:10-1:14");
}

#[ignore = "flipped by T6"]
#[spec(
    invariant = "cursor_names_one_entity",
    verify = "a quoted reference under the cursor names its entity"
)]
fn a_cursor_inside_a_quoted_reference_names_its_entity() {
    let served = served(QUOTED);
    let (state, uri) = (served.state(), served.uri("main.spec"));
    let at = Position::new(1, 11);

    let Some(GotoDefinitionResponse::Scalar(location)) = answers::definition(&state, &uri, at)
    else {
        panic!("a location");
    };
    assert_eq!(range(location.range), "4:5-4:9");

    let found: Vec<String> = answers::references(&state, &uri, at, false)
        .expect("references")
        .into_iter()
        .map(|l| range(l.range))
        .collect();
    assert_eq!(found, ["1:10-1:14"]);

    let edit = answers::rename(&state, &uri, at, "Store")
        .unwrap()
        .expect("the cursor names Repo");
    let mut edits: Vec<String> = edit
        .changes
        .unwrap()
        .into_values()
        .flatten()
        .map(|e| range(e.range))
        .collect();
    edits.sort();
    assert_eq!(edits, ["1:10-1:14", "4:5-4:9"]);
}
