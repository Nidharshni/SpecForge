//! A duplicate declaration (E002) as the LSP serves it: its references are tokens like any other (ADR 0176).
use specforge_lsp::answers;
use specforge_test_macros::test as spec;
use tower_lsp::lsp_types::{Position, PrepareRenameResponse};

use crate::defaults::served;

const DUPLICATE: &str = "behavior a \"A\" {\n  ports [Repo]\n}\n\nbehavior a \"Again\" {\n  ports [Repo]\n}\n\n\
                         port Repo \"Repo\" {\n}\n";

#[spec(
    behavior = "prepare_rename",
    verify = "prepare rename answers on a reference in a duplicate declaration"
)]
#[ignore = "flipped by T4"]
fn prepare_rename_on_a_reference_in_a_duplicate_declaration_has_a_rename_range() {
    let served = served(DUPLICATE);
    let (state, uri) = (served.state(), served.uri("main.spec"));
    let answer = answers::prepare_rename(&state, &uri, Position::new(5, 10)).unwrap();
    let Some(PrepareRenameResponse::Range(range)) = answer else {
        panic!("{answer:?}")
    };
    assert_eq!(
        (range.start, range.end),
        (Position::new(5, 9), Position::new(5, 13))
    );
    // The duplicate's own name is not what a rename of `a` edits (the winner's is): it stays unavailable.
    assert!(
        answers::prepare_rename(&state, &uri, Position::new(4, 9))
            .unwrap()
            .is_none()
    );
}
