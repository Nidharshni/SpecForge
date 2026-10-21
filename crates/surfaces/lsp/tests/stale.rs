//! What an answer says over a document that is not the text the project was compiled from
//! (`specforge_lsp::answers`, ADR 0023 D3, D7): typed since (the compile is coming), or a file the
//! project does not build (none is).

use specforge_lsp::changes::Change;
use specforge_lsp::{ClientSupport, answers};
use specforge_test_macros::test as spec;
use tempfile::TempDir;
use tower_lsp::lsp_types::{
    DocumentSymbol, DocumentSymbolResponse, FileChangeType, FileEvent, GotoDefinitionResponse,
    Location, Position, Url,
};

use crate::served::Served;

const LIMIT: &str = "invariant session_limit \"Limit\" {\n}\n";
const OUTSIDE: &str = "invariant outside_one \"Out\" {\n}\n";

/// A document outside the project's root, opened as `didOpen` opens it: the session holds its buffer
/// and builds nothing from it (`ProjectSession::hold_with`).
#[spec(
    behavior = "prepare_rename",
    verify = "prepare rename from a document the project does not build answers that rename is not available"
)]
fn a_rename_from_a_document_outside_the_root_answers_nothing() {
    let mut served = Served::new(&[("limit.spec", LIMIT)]).open(&["limit.spec"]);
    let elsewhere = TempDir::new().unwrap();
    let path = elsewhere.path().join("x.spec");
    std::fs::write(&path, OUTSIDE).unwrap();
    let uri = Url::from_file_path(&path).unwrap();
    served.open_document(&uri, OUTSIDE);
    let state = served.state();
    let prepared = answers::prepare_rename(&state, &uri, Position::new(0, 12));
    assert!(matches!(prepared, Ok(None)), "{prepared:?}");
    let renamed = answers::rename(&state, &uri, Position::new(0, 12), "outside_two");
    assert!(matches!(renamed, Ok(None)), "{renamed:?}");
}

/// A document inside the root that the configuration excludes.
#[spec(
    behavior = "rename_entity_id",
    verify = "rename from a document the project does not build answers no edit"
)]
fn a_rename_from_an_excluded_document_answers_nothing() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[],"exclude":["drafts/**"]}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("limit.spec"), LIMIT).unwrap();
    std::fs::create_dir(dir.path().join("drafts")).unwrap();
    std::fs::write(dir.path().join("drafts/d.spec"), OUTSIDE).unwrap();
    let served = Served::at(dir).open(&["drafts/d.spec"]);
    let uri = served.uri("drafts/d.spec");
    let state = served.state();
    let prepared = answers::prepare_rename(&state, &uri, Position::new(0, 12));
    assert!(matches!(prepared, Ok(None)), "{prepared:?}");
    let renamed = answers::rename(&state, &uri, Position::new(0, 12), "outside_two");
    assert!(matches!(renamed, Ok(None)), "{renamed:?}");
}

/// A source opened before its compile (the debounce window): its compile is coming, so the rename
/// waits for it.
#[spec(
    behavior = "prepare_rename",
    verify = "prepare rename from a document opened since the compile waits for it"
)]
fn a_rename_from_a_document_opened_since_the_compile_waits_for_it() {
    let mut served = Served::new(&[("limit.spec", LIMIT)]).open(&["limit.spec"]);
    // Written after the project opened, and held as `didOpen` holds it, not reacted to yet.
    served.write("fresh.spec", OUTSIDE);
    served.hold("fresh.spec");
    let uri = served.uri("fresh.spec");
    let refused = answers::prepare_rename(&served.state(), &uri, Position::new(0, 12));
    assert!(
        matches!(&refused, Err(e) if e.code == tower_lsp::jsonrpc::ErrorCode::ServerError(-32801)),
        "{refused:?}"
    );
    served.apply(Change::Edited(vec![uri.clone()]));
    assert!(matches!(
        answers::prepare_rename(&served.state(), &uri, Position::new(0, 12)),
        Ok(Some(_))
    ));
}

const TYPO: &str = "behavior login \"Login\" {\n  invariants [session_limt]\n}\n";

/// A line typed above the block: the compile's E003 is at line 1 of the text it read, and line 1 of
/// the buffer is the block's header now.
#[spec(
    behavior = "hover_diagnostic",
    verify = "hovering a buffer typed since the compile shows none of the compile's diagnostics"
)]
fn hover_over_a_buffer_typed_since_shows_no_diagnostic_of_the_compile() {
    let mut served =
        Served::new(&[("limit.spec", LIMIT), ("login.spec", TYPO)]).open(&["login.spec"]);
    assert!(
        served
            .hover("login.spec", 1, 18)
            .is_some_and(|md| md.contains("E003"))
    );

    served.type_text("login.spec", &format!("// typed\n{TYPO}"));
    for line in [1, 2] {
        let md = served.hover("login.spec", line, 18);
        assert!(
            !md.as_deref().is_some_and(|md| md.contains("E003")),
            "line {line}: {md:?}"
        );
    }

    served.apply(Change::Edited(vec![served.uri("login.spec")]));
    assert!(
        served
            .hover("login.spec", 2, 18)
            .is_some_and(|md| md.contains("E003"))
    );
}

/// `alpha` and `login` in a.spec, `beta` in b.spec; login's types name both.
const ALPHA: &str = "type alpha \"A\" {}\nbehavior login \"L\" {\n  types [alpha, beta]\n}\n";
const BETA: &str = "type beta \"B\" {}\n";

/// The project of `ALPHA` and `BETA`, a.spec open, then three lines typed above a.spec's text: every
/// block of it is three lines lower in the buffer than in the compile, and nothing is compiled (the
/// stale window).
fn typed_three_lines_above(client: ClientSupport) -> Served {
    let mut served = Served::new(&[("a.spec", ALPHA), ("b.spec", BETA)])
        .client(client)
        .open(&["a.spec"]);
    served.type_text("a.spec", &format!("\n\n\n{ALPHA}"));
    served
}

/// (file name, start line, start character) of each location, sorted.
fn places(locations: impl IntoIterator<Item = Location>) -> Vec<(String, u32, u32)> {
    let mut places: Vec<_> = locations
        .into_iter()
        .map(|l| {
            let name = l.uri.path_segments().unwrap().next_back().unwrap();
            (
                name.to_string(),
                l.range.start.line,
                l.range.start.character,
            )
        })
        .collect();
    places.sort();
    places
}

/// The start line of every symbol of an outline, flat or nested; none for `None`.
fn symbol_lines(answer: Option<DocumentSymbolResponse>) -> Vec<u32> {
    fn nested(symbols: &[DocumentSymbol], lines: &mut Vec<u32>) {
        for symbol in symbols {
            lines.push(symbol.range.start.line);
            nested(symbol.children.as_deref().unwrap_or_default(), lines);
        }
    }
    let mut lines = Vec::new();
    match answer {
        Some(DocumentSymbolResponse::Flat(flat)) => {
            lines.extend(flat.iter().map(|s| s.location.range.start.line));
        }
        Some(DocumentSymbolResponse::Nested(tree)) => nested(&tree, &mut lines),
        None => {}
    }
    lines.sort();
    lines
}

/// The location a definition answers (a `Scalar`, the default client), if any.
fn definition_at(
    served: &Served,
    file: &str,
    line: u32,
    character: u32,
) -> Option<(String, u32, u32)> {
    let uri = served.uri(file);
    let answer = answers::definition(&served.state(), &uri, Position::new(line, character));
    match answer {
        Some(GotoDefinitionResponse::Scalar(location)) => places([location]).pop(),
        None => None,
        other => panic!("{other:?}"),
    }
}

fn place(file: &str, line: u32, character: u32) -> (String, u32, u32) {
    (file.to_string(), line, character)
}

#[spec(
    invariant = "lsp_utf16_positions",
    verify = "an outline of a buffer typed since the compile places no symbol at a line of the compiled text"
)]
fn an_outline_of_a_buffer_typed_since_places_no_symbol_at_a_compiled_line() {
    for hierarchical in [false, true] {
        let client = ClientSupport {
            hierarchical_symbols: hierarchical,
            ..Default::default()
        };
        let mut served = typed_three_lines_above(client);
        let a = served.uri("a.spec");
        let lines = symbol_lines(answers::document_symbols(&served.state(), &a));
        assert_eq!(lines, Vec::<u32>::new(), "hierarchical {hierarchical}");
        served.apply(Change::Edited(vec![a.clone()]));
        let lines = symbol_lines(answers::document_symbols(&served.state(), &a));
        assert_eq!(lines, vec![3, 4], "hierarchical {hierarchical}");
    }
}

#[spec(
    invariant = "lsp_utf16_positions",
    verify = "references from a buffer typed since leave out its lines and keep those of files that are the compiled text"
)]
fn references_from_a_buffer_typed_since_leave_out_its_compiled_lines() {
    let mut served = typed_three_lines_above(ClientSupport::default());
    let a = served.uri("a.spec");
    let on_beta = Position::new(5, 17);
    let found = answers::references(&served.state(), &a, on_beta, true).unwrap_or_default();
    assert_eq!(places(found), vec![place("b.spec", 0, 5)]);
    served.apply(Change::Edited(vec![a.clone()]));
    let found = answers::references(&served.state(), &a, on_beta, true).unwrap_or_default();
    assert_eq!(
        places(found),
        vec![place("a.spec", 5, 16), place("b.spec", 0, 5)]
    );
}

#[spec(
    invariant = "lsp_utf16_positions",
    verify = "a definition in a buffer typed since is not placed at its compiled position; one in a file that is the compiled text is"
)]
fn a_definition_in_a_buffer_typed_since_is_not_placed_at_its_compiled_line() {
    let mut served = typed_three_lines_above(ClientSupport::default());
    assert_eq!(definition_at(&served, "a.spec", 5, 10), None);
    assert_eq!(
        definition_at(&served, "a.spec", 5, 17),
        Some(place("b.spec", 0, 5))
    );
    let a = served.uri("a.spec");
    served.apply(Change::Edited(vec![a]));
    assert_eq!(
        definition_at(&served, "a.spec", 5, 10),
        Some(place("a.spec", 3, 5))
    );
}

#[spec(
    invariant = "lsp_utf16_positions",
    verify = "workspace symbols leave out the entities of a buffer typed since the compile"
)]
fn workspace_symbols_leave_out_a_buffer_typed_since() {
    let mut served = typed_three_lines_above(ClientSupport::default());
    let found = |served: &Served, query: &str| {
        let symbols = answers::workspace_symbols(&served.state(), query);
        places(symbols.unwrap_or_default().into_iter().map(|s| s.location))
    };
    assert_eq!(found(&served, "alpha"), vec![]);
    assert_eq!(found(&served, "beta"), vec![place("b.spec", 0, 0)]);
    let a = served.uri("a.spec");
    served.apply(Change::Edited(vec![a]));
    assert_eq!(found(&served, "alpha"), vec![place("a.spec", 3, 0)]);
}

#[spec(
    invariant = "lsp_utf16_positions",
    verify = "a list leaves out a closed file changed on disk since the compile until its change is compiled"
)]
fn a_list_leaves_out_a_closed_file_changed_on_disk_since_the_compile() {
    let mut served = Served::new(&[("a.spec", ALPHA), ("b.spec", BETA)]).open(&["a.spec"]);
    served.write("b.spec", &format!("// one\n// two\n{BETA}"));
    let a = served.uri("a.spec");
    let on_beta = Position::new(2, 17);
    let found = answers::references(&served.state(), &a, on_beta, true).unwrap_or_default();
    assert_eq!(places(found), vec![place("a.spec", 2, 16)]);
    assert_eq!(definition_at(&served, "a.spec", 2, 17), None);
    let symbols = answers::workspace_symbols(&served.state(), "beta");
    assert!(symbols.is_none(), "{symbols:?}");
    let event = FileEvent {
        uri: served.uri("b.spec"),
        typ: FileChangeType::CHANGED,
    };
    served.apply(Change::Watched(vec![event]));
    assert_eq!(
        definition_at(&served, "a.spec", 2, 17),
        Some(place("b.spec", 2, 5))
    );
}
