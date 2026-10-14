//! The LSP's rename (`specforge_lsp::answers::rename`): the shared plan
//! (`specforge_ops::rename`) as a workspace edit, every position from the
//! parser.

use specforge_test_macros::test as spec;

use crate::served::{buffers, uri_of_path};
use specforge_lsp::{LspState, answers};
use tower_lsp::lsp_types::Position;

/// The edits of the rename of the entity declared at `line`:`character` of
/// the open buffer `path` to `new`, as `"file line:start-end"` (0-based
/// lines, UTF-16 columns), in file order.
fn renamed_edits(
    state: &LspState,
    path: &str,
    (line, character): (u32, u32),
    new: &str,
) -> Result<Vec<String>, tower_lsp::jsonrpc::Error> {
    let edit = answers::rename(
        state,
        &uri_of_path(path),
        Position::new(line, character),
        new,
    )?
    .expect("the cursor names an entity");
    let mut edits: Vec<String> = edit
        .changes
        .expect("changes")
        .into_iter()
        .flat_map(|(uri, edits)| {
            edits.into_iter().map(move |e| {
                format!(
                    "{} {}:{}-{}",
                    uri.path(),
                    e.range.start.line,
                    e.range.start.character,
                    e.range.end.character
                )
            })
        })
        .collect();
    edits.sort();
    Ok(edits)
}

const TYPES: &str = "type auth_token \"Token\" {\n}\n";
const AUTH: &str = "behavior user_login \"Login\" {\n  types [auth_token]\n}\n";

// -- rename_entity_id ---------------------------------------------------------

#[spec(
    behavior = "rename_entity_id",
    verify = "rename updates declaration and all references"
)]
fn rename_updates_all_sites() {
    let state = buffers(&[("/p/types.spec", TYPES), ("/p/auth.spec", AUTH)]);
    let edits = renamed_edits(&state, "/p/types.spec", (0, 6), "session_token").unwrap();
    // The declaration's name, and the reference from user_login.
    assert_eq!(edits, ["/p/auth.spec 1:9-19", "/p/types.spec 0:5-15"]);
}

#[spec(
    behavior = "rename_entity_id",
    verify = "rename is atomic — all or nothing"
)]
#[tokio::test]
async fn rename_is_atomic() {
    use crate::session::{Session, uri_of};
    use serde_json::json;

    let dir = tempfile::TempDir::new().unwrap();
    let limit = dir.path().join("limit.spec");
    let login = dir.path().join("login.spec");
    let login_text = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";
    std::fs::write(&limit, "invariant session_limit \"Limit\" {\n}\n").unwrap();
    std::fs::write(&login, login_text).unwrap();
    let (mut session, _) = Session::start(Some(dir.path())).await;
    let login_uri = uri_of(&login);
    session.open(&login_uri, login_text).await;
    session.diagnostics(&login_uri).await;

    let rename = json!({
        "textDocument": {"uri": login_uri},
        "position": {"line": 1, "character": 16},
        "newName": "session_cap",
    });
    let edit = |line: u32, start: u32| {
        json!([{
            "range": {
                "start": {"line": line, "character": start},
                "end": {"line": line, "character": start + 13},
            },
            "newText": "session_cap",
        }])
    };

    // All: one workspace edit renames the declaration and the reference.
    let all = session.request("textDocument/rename", rename.clone()).await;
    assert_eq!(
        all["result"]["changes"],
        json!({ uri_of(&limit): edit(0, 10), login_uri.clone(): edit(1, 14) })
    );

    // Nothing: when the declaration's file can no longer be read, the
    // rename is refused rather than applied to the reference alone.
    std::fs::remove_file(&limit).unwrap();
    let nothing = session.request("textDocument/rename", rename).await;
    assert!(nothing["result"].is_null(), "{nothing}");
    // Said as the protocol's ContentModified: what the edit would apply to
    // is not what the project was compiled from.
    assert_eq!(nothing["error"]["code"], -32801, "{nothing}");
}

/// The edits of a rename are positions in the text the project was compiled
/// from, and apply to the text the editor has: a file whose text changed
/// since the compile (here, on disk, with no change event yet) is not
/// renamed from stale positions.
#[spec(
    behavior = "rename_entity_id",
    verify = "rename is refused as content modified when a file it edits changed since the compile"
)]
#[tokio::test]
async fn rename_waits_for_the_compile_of_a_changed_file() {
    use crate::session::{Session, uri_of};
    use serde_json::json;

    let dir = tempfile::TempDir::new().unwrap();
    let limit = dir.path().join("limit.spec");
    let login = dir.path().join("login.spec");
    let login_text = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";
    std::fs::write(&limit, "invariant session_limit \"Limit\" {\n}\n").unwrap();
    std::fs::write(&login, login_text).unwrap();
    let (mut session, _) = Session::start(Some(dir.path())).await;
    let login_uri = uri_of(&login);
    session.open(&login_uri, login_text).await;
    session.diagnostics(&login_uri).await;
    let rename = json!({
        "textDocument": {"uri": login_uri},
        "position": {"line": 1, "character": 16},
        "newName": "session_cap",
    });

    // A line added above the declaration, as another tool would.
    std::fs::write(&limit, "// moved\ninvariant session_limit \"Limit\" {\n}\n").unwrap();
    let refused = session.request("textDocument/rename", rename).await;
    assert!(refused["result"].is_null(), "{refused}");
    assert_eq!(refused["error"]["code"], -32801, "{refused}");
}

/// The new name follows the shared entity-ID rule: the editor gets an
/// error saying why, and no edit.
#[spec(
    behavior = "rename_entity_id",
    verify = "rename to an illegal entity ID is refused with why"
)]
#[tokio::test]
async fn rename_to_an_illegal_id_is_refused_with_why() {
    use crate::session::{Session, uri_of};
    use serde_json::json;

    let dir = tempfile::TempDir::new().unwrap();
    let login = dir.path().join("login.spec");
    let text = "behavior login \"Login\" {\n}\n";
    std::fs::write(&login, text).unwrap();
    let (mut session, _) = Session::start(Some(dir.path())).await;
    let uri = uri_of(&login);
    session.open(&uri, text).await;
    session.diagnostics(&uri).await;

    let refused = session
        .request(
            "textDocument/rename",
            json!({
                "textDocument": {"uri": uri},
                "position": {"line": 0, "character": 10},
                "newName": "log-in",
            }),
        )
        .await;
    assert!(refused["result"].is_null(), "{refused}");
    let message = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("invalid entity ID 'log-in'"), "{refused}");
}

#[spec(behavior = "rename_entity_id", verify = "rename across multiple files")]
fn rename_across_files() {
    let state = buffers(&[
        ("/p/a.spec", "type tok \"T\" {\n}\n"),
        ("/p/b.spec", "behavior b1 \"B1\" {\n  types [tok]\n}\n"),
        ("/p/c.spec", "behavior b2 \"B2\" {\n  types [tok]\n}\n"),
    ]);
    let edits = renamed_edits(&state, "/p/a.spec", (0, 6), "token").unwrap();
    assert_eq!(
        edits,
        ["/p/a.spec 0:5-8", "/p/b.spec 1:9-12", "/p/c.spec 1:9-12"]
    );
}

#[spec(
    behavior = "rename_entity_id",
    verify = "rename rejects new name that duplicates existing entity ID"
)]
fn rename_rejects_duplicate() {
    let state = buffers(&[("/p/types.spec", TYPES), ("/p/auth.spec", AUTH)]);
    let refused = renamed_edits(&state, "/p/types.spec", (0, 6), "user_login").unwrap_err();
    assert_eq!(refused.code, tower_lsp::jsonrpc::ErrorCode::InvalidParams);
    assert_eq!(
        refused.message,
        "cannot rename 'auth_token': 'user_login' exists"
    );
}
