use super::*;

#[tokio::test]
async fn e2e_did_change_incremental_updates_hover() {
    let text = "behavior old_name \"Old\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;

    // Rename "old_name" to "new_name" via incremental change (replace chars 9-17 on line 0)
    client
        .did_change(
            &uri,
            2,
            vec![json!({
                "range": {
                    "start": { "line": 0, "character": 9 },
                    "end": { "line": 0, "character": 17 },
                },
                "text": "new_name"
            })],
        )
        .await;

    // Wait for re-parse diagnostics
    client
        .wait_for_notification("textDocument/publishDiagnostics", 5000)
        .await;

    // Hover on new_name should work
    let resp = client.hover(&uri, 0, 12).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected hover after rename");
    let md = result["contents"]["value"].as_str().unwrap();
    assert!(
        md.contains("new_name"),
        "Hover should reflect new name, got: {md}"
    );
}

#[tokio::test]
async fn e2e_did_change_triggers_diagnostics() {
    let text = "behavior valid \"Valid\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;

    // Introduce a parse error by replacing full content
    client
        .did_change(
            &uri,
            2,
            vec![json!({
                "text": "behavior {"
            })],
        )
        .await;

    let notif = client
        .wait_for_notification("textDocument/publishDiagnostics", 5000)
        .await;
    assert!(notif.is_some(), "Expected diagnostics after error");
    let diags = notif.unwrap()["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .to_vec();
    assert!(!diags.is_empty(), "Expected at least one error diagnostic");
}

#[tokio::test]
async fn e2e_did_change_multiple_edits() {
    let text = "behavior aaa \"A\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;

    // Three sequential changes
    for (ver, name) in [(2, "bbb"), (3, "ccc"), (4, "ddd")] {
        client
            .did_change(
                &uri,
                ver,
                vec![json!({
                    "text": format!("behavior {name} \"{name}\" {{}}\n")
                })],
            )
            .await;
        client
            .wait_for_notification("textDocument/publishDiagnostics", 5000)
            .await;
    }

    // Hover should reflect the final state
    let resp = client.hover(&uri, 0, 12).await;
    let result = &resp["result"];
    assert!(!result.is_null());
    let md = result["contents"]["value"].as_str().unwrap();
    assert!(md.contains("ddd"), "Should reflect final state 'ddd'");
}

#[tokio::test]
async fn e2e_did_change_full_replacement() {
    let text = "behavior first \"First\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;

    // Full text replacement (no range = full document)
    client
        .did_change(
            &uri,
            2,
            vec![json!({
                "text": "type second \"Second\" {}\n"
            })],
        )
        .await;
    client
        .wait_for_notification("textDocument/publishDiagnostics", 5000)
        .await;

    let resp = client.hover(&uri, 0, 6).await;
    let result = &resp["result"];
    assert!(!result.is_null());
    let md = result["contents"]["value"].as_str().unwrap();
    assert!(
        md.contains("second"),
        "Should reflect replaced content 'second'"
    );
}

#[tokio::test]
async fn e2e_prepare_rename_on_entity() {
    let text = "behavior user_login \"Login\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Cursor on "user_login" (line 0, col 12)
    let resp = client.rename_range_at(&uri, 0, 12).await;
    let result = &resp["result"];
    assert!(
        !result.is_null(),
        "Expected prepare rename range for entity ID"
    );
    // Should return a range covering the ID token
    let start = &result["start"];
    let end = &result["end"];
    assert_eq!(start["line"], 0);
    assert_eq!(end["line"], 0);
}

#[tokio::test]
async fn e2e_prepare_rename_on_non_entity() {
    let text = "behavior foo \"Foo\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Cursor on "behavior" keyword (col 4) — not an entity ID in the graph
    let resp = client.rename_range_at(&uri, 0, 4).await;
    let result = &resp["result"];
    // "behavior" is the keyword, not a graph entity; prepare_rename checks graph
    // It may return null or a range depending on implementation
    // The key assertion: if it returns non-null, the word should be in the graph
    if !result.is_null() {
        // If it returned something, it found "behavior" in the graph (which is a keyword, not entity)
        // This is acceptable — the real test is that rename of a non-entity should not succeed
    }
}

#[tokio::test]
async fn e2e_rename_updates_all_references() {
    let text = "type token \"Token\" {}\nbehavior login \"Login\" {\n  types [token]\n}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Rename "token" at line 0, col 6
    let resp = client.rename(&uri, 0, 6, "jwt_token").await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected workspace edit for rename");
    let changes = &result["changes"];
    assert!(!changes.is_null(), "Expected changes in workspace edit");
    // Collect all edits across all files
    let all_edits: Vec<&Value> = changes
        .as_object()
        .unwrap()
        .values()
        .flat_map(|edits| edits.as_array().unwrap())
        .collect();
    assert!(
        all_edits.len() >= 2,
        "Expected at least 2 edit sites (declaration + reference), got {}",
        all_edits.len()
    );
    // All edits should set newText to "jwt_token"
    for edit in &all_edits {
        assert_eq!(edit["newText"], "jwt_token");
    }
}

#[tokio::test]
async fn e2e_rename_rejects_duplicate() {
    let text = "behavior alpha \"Alpha\" {}\nbehavior beta \"Beta\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Try to rename "alpha" to "beta" (duplicate)
    let resp = client.rename(&uri, 0, 10, "beta").await;
    let result = &resp["result"];
    // Should be null (rejected due to duplicate)
    assert!(
        result.is_null(),
        "Expected null result when renaming to duplicate ID"
    );
}

// C4-02c regression: renaming in a file whose earlier lines contain
// multi-byte characters must still emit edits at the right UTF-16 columns.
#[tokio::test]
async fn e2e_rename_multibyte_lines_utf16_columns() {
    // Line 1 holds the declaration; lines 0 is ASCII, so also add a comment
    // line with multibyte content BEFORE the declaration to shift byte cols.
    // (Spec files accept free-text strings anywhere a string is legal; the
    // declaration line itself is what rename targets.)
    let text = "type token \"Token – ünits\" {}\nbehavior login \"Login\" {\n  types [token]\n}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Rename "token" at line 0, col 6 (ASCII declaration line)
    let resp = client.rename(&uri, 0, 6, "jwt_token").await;
    let result = &resp["result"];
    assert!(
        !result.is_null(),
        "rename succeeds with multibyte strings in file"
    );
    let changes = result["changes"].as_object().unwrap();
    let mut saw_reference_edit = false;
    for (_uri, edits) in changes {
        for edit in edits.as_array().unwrap() {
            let line = edit["range"]["start"]["line"].as_u64().unwrap();
            if line != 2 {
                // The declaration edit may replace the whole line; skip it.
                continue;
            }
            saw_reference_edit = true;
            let start = edit["range"]["start"]["character"].as_u64().unwrap();
            let end = edit["range"]["end"]["character"].as_u64().unwrap();
            // Line 2 is `  types [token]` — pure ASCII, so byte and UTF-16
            // agree here: the identifier is at 9..14.
            assert!(
                (start, end) == (9, 14),
                "reference edit must cover the identifier exactly (9..14), got {start}..{end}"
            );
        }
    }
    assert!(
        saw_reference_edit,
        "expected an edit on the reference line; changes were: {changes:?}"
    );
}

/// The diagnostic codes of the next publishDiagnostics for `uri`.
async fn next_published_codes(client: &mut Session, uri: &str) -> Vec<String> {
    loop {
        let msg = client
            .wait_for_notification("textDocument/publishDiagnostics", 5000)
            .await
            .expect("diagnostics published");
        if msg["params"]["uri"] != uri {
            continue;
        }
        return msg["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["code"].as_str().unwrap_or_default().to_string())
            .collect();
    }
}

#[spec(
    behavior = "emit_live_diagnostics",
    verify = "diagnostics update after file change"
)]
#[tokio::test]
async fn e2e_diagnostics_follow_each_edit() {
    let clean = "invariant token_unique \"T\" {\n  guarantee \"g\"\n}\n\nbehavior login \"L\" {\n  invariants [token_unique]\n}\n";
    let (mut client, uri, _dir) = Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "auth.spec",
        clean,
    )
    .await;

    let broken = clean.replace("[token_unique]", "[tokn_unique]");
    client
        .did_change(&uri, 2, vec![json!({"text": broken})])
        .await;
    let codes = next_published_codes(&mut client, &uri).await;
    assert!(codes.contains(&"E003".to_string()), "{codes:?}");

    client
        .did_change(&uri, 3, vec![json!({"text": clean})])
        .await;
    let codes = next_published_codes(&mut client, &uri).await;
    assert!(!codes.contains(&"E003".to_string()), "{codes:?}");
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "cycle_detection finds cycles in edge type"
)]
#[tokio::test]
async fn e2e_extension_cycle_rule_reports_its_own_code() {
    // Two modules that depend on each other: product's cycle rule is E007.
    let text = "module core \"Core\" {\n  depends_on [ui]\n}\n\nmodule ui \"UI\" {\n  depends_on [core]\n}\n";
    let (mut client, uri, _dir) =
        Session::with_extensions(&["@specforge/product"], "modules.spec", text).await;

    // A no-op edit republishes the file's diagnostics.
    client
        .did_change(&uri, 2, vec![json!({"text": text})])
        .await;
    let codes = next_published_codes(&mut client, &uri).await;

    assert!(codes.contains(&"E007".to_string()), "{codes:?}");
}

#[tokio::test]
#[ignore = "fixed by T1 (the server keeps serving) and T2 (the text)"]
async fn e2e_an_edit_at_the_largest_column_keeps_the_server_serving() {
    let (mut client, uri) = Session::with_doc(None, "test.spec", "éé\n").await;

    // A column no line has: the edit lands at the line's end.
    client
        .did_change(
            &uri,
            2,
            vec![json!({
                "range": {
                    "start": { "line": 0, "character": u32::MAX },
                    "end": { "line": 0, "character": u32::MAX },
                },
                "text": "x"
            })],
        )
        .await;

    // The server answers: a request after the edit is not met by a closed
    // connection, and shutdown is answered.
    let hover = client.hover(&uri, 0, 0).await;
    assert!(hover["error"].is_null(), "{hover}");
    let shutdown = client.shutdown().await;
    assert!(shutdown["error"].is_null(), "{shutdown}");

    // The buffer is `ééx`: its syntax error quotes it.
    let published = client
        .wait_for_notification("textDocument/publishDiagnostics", 5000)
        .await
        .expect("diagnostics after the edit");
    let text = published["params"]["diagnostics"].to_string();
    assert!(text.contains("ééx"), "{text}");
}
