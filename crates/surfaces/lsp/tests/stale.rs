//! What an answer says over a document that is not the text the project was compiled from
//! (`specforge_lsp::answers`, ADR 0023 D3, D7): typed since (the compile is coming), or a file the
//! project does not build (none is).

use specforge_lsp::answers;
use specforge_lsp::changes::Change;
use specforge_test_macros::test as spec;
use tempfile::TempDir;
use tower_lsp::lsp_types::{Position, Url};

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
