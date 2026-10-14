use super::*;
use tempfile::TempDir;

#[tokio::test]
async fn e2e_goto_definition_entity() {
    let text = "type token \"Token\" {}\nbehavior login \"Login\" {\n  types [token]\n}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // "token" on line 2, col ~10 (inside [token])
    let resp = client.goto_definition(&uri, 2, 10).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected definition result");
    // Should point to line 0 (0-based) where "type token" is defined
    assert_eq!(result["range"]["start"]["line"], 0);
}

#[tokio::test]
async fn e2e_goto_definition_nonexistent() {
    let text = "behavior foo \"Foo\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Position past end of meaningful content
    let resp = client.goto_definition(&uri, 0, 50).await;
    let result = &resp["result"];
    assert!(result.is_null(), "Expected null for nonexistent position");
}

#[tokio::test]
async fn e2e_goto_definition_cross_file() {
    let dir = TempDir::new().unwrap();
    let file_a = dir.path().join("a.spec");
    let file_b = dir.path().join("b.spec");
    std::fs::write(&file_b, "type token \"Token\" {}\n").unwrap();
    std::fs::write(&file_a, "behavior login \"Login\" {\n  types [token]\n}\n").unwrap();

    let root = dir.path().to_str().unwrap();
    let mut client = Session::launch(Some(root), json!({})).await.0;

    // Wait for workspace indexing log
    client
        .wait_for_notification("window/logMessage", 5000)
        .await;

    let uri_a = tower_lsp::lsp_types::Url::from_file_path(&file_a)
        .unwrap()
        .to_string();
    let text_a = std::fs::read_to_string(&file_a).unwrap();
    client.did_open(&uri_a, "specforge", &text_a).await;
    client
        .wait_for_notification("textDocument/publishDiagnostics", 5000)
        .await;

    // goto_definition on "token" in file A should point to file B
    let resp = client.goto_definition(&uri_a, 1, 10).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected cross-file definition");
    let target_uri = result["uri"].as_str().unwrap();
    assert!(
        target_uri.contains("b.spec"),
        "Expected definition in b.spec, got: {target_uri}"
    );
}

#[tokio::test]
async fn e2e_goto_definition_on_use_line() {
    let dir = TempDir::new().unwrap();
    let types_dir = dir.path().join("types");
    std::fs::create_dir_all(&types_dir).unwrap();
    std::fs::write(types_dir.join("core.spec"), "type token \"Token\" {}\n").unwrap();

    let root = dir.path().to_str().unwrap();
    let mut client = Session::launch(Some(root), json!({})).await.0;
    client
        .wait_for_notification("window/logMessage", 5000)
        .await;

    let main_file = dir.path().join("main.spec");
    let main_uri = tower_lsp::lsp_types::Url::from_file_path(&main_file)
        .unwrap()
        .to_string();
    client
        .did_open(&main_uri, "specforge", "use \"types/core\"\n")
        .await;
    client
        .wait_for_notification("textDocument/publishDiagnostics", 5000)
        .await;

    // Cursor on the import path (line 0, col 6 = inside "types/core")
    let resp = client.goto_definition(&main_uri, 0, 7).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected definition for use import");
    let target_uri = result["uri"].as_str().unwrap();
    assert!(
        target_uri.contains("core.spec"),
        "Expected definition pointing to types/core.spec, got: {target_uri}"
    );
}

#[tokio::test]
async fn e2e_references_returns_all_sites() {
    let text = concat!(
        "type token \"Token\" {}\n",
        "behavior a \"A\" {\n",
        "  types [token]\n",
        "}\n",
        "behavior b \"B\" {\n",
        "  types [token]\n",
        "}\n",
    );
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // "token" at line 0, col 6 (the declaration)
    let resp = client.references(&uri, 0, 6).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected references result");
    let refs = result.as_array().unwrap();
    // declaration + 2 references = 3
    assert!(
        refs.len() >= 3,
        "Expected at least 3 reference locations, got {}",
        refs.len()
    );
}

#[tokio::test]
async fn e2e_references_includes_declaration() {
    let text = concat!(
        "type token \"Token\" {}\n",
        "behavior a \"A\" {\n",
        "  types [token]\n",
        "}\n",
    );
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    let resp = client.references(&uri, 0, 6).await;
    let result = &resp["result"];
    assert!(!result.is_null());
    let refs = result.as_array().unwrap();
    // At least one reference should be at line 0 (the declaration)
    let has_decl = refs
        .iter()
        .any(|r| r["range"]["start"]["line"].as_u64() == Some(0));
    assert!(has_decl, "Expected declaration in references");
}

#[tokio::test]
async fn e2e_references_nonexistent() {
    let text = "behavior foo \"Foo\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Position on a word that isn't an entity ID in the graph references
    let resp = client.references(&uri, 0, 50).await;
    let result = &resp["result"];
    assert!(result.is_null(), "Expected null for nonexistent references");
}

// -- answers from shared navigation (plan 05) ---------------------------------

/// `"line:char-line:char"` of an LSP range, 0-based UTF-16.
fn lsp_range(range: &Value) -> String {
    format!(
        "{}:{}-{}:{}",
        range["start"]["line"],
        range["start"]["character"],
        range["end"]["line"],
        range["end"]["character"]
    )
}

/// The ranges of a `Location[]` result, sorted.
fn ranges(result: &Value) -> Vec<String> {
    let mut ranges: Vec<String> = result
        .as_array()
        .map(|l| l.iter().map(|l| lsp_range(&l["range"])).collect())
        .unwrap_or_default();
    ranges.sort();
    ranges
}

const LIMIT_AND_LOGIN: &str = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n\
                               behavior login \"Login\" {\n  invariants [session_limit]\n}\n\
                               behavior audit \"Audit\" {\n  invariants [session_limit]\n}\n";

#[spec(
    behavior = "find_all_references",
    verify = "Find All References: find all references holds — graph_available, all_references_returned, declaration_included"
)]
#[tokio::test]
async fn find_all_references_contract() {
    // Requires: the project's graph is built, with the entity's references.
    let (mut client, uri, _dir) =
        Session::with_extensions(&["@specforge/software"], "a.spec", LIMIT_AND_LOGIN).await;
    // Ensures: every reference, each its token; the declaration's name
    // only when the request includes it.
    let with = client.references_with(&uri, 0, 12, true).await;
    assert_eq!(
        ranges(&with["result"]),
        ["0:10-0:23", "4:14-4:27", "7:14-7:27"],
        "{with}"
    );
    let without = client.references_with(&uri, 0, 12, false).await;
    assert_eq!(
        ranges(&without["result"]),
        ["4:14-4:27", "7:14-7:27"],
        "{without}"
    );
    // From a reference, the same answer; what login refers to is not a
    // reference to login.
    let from_reference = client.references_with(&uri, 4, 16, false).await;
    assert_eq!(
        ranges(&from_reference["result"]),
        ranges(&without["result"])
    );
    let login = client.references_with(&uri, 3, 10, false).await;
    assert!(login["result"].is_null(), "{login}");
}

#[spec(
    behavior = "find_all_references",
    verify = "reference ranges are UTF-16 after multi-byte text"
)]
#[tokio::test]
async fn e2e_reference_ranges_are_utf16() {
    let text = "invariant cap \"é→\" { guarantee \"x\" }\nbehavior b \"é→\" { invariants [cap] }\n";
    let (mut client, uri, _dir) =
        Session::with_extensions(&["@specforge/software"], "a.spec", text).await;
    let resp = client.references_with(&uri, 0, 11, true).await;
    // `cap` follows "é→" on line 1: 2 UTF-16 units, 5 bytes.
    assert_eq!(
        ranges(&resp["result"]),
        ["0:10-0:13", "1:30-1:33"],
        "{resp}"
    );
}

#[spec(
    behavior = "prepare_rename",
    verify = "prepare rename on entity ID returns token range"
)]
#[tokio::test]
async fn e2e_prepare_rename_answers_the_token() {
    let (mut client, uri, _dir) =
        Session::with_extensions(&["@specforge/software"], "a.spec", LIMIT_AND_LOGIN).await;
    let declaration = client.rename_range_at(&uri, 0, 15).await;
    assert_eq!(
        lsp_range(&declaration["result"]),
        "0:10-0:23",
        "{declaration}"
    );
    let reference = client.rename_range_at(&uri, 4, 20).await;
    assert_eq!(lsp_range(&reference["result"]), "4:14-4:27", "{reference}");
}

#[spec(
    behavior = "prepare_rename",
    verify = "prepare rename on non-renameable token returns not available"
)]
#[tokio::test]
async fn e2e_prepare_rename_outside_an_id_is_not_available() {
    let (mut client, uri, _dir) =
        Session::with_extensions(&["@specforge/software"], "a.spec", LIMIT_AND_LOGIN).await;
    // The title, the kind keyword, a field name.
    for (line, character) in [(0, 27), (0, 3), (1, 4)] {
        let resp = client.rename_range_at(&uri, line, character).await;
        assert!(resp["result"].is_null(), "{line}:{character} {resp}");
    }
}

#[tokio::test]
async fn e2e_goto_definition_selects_the_name() {
    let (mut client, uri, _dir) =
        Session::with_extensions(&["@specforge/software"], "a.spec", LIMIT_AND_LOGIN).await;
    let resp = client.goto_definition(&uri, 4, 16).await;
    assert_eq!(lsp_range(&resp["result"]["range"]), "0:10-0:23", "{resp}");
}

#[tokio::test]
async fn e2e_goto_definition_links_the_block_and_its_name() {
    let capabilities = json!({"textDocument": {"definition": {"linkSupport": true}}});
    let (mut client, uri, _dir, _) = Session::with_extensions_as(
        &["@specforge/software"],
        "a.spec",
        LIMIT_AND_LOGIN,
        capabilities,
    )
    .await;
    let resp = client.goto_definition(&uri, 4, 16).await;
    let link = &resp["result"][0];
    assert_eq!(lsp_range(&link["targetRange"]), "0:0-2:1", "{resp}");
    assert_eq!(
        lsp_range(&link["targetSelectionRange"]),
        "0:10-0:23",
        "{resp}"
    );
    assert_eq!(
        lsp_range(&link["originSelectionRange"]),
        "4:14-4:27",
        "{resp}"
    );
}
