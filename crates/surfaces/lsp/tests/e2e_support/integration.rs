use super::*;
use tempfile::TempDir;

#[tokio::test]
async fn e2e_full_workflow_open_edit_hover_rename() {
    let text = "behavior user_login \"Login\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;

    // 1. Hover works on initial state
    let resp = client.hover(&uri, 0, 12).await;
    assert!(!resp["result"].is_null(), "Initial hover should work");

    // 2. Change the entity
    client
        .did_change(
            &uri,
            2,
            vec![json!({
                "text": "behavior auth_flow \"Auth Flow\" {}\n"
            })],
        )
        .await;
    client
        .wait_for_notification("textDocument/publishDiagnostics", 5000)
        .await;

    // 3. Hover reflects the change
    let resp = client.hover(&uri, 0, 12).await;
    let md = resp["result"]["contents"]["value"].as_str().unwrap();
    assert!(md.contains("auth_flow"), "Hover should reflect edit");

    // 4. Rename: one edit, naming the new ID.
    let resp = client.rename(&uri, 0, 12, "login_flow").await;
    let changes = resp["result"]["changes"].as_object().expect("a rename");
    let edits: Vec<&Value> = changes
        .values()
        .flat_map(|e| e.as_array().unwrap())
        .collect();
    assert_eq!(edits.len(), 1, "{resp}");
    assert_eq!(edits[0]["newText"], "login_flow", "{resp}");

    // 5. Document symbol reflects current state. Rename returns edits for
    // the client to apply: the graph still has the old name until the
    // client sends the didChange.
    let resp = client.document_symbol(&uri).await;
    let names: Vec<&str> = resp["result"]
        .as_array()
        .expect("document symbols")
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert_eq!(names, ["auth_flow"], "{resp}");
}

#[tokio::test]
async fn e2e_graph_serves_all_features() {
    let text = concat!(
        "type token \"Token\" {}\n",
        "behavior login \"Login\" {\n",
        "  types [token]\n",
        "}\n",
    );
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;

    // Hover
    let resp = client.hover(&uri, 0, 6).await;
    let md = resp["result"]["contents"]["value"].as_str().unwrap_or("");
    assert!(
        md.starts_with("**type** `token`"),
        "Hover should work: {resp}"
    );

    // Goto definition: a client with no link support gets a location at the
    // declaration's name.
    let resp = client.goto_definition(&uri, 2, 10).await;
    assert_eq!(
        resp["result"]["range"]["start"]["line"], 0,
        "Goto definition should work: {resp}"
    );

    // References: the declaration and the use.
    let resp = client.references(&uri, 0, 6).await;
    let locations = resp["result"].as_array().expect("References should work");
    assert!(
        locations
            .iter()
            .any(|l| l["range"]["start"] == json!({"line": 2, "character": 9})),
        "{resp}"
    );

    // Completion offers the entity inside the list.
    let resp = client.completion(&uri, 2, 10).await;
    let labels: Vec<&str> = resp["result"]
        .as_array()
        .expect("Completion should work")
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert!(labels.contains(&"token"), "{labels:?}");

    // Document symbols
    let resp = client.document_symbol(&uri).await;
    let names: Vec<&str> = resp["result"]
        .as_array()
        .expect("Document symbols should work")
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert_eq!(names, ["token", "login"], "{resp}");
}

#[tokio::test]
async fn e2e_diagnostics_latency() {
    let mut client = Session::launch(None, json!({})).await.0;
    let uri = "file:///test/latency.spec";

    let start = std::time::Instant::now();
    client
        .did_open(uri, "specforge", "behavior foo \"Foo\" {}\n")
        .await;
    let notif = client
        .wait_for_notification("textDocument/publishDiagnostics", 5000)
        .await;
    let elapsed = start.elapsed();

    assert!(notif.is_some(), "Should receive diagnostics");
    assert!(
        elapsed.as_millis() < 200,
        "Diagnostics should arrive within 200ms, took {}ms",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn e2e_multiple_files_cross_reference() {
    let dir = TempDir::new().unwrap();
    let file_a = dir.path().join("a.spec");
    let file_b = dir.path().join("b.spec");
    std::fs::write(&file_b, "type shared_token \"Token\" {}\n").unwrap();
    std::fs::write(
        &file_a,
        "behavior consumer \"Consumer\" {\n  types [shared_token]\n}\n",
    )
    .unwrap();

    let root = dir.path().to_str().unwrap();
    let mut client = Session::launch(Some(root), json!({})).await.0;
    client
        .wait_for_notification("window/logMessage", 5000)
        .await;

    let uri_a = tower_lsp::lsp_types::Url::from_file_path(&file_a)
        .unwrap()
        .to_string();
    let uri_b = tower_lsp::lsp_types::Url::from_file_path(&file_b)
        .unwrap()
        .to_string();

    let text_a = std::fs::read_to_string(&file_a).unwrap();
    let text_b = std::fs::read_to_string(&file_b).unwrap();
    // Open B first so its entities exist, then A so edges from A→B are built
    client.did_open(&uri_b, "specforge", &text_b).await;
    client
        .wait_for_notification("textDocument/publishDiagnostics", 5000)
        .await;
    client.did_open(&uri_a, "specforge", &text_a).await;
    client
        .wait_for_notification("textDocument/publishDiagnostics", 5000)
        .await;

    // Goto definition from A -> B. The reparse now runs on the blocking pool,
    // so the awaited diagnostic may belong to the previous open; poll until
    // A's index is live rather than assuming a single notification suffices.
    let mut result = serde_json::Value::Null;
    for _ in 0..50 {
        let resp = client.goto_definition(&uri_a, 1, 10).await;
        result = resp["result"].clone();
        if !result.is_null() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(!result.is_null(), "Expected cross-file definition");
    let target = result["uri"].as_str().unwrap();
    assert!(
        target.contains("b.spec"),
        "Definition should point to b.spec"
    );

    // References on B entity -> should include A
    let resp = client.references(&uri_b, 0, 6).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected references");
    let refs = result.as_array().unwrap();
    let has_file_a = refs
        .iter()
        .any(|r| r["uri"].as_str().is_some_and(|u| u.contains("a.spec")));
    assert!(has_file_a, "References should include file A");
}

/// Verify that cross-file references work immediately after workspace indexing,
/// before any file is explicitly opened. This catches the bug where
/// index_workspace adds nodes but doesn't build edges.
#[tokio::test]
async fn e2e_workspace_index_builds_edges_immediately() {
    let dir = TempDir::new().unwrap();
    let file_a = dir.path().join("a.spec");
    let file_b = dir.path().join("b.spec");
    std::fs::write(&file_b, "type token \"Token\" {}\n").unwrap();
    std::fs::write(&file_a, "behavior login \"Login\" {\n  types [token]\n}\n").unwrap();

    let root = dir.path().to_str().unwrap();
    let mut client = Session::launch(Some(root), json!({})).await.0;
    client
        .wait_for_notification("window/logMessage", 5000)
        .await;

    // Open file A and immediately try go-to-definition on 'token' reference
    let uri_a = tower_lsp::lsp_types::Url::from_file_path(&file_a)
        .unwrap()
        .to_string();
    let text_a = std::fs::read_to_string(&file_a).unwrap();
    client.did_open(&uri_a, "specforge", &text_a).await;
    client
        .wait_for_notification("textDocument/publishDiagnostics", 5000)
        .await;

    // Go-to-definition on 'token' (line 1, inside [token])
    let resp = client.goto_definition(&uri_a, 1, 10).await;
    let result = &resp["result"];
    assert!(
        !result.is_null(),
        "Go-to-definition should work immediately after indexing (edges must be built)"
    );
    let target_uri = result["uri"].as_str().unwrap();
    assert!(
        target_uri.contains("b.spec"),
        "Definition should point to b.spec, got: {target_uri}"
    );
}
