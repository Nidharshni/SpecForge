use super::*;

#[tokio::test]
async fn e2e_formatting_returns_edits() {
    // Badly formatted: extra spaces, wrong indentation
    let text = "behavior  foo   \"Foo\"  {\ncontract \"test\"\n}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    let resp = client.formatting(&uri, 2).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected formatting edits");
    let edits = result.as_array().unwrap();
    assert!(
        !edits.is_empty(),
        "Expected at least one formatting edit for badly formatted input"
    );
}

#[tokio::test]
async fn e2e_range_formatting() {
    let text = "behavior foo \"Foo\" {\ncontract \"test\"\n}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Format only lines 0-2
    let resp = client.range_formatting(&uri, 2, 0, 2).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected range formatting result");
    let edits = result.as_array().unwrap();
    // If there are edits, they should be within the specified range
    for edit in edits {
        let start_line = edit["range"]["start"]["line"].as_u64().unwrap();
        let end_line = edit["range"]["end"]["line"].as_u64().unwrap();
        assert!(
            start_line <= 2 && end_line <= 2,
            "Edits should be within range (0-2), got lines {start_line}-{end_line}"
        );
    }
}
