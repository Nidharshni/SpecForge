use super::*;

#[tokio::test]
async fn e2e_hover_returns_markdown() {
    let text = "behavior user_login \"User Login\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Hover on "user_login" (line 0, col 12)
    let resp = client.hover(&uri, 0, 12).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected hover result");
    assert_eq!(result["contents"]["kind"], "markdown");
    let value = result["contents"]["value"].as_str().unwrap();
    assert!(value.contains("behavior"), "Hover should mention kind");
    assert!(
        value.contains("user_login"),
        "Hover should mention entity ID"
    );
    assert!(value.contains("User Login"), "Hover should mention title");
}

#[tokio::test]
async fn e2e_hover_includes_reference_count() {
    let text = concat!(
        "type token \"Token\" {}\n",
        "behavior login \"Login\" {\n",
        "  types [token]\n",
        "}\n",
    );
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Hover on "token" at line 0
    let resp = client.hover(&uri, 0, 6).await;
    let result = &resp["result"];
    assert!(!result.is_null());
    let value = result["contents"]["value"].as_str().unwrap();
    assert!(
        value.contains("References:")
            || value.contains("references:")
            || value.contains("Referenced"),
        "Expected reference count in hover, got: {value}"
    );
}

#[tokio::test]
async fn e2e_hover_on_empty_returns_null() {
    let text = "behavior foo \"Foo\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Past end of file
    let resp = client.hover(&uri, 5, 0).await;
    let result = &resp["result"];
    assert!(result.is_null(), "Expected null hover past end of file");
}

#[tokio::test]
async fn e2e_hover_on_non_entity_word() {
    let text = "behavior foo \"Foo\" {\n  contract \"test\"\n}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Hover on "test" inside the string value (line 1, col 13) — not an entity ID or field name
    let resp = client.hover(&uri, 1, 13).await;
    let result = &resp["result"];
    // "test" is just a string value, not an entity ID in the graph, so null
    assert!(
        result.is_null(),
        "Expected null hover for non-entity word 'test'"
    );
}

/// A diagnostic under the cursor: its catalogued meaning comes first,
/// before the entity hover.
#[tokio::test]
async fn e2e_hover_explains_the_diagnostic_under_the_cursor() {
    let text = "behavior login \"Login\" {\n  types [ghost]\n}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // On "ghost", the unresolved reference.
    let resp = client.hover(&uri, 1, 11).await;
    let value = resp["result"]["contents"]["value"]
        .as_str()
        .unwrap_or_else(|| panic!("no hover: {resp}"));
    assert!(
        value.starts_with("**E003** · Unresolved reference"),
        "{value}"
    );
    assert!(
        value.contains(
            "[Documentation](https://github.com/leaderiop/SpecForge/blob/main/docs/diagnostics.md#e003)"
        ),
        "{value}"
    );
}
