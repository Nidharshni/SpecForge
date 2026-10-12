//! An open document is one file whatever spelling of its URI the client sends. VS Code
//! percent-encodes `@`, `+`, `=` (and a Windows drive's `:`) where `Url::from_file_path` does
//! not; the server keys the document, its diagnostics and its staleness by the file, and
//! names it to the client as the client named it (ADR 0056).

use serde_json::Value;
use specforge_lsp::answers;
use specforge_test_macros::test as spec;
use tower_lsp::lsp_types::{CodeActionOrCommand, Position, Url};

use crate::lsp_range;
use crate::recorder::{last_codes, publications};
use crate::served::{Served, hover_text};
use crate::session::{Session, codes, uri_of};

/// `login` names `tokn`: an E003 whose fix replaces it with `token`.
const MISSPELT: &str = "type token \"T\" {}\nbehavior login \"L\" {\n  types [tokn]\n}\n";
/// A project file in a directory whose name VS Code percent-encodes.
const FILE: &str = "@scope/main.spec";
/// A parse error formatting keeps verbatim (W142), beside the unresolved reference.
const KEPT: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n\n}}}\n";

/// `file`'s URI as VS Code spells it: `@` percent-encoded (`%40`).
fn encoded(served: &Served, file: &str) -> Url {
    Url::parse(&served.uri(file).as_str().replace('@', "%40")).unwrap()
}

fn titles(actions: &[CodeActionOrCommand]) -> Vec<String> {
    actions
        .iter()
        .map(|action| match action {
            CodeActionOrCommand::CodeAction(action) => action.title.clone(),
            CodeActionOrCommand::Command(command) => command.title.clone(),
        })
        .collect()
}

fn has(codes: Option<Vec<String>>, code: &str) -> bool {
    codes.is_some_and(|codes| codes.iter().any(|c| c == code))
}

#[spec(
    behavior = "document_open_close",
    verify = "a document is one file whatever spelling of its URI the client sends"
)]
fn a_percent_encoded_document_is_published_and_fixed_under_its_own_uri() {
    let mut served = Served::new(&[(FILE, MISSPELT)]).open(&[]);
    let plain = served.uri(FILE);
    let encoded = encoded(&served, FILE);
    served.open_document(&encoded, MISSPELT);
    let sent = served.sent();

    assert!(
        has(last_codes(&sent, &encoded), "E003"),
        "the compile is published under the encoded URI: {:?}",
        last_codes(&sent, &encoded)
    );
    assert!(
        publications(&sent, &plain).is_empty(),
        "nothing goes to the other spelling"
    );

    let state = served.state();
    let actions = answers::code_actions(&state, &encoded, lsp_range(2, 9, 2, 13))
        .map(|actions| titles(&actions))
        .unwrap_or_default();
    assert!(
        actions.iter().any(|t| t == "Replace with 'token'"),
        "{actions:?}"
    );
    let hover = hover_text(&state, &encoded, Position::new(2, 10)).unwrap_or_default();
    assert!(hover.starts_with("**E003**"), "{hover}");
}

#[spec(
    behavior = "rename_entity_id",
    verify = "rename from a buffer typed since the compile is refused as content modified"
)]
fn a_percent_encoded_document_typed_since_the_compile_is_not_renamed() {
    let mut served = Served::new(&[(FILE, MISSPELT)]).open(&[]);
    let encoded = encoded(&served, FILE);
    served.open_document(&encoded, MISSPELT);
    served.type_into(&encoded, &format!("// typed\n{MISSPELT}"));

    let state = served.state();
    let position = Position::new(1, 6);
    let prepared = answers::prepare_rename(&state, &encoded, position);
    let renamed = answers::rename(&state, &encoded, position, "tok");
    for (what, answer) in [
        ("prepareRename", prepared.map(|_| ())),
        ("rename", renamed.map(|_| ())),
    ] {
        match answer {
            Err(e) => assert_eq!(
                e.code,
                tower_lsp::jsonrpc::ErrorCode::ServerError(-32801),
                "{what}"
            ),
            Ok(()) => panic!("{what} answered over a stale buffer"),
        }
    }
}

#[spec(
    behavior = "emit_live_diagnostics",
    verify = "each file's diagnostics are published under the URI the client named it by"
)]
fn closing_a_percent_encoded_document_publishes_under_its_own_uri() {
    let mut served = Served::new(&[(FILE, MISSPELT)]).open(&[]);
    let plain = served.uri(FILE);
    let encoded = encoded(&served, FILE);
    served.open_document(&encoded, MISSPELT);
    served.sent();

    served.close_uri(&encoded);
    let sent = served.sent();
    assert!(
        has(last_codes(&sent, &encoded), "E003"),
        "the file on disk still has the error: {:?}",
        last_codes(&sent, &encoded)
    );
    assert!(publications(&sent, &plain).is_empty());
}

/// The next publication for `uri` labelled `version`.
async fn published(session: &mut Session, uri: &str, version: i64) -> Vec<Value> {
    let params = session
        .notification("textDocument/publishDiagnostics", |p| {
            p["uri"] == uri && p["version"] == version
        })
        .await
        .unwrap_or_else(|| panic!("nothing published for {uri} at version {version}"));
    params["diagnostics"].as_array().unwrap().clone()
}

#[specforge_test_macros::test(
    behavior = "lsp_format_document",
    verify = "formatting keeps the document's compile diagnostics published"
)]
#[tokio::test]
async fn formatting_a_percent_encoded_document_keeps_its_compile_diagnostics() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join("specforge.json"), "{}").unwrap();
    std::fs::create_dir_all(root.join("@scope")).unwrap();
    std::fs::write(root.join("@scope/f.spec"), KEPT).unwrap();
    let (mut session, _) = Session::start(Some(&root)).await;
    let uri = uri_of(&root.join("@scope/f.spec")).replace('@', "%40");
    session.open(&uri, KEPT).await;

    let compiled = published(&mut session, &uri, 1).await;
    assert!(codes(&compiled).contains(&"E001"), "{compiled:?}");

    assert!(session.format(&uri).await.is_array());
    let formatted = published(&mut session, &uri, 1).await;
    let formatted = codes(&formatted);
    assert!(
        formatted.contains(&"E001") && formatted.contains(&"W142"),
        "{formatted:?}"
    );
}

#[spec(
    behavior = "document_open_close",
    verify = "an open document stays itself when an environment load moves its key"
)]
fn a_document_is_found_after_its_key_moves() {
    let text = "type alpha \"Alpha\" {}\n";
    let mut served = Served::new(&[("spec/a.spec", text)]).restore(&[("spec/a.spec", text)]);
    let uri = served.uri("spec/a.spec");
    served = served.open(&[]);
    {
        let state = served.state();
        assert!(state.document(&uri).is_some());
        assert!(answers::document_symbols(&state, &uri).is_some());
        assert!(state.compiled_text("spec/a.spec").is_some());
    }

    // The project's configuration now names a spec root: the same file is keyed anew.
    served.write(
        "specforge.json",
        r#"{"name":"t","version":"0.1.0","extensions":[],"spec_root":"spec"}"#,
    );
    let config = served.uri("specforge.json");
    served.apply(specforge_lsp::changes::Change::Watched(vec![
        tower_lsp::lsp_types::FileEvent::new(config, tower_lsp::lsp_types::FileChangeType::CHANGED),
    ]));
    let state = served.state();
    assert!(state.compiled_text("a.spec").is_some(), "the key moved");
    assert!(state.document(&uri).is_some());
    assert!(answers::document_symbols(&state, &uri).is_some());
    let hover = hover_text(&state, &uri, Position::new(0, 6)).unwrap_or_default();
    assert!(hover.contains("alpha"), "{hover}");
}

#[spec(
    behavior = "emit_live_diagnostics",
    verify = "each file's diagnostics are published under the URI the client named it by"
)]
fn a_publication_follows_the_spelling_the_client_opened_with() {
    let mut served = Served::new(&[(FILE, MISSPELT)]).open(&[]);
    let plain = served.uri(FILE);
    let encoded = encoded(&served, FILE);
    served.open_document(&plain, MISSPELT);
    served.close_uri(&plain);
    served.sent();

    served.open_document(&encoded, MISSPELT);
    let sent = served.sent();
    assert!(
        has(last_codes(&sent, &encoded), "E003"),
        "{:?}",
        last_codes(&sent, &encoded)
    );
    assert!(
        publications(&sent, &plain).is_empty(),
        "nothing goes to the spelling the file was first opened with"
    );
}
