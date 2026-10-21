//! What the server answers a request, decided over the LSP state alone
//! (`specforge_lsp::answers`): no client, no debounce, no stdio.

use specforge_lsp::{ClientSupport, LspState, answers};
use specforge_project::{Buffer, ProjectSession};
use specforge_test_macros::test as spec;
use tempfile::TempDir;
use tower_lsp::lsp_types::{
    ClientCapabilities, CompletionClientCapabilities, CompletionItemCapability,
    DocumentSymbolClientCapabilities, GotoCapability, GotoDefinitionResponse, Position,
    TextDocumentClientCapabilities, Url,
};

/// The text the project is compiled from, and the buffer after a line is
/// inserted at the top (typed, not yet compiled): `beta` is on line 2 of the
/// buffer and on line 1 of the compile.
const STALE_COMPILED: &str = "type alpha \"A\" {}\ntype beta \"B\" {}\ntype gamma \"C\" {}\n";
const STALE_TYPED: &str =
    "type delta \"D\" {}\ntype alpha \"A\" {}\ntype beta \"B\" {}\ntype gamma \"C\" {}\n";
const LINKED: &str = "type token \"T\" {}\nbehavior login \"L\" {\n  types [token]\n}\n";

/// A state serving the project at `dir` (one file, `main.spec`, holding
/// `text`) with `main.spec` open, and its URI.
fn serving(dir: &TempDir, text: &str) -> (LspState, Url) {
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("main.spec"), text).unwrap();
    let mut state = LspState::new();
    state.set_session(ProjectSession::open(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
    ));
    let uri = Url::from_file_path(dir.path().join("main.spec")).unwrap();
    state.open_document(&uri, text, 1);
    (state, uri)
}

fn declaring(definition: bool, hierarchical: bool, insert_replace: bool) -> ClientCapabilities {
    ClientCapabilities {
        text_document: Some(TextDocumentClientCapabilities {
            definition: Some(GotoCapability {
                link_support: Some(definition),
                ..Default::default()
            }),
            document_symbol: Some(DocumentSymbolClientCapabilities {
                hierarchical_document_symbol_support: Some(hierarchical),
                ..Default::default()
            }),
            completion: Some(CompletionClientCapabilities {
                completion_item: Some(CompletionItemCapability {
                    insert_replace_support: Some(insert_replace),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[spec(
    behavior = "go_to_definition",
    verify = "a definition is a location link for a client that declares linkSupport, else a location at the name"
)]
fn client_support_reads_what_the_client_declared() {
    assert_eq!(
        ClientSupport::of(&ClientCapabilities::default()),
        ClientSupport::default()
    );
    for (flags, expected) in [
        ((true, false, false), (true, false, false)),
        ((false, true, false), (false, true, false)),
        ((false, false, true), (false, false, true)),
    ] {
        let support = ClientSupport::of(&declaring(flags.0, flags.1, flags.2));
        assert_eq!(
            (
                support.definition_links,
                support.hierarchical_symbols,
                support.insert_replace
            ),
            expected
        );
    }

    let dir = TempDir::new().unwrap();
    let (mut state, uri) = serving(&dir, LINKED);
    let cursor = Position::new(2, 10);

    state.set_client(ClientSupport::of(&declaring(true, false, false)));
    let Some(GotoDefinitionResponse::Link(links)) = answers::definition(&state, &uri, cursor)
    else {
        panic!("a client declaring linkSupport gets a link");
    };
    let range = |r: tower_lsp::lsp_types::Range| {
        (r.start.line, r.start.character, r.end.line, r.end.character)
    };
    assert_eq!(links.len(), 1);
    assert_eq!(range(links[0].target_range), (0, 0, 0, 17));
    assert_eq!(range(links[0].target_selection_range), (0, 5, 0, 10));
    assert_eq!(
        links[0].origin_selection_range.map(range),
        Some((2, 9, 2, 14))
    );

    state.set_client(ClientSupport::of(&declaring(false, false, false)));
    let Some(GotoDefinitionResponse::Scalar(location)) = answers::definition(&state, &uri, cursor)
    else {
        panic!("any other client gets a location");
    };
    assert_eq!(location.uri, uri);
    assert_eq!(range(location.range), (0, 5, 0, 10));
}

/// A state serving `STALE_COMPILED` with `main.spec` open and then typed in
/// (`STALE_TYPED`): the stale window, before the compile.
fn typed_since_the_compile(dir: &TempDir) -> (LspState, Url) {
    let (mut state, uri) = serving(dir, STALE_COMPILED);
    state.change_document(&uri, 2, &[crate::whole(STALE_TYPED)]);
    (state, uri)
}

/// The compile catches up with the buffer.
fn compile(state: &mut LspState) {
    let session = state.session_mut().expect("held");
    let path = session
        .project()
        .environment()
        .spec_root()
        .join("main.spec");
    session.hold(&[Buffer::new(path, STALE_TYPED)]);
}

fn quad(r: tower_lsp::lsp_types::Range) -> (u32, u32, u32, u32) {
    (r.start.line, r.start.character, r.end.line, r.end.character)
}

#[spec(
    invariant = "cursor_names_one_entity",
    verify = "a cursor in a buffer typed since the compile names what its own word names, never the compiled text's token at its position"
)]
fn a_stale_cursor_names_its_own_word() {
    let dir = TempDir::new().unwrap();
    let (mut state, uri) = typed_since_the_compile(&dir);
    state.set_client(ClientSupport {
        definition_links: true,
        ..ClientSupport::default()
    });
    let on_beta = Position::new(2, 6);
    let on_alpha = Position::new(1, 6);
    let hover_at = |state: &LspState, position| {
        let hover = answers::hover(state, &uri, position).expect("a hover");
        match hover.contents {
            tower_lsp::lsp_types::HoverContents::Markup(markup) => markup.value,
            other => panic!("{other:?}"),
        }
    };

    // The token under the cursor is `beta`; the compiled text has `gamma`
    // at that position.
    let stale_hover = hover_at(&state, on_beta);
    assert!(
        stale_hover.starts_with("**type** `beta` \u{2014} B"),
        "{stale_hover}"
    );
    assert!(
        hover_at(&state, on_alpha).starts_with("**type** `alpha` \u{2014} A"),
        "the buffer's alpha is one line below the compile's"
    );
    // Its declaration lies in the buffer typed since: no location of the compile is a place in it
    // (ADR 0205).
    assert!(answers::definition(&state, &uri, on_beta).is_none());
    assert!(answers::references(&state, &uri, on_beta, true).is_none());

    // The compile catches up: the same entity is named, now placed where
    // the buffer has it.
    compile(&mut state);
    assert_eq!(hover_at(&state, on_beta), stale_hover);
    let Some(GotoDefinitionResponse::Link(links)) = answers::definition(&state, &uri, on_beta)
    else {
        panic!("a link");
    };
    assert_eq!(
        links[0].origin_selection_range.map(quad),
        Some((2, 5, 2, 9))
    );
    assert_eq!(quad(links[0].target_selection_range), (2, 5, 2, 9));
}

fn content_modified<T: std::fmt::Debug>(result: tower_lsp::jsonrpc::Result<T>) {
    let error = result.expect_err("refused");
    assert_eq!(
        error.code,
        tower_lsp::jsonrpc::ErrorCode::ServerError(-32801),
        "{error:?}"
    );
}

#[spec(
    behavior = "prepare_rename",
    verify = "prepare rename over a buffer typed since the compile is refused as content modified"
)]
fn prepare_rename_waits_for_the_compile_of_its_document() {
    let dir = TempDir::new().unwrap();
    let (mut state, uri) = typed_since_the_compile(&dir);
    content_modified(answers::prepare_rename(&state, &uri, Position::new(2, 6)));

    compile(&mut state);
    let Ok(Some(tower_lsp::lsp_types::PrepareRenameResponse::Range(range))) =
        answers::prepare_rename(&state, &uri, Position::new(2, 6))
    else {
        panic!("a range once compiled");
    };
    assert_eq!(quad(range), (2, 5, 2, 9));
}

#[spec(
    behavior = "rename_entity_id",
    verify = "rename from a buffer typed since the compile is refused as content modified"
)]
fn rename_waits_for_the_compile_of_its_document() {
    let dir = TempDir::new().unwrap();
    let (mut state, uri) = typed_since_the_compile(&dir);
    // The entity has no occurrence in any other file, yet the rename is
    // refused: its edits are positions in a text the editor no longer has.
    content_modified(answers::rename(&state, &uri, Position::new(2, 6), "bravo"));

    compile(&mut state);
    let edit = answers::rename(&state, &uri, Position::new(2, 6), "bravo")
        .expect("planned")
        .expect("an edit");
    let edits = &edit.changes.unwrap()[&uri];
    assert_eq!(edits.len(), 1);
    assert_eq!(quad(edits[0].range), (2, 5, 2, 9));
}

#[test]
fn formatting_answers_its_edits_and_what_to_publish() {
    let text = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n\n}}}\n";
    let mut state = crate::served::buffers(&[("/buffer/format_keeps.spec", text)]);
    let uri = crate::served::uri_of_path("/buffer/format_keeps.spec");
    let options = tower_lsp::lsp_types::FormattingOptions {
        tab_size: 2,
        insert_spaces: true,
        ..Default::default()
    };

    let formatted = answers::formatting(
        &state,
        &specforge_project::RuntimeSource::Fixed(None),
        &uri,
        &options,
        None,
    )
    .expect("an open document");
    let codes_of = |list: &[tower_lsp::lsp_types::Diagnostic]| {
        let mut codes: Vec<String> = list
            .iter()
            .filter_map(|d| match d.code.as_ref()? {
                tower_lsp::lsp_types::NumberOrString::String(code) => Some(code.clone()),
                tower_lsp::lsp_types::NumberOrString::Number(_) => None,
            })
            .collect();
        codes.sort();
        codes
    };
    assert_eq!(codes_of(&formatted.findings.sent), ["W142"]);
    assert_eq!(formatted.findings.version, Some(1));
    // The formatter's W142 goes beside the compile's E001 and E003, which
    // a publish would otherwise erase.
    let send = state
        .publish_formatting(formatted.findings.clone())
        .expect("formatting reported something");
    assert_eq!(codes_of(&send.diagnostics), ["E001", "E003", "W142"]);
    assert_eq!((send.uri, send.version), (uri.clone(), Some(1)));
    assert_eq!(
        state.publish_formatting(formatted.findings),
        None,
        "nothing changed"
    );
    // Outside a project the editor's settings are the ones that apply.
    assert_eq!(formatted.notice, None);

    // A document that is not open is not formatted.
    let closed = crate::served::uri_of_path("/buffer/closed.spec");
    assert!(
        answers::formatting(
            &state,
            &specforge_project::RuntimeSource::Fixed(None),
            &closed,
            &options,
            None
        )
        .is_none()
    );
}

#[test]
fn client_support_reads_what_steers_the_reaction() {
    let declared: ClientCapabilities = serde_json::from_value(serde_json::json!({
        "workspace": {
            "semanticTokens": {"refreshSupport": true},
            "didChangeWatchedFiles": {"relativePatternSupport": true},
        },
    }))
    .unwrap();
    let support = ClientSupport::of(&declared);
    assert!(support.tokens_refresh);
    assert!(support.relative_patterns);

    let nothing = ClientSupport::of(&ClientCapabilities::default());
    assert!(!nothing.tokens_refresh);
    assert!(!nothing.relative_patterns);
}

/// A body left unclosed while a field's name is typed, then two types.
const HALF_TYPED: &str =
    "type alpha \"A\" {\n  note\n\ntype beta \"B\" {}\n\ntype gamma \"C\" {}\n";

#[spec(
    behavior = "go_to_definition",
    verify = "the entity after a body being typed is navigated to"
)]
fn the_entity_after_a_half_typed_body_is_navigable() {
    let dir = TempDir::new().unwrap();
    let (mut state, uri) = serving(&dir, HALF_TYPED);
    state.set_client(ClientSupport {
        definition_links: true,
        ..ClientSupport::default()
    });
    let on_beta = Position::new(3, 6);
    let Some(GotoDefinitionResponse::Link(links)) = answers::definition(&state, &uri, on_beta)
    else {
        panic!("a link");
    };
    assert_eq!(quad(links[0].target_selection_range), (3, 5, 3, 9));
    let hover = answers::hover(&state, &uri, on_beta).expect("a hover");
    let tower_lsp::lsp_types::HoverContents::Markup(markup) = hover.contents else {
        panic!("markup");
    };
    assert!(
        markup.value.starts_with("**type** `beta`"),
        "{}",
        markup.value
    );
}
