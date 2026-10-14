use super::*;

#[tokio::test]
async fn e2e_initialize_returns_all_capabilities() {
    let expected = serde_json::to_value(specforge_lsp::initialize_result()).unwrap();

    let mut client = Session::spawn();
    let resp = client.initialize(None).await;
    assert_eq!(resp["result"], expected);

    // A project with extensions is offered the same capabilities.
    let extended = crate::contracts::project_with(&["@specforge/software", "@specforge/testing"]);
    let (_client, init) = Session::start(Some(extended.path())).await;
    assert_eq!(init, expected);
}

#[tokio::test]
async fn e2e_shutdown_releases_state() {
    let mut client = Session::launch(None, json!({})).await.0;
    // Drain the logMessage notification from initialized()
    client
        .wait_for_notification("window/logMessage", 2000)
        .await;
    let resp = client.shutdown().await;
    // shutdown should return null result (success)
    assert!(
        resp.get("result").is_some(),
        "Expected result field in shutdown response, got: {resp}"
    );
}

#[tokio::test]
async fn e2e_requests_after_shutdown_fail() {
    let (mut client, uri) = Session::with_doc(None, "test.spec", "behavior foo \"Foo\" {}\n").await;
    client.shutdown().await;
    let resp = client.hover(&uri, 0, 10).await;
    // After shutdown, hover should return null result
    let result = &resp["result"];
    assert!(result.is_null(), "Expected null result after shutdown");
}

#[tokio::test]
async fn e2e_did_open_registers_document() {
    let (mut client, uri) = Session::with_doc(None, "test.spec", "behavior foo \"Foo\" {}\n").await;
    // If the document was registered, hover on the entity ID should return info
    let resp = client.hover(&uri, 0, 10).await;
    let result = &resp["result"];
    assert!(
        !result.is_null(),
        "Expected hover result for tracked document"
    );
    let md = result["contents"]["value"].as_str().unwrap();
    assert!(md.contains("foo"), "Hover should mention entity ID 'foo'");
}

#[spec(
    behavior = "lsp_shutdown",
    verify = "requests after shutdown return InvalidRequest"
)]
#[tokio::test]
async fn e2e_requests_after_shutdown_are_invalid() {
    let text = "behavior alpha \"Alpha\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;

    let resp = client.shutdown().await;
    assert!(resp["error"].is_null(), "{resp}");

    let hover = client.hover(&uri, 0, 10).await;
    assert_eq!(hover["error"]["code"], -32600, "{hover}");
}

#[tokio::test]
async fn e2e_a_project_whose_error_line_holds_wide_characters_opens() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("spec")).unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"r","version":"0.1.0"}"#,
    )
    .unwrap();
    // The 57th byte of the line falls inside a three-byte character.
    let text = "ref rust.fn:x// ── Property Entity Kind ──────────────────────\n";
    let path = dir.path().join("spec/main.spec");
    std::fs::write(&path, text).unwrap();

    let (mut client, _) = Session::start(Some(dir.path())).await;
    let uri = uri_of(&path);
    client.open(&uri, text).await;
    let diagnostics = client.diagnostics(&uri).await;
    assert!(codes(&diagnostics).contains(&"E001"), "{diagnostics:?}");
}
