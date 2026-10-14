//! The LSP's code actions: what navigation's fixes are, applied to a
//! document over the wire (the fixes themselves are tested in
//! specforge-ops).

use specforge_test_macros::test as spec;

#[spec(
    behavior = "code_actions_for_missing_verify",
    verify = "generated verify stubs added to entity block in .spec file"
)]
#[tokio::test]
async fn missing_verify_produces_stub() {
    // `first` has no verify statement; `second` follows it in the file.
    let text = "behavior first \"First\" {\n  contract \"c\"\n}\n\n\
                behavior second \"Second\" {\n  contract \"c\"\n  verify unit \"s\"\n}\n";
    let (mut client, uri, _dir) = crate::session::Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "flows.spec",
        text,
    )
    .await;

    let resp = client.code_action(&uri, 0, 0, 8, 0).await;
    let actions = resp["result"].as_array().cloned().unwrap_or_default();
    let stub = actions
        .iter()
        .find(|a| a["title"] == "Add verify stub for first")
        .unwrap_or_else(|| panic!("no verify stub action in {resp}"));
    // The edit targets the .spec file itself, and nothing else.
    let changes = stub["edit"]["changes"].as_object().unwrap();
    assert_eq!(changes.keys().collect::<Vec<_>>(), [&uri]);
    let edits = changes[&uri].as_array().unwrap();
    assert_eq!(edits.len(), 1);

    // Applied, the stub lands inside `first`'s block, before its brace.
    let edited = insert(text, &edits[0]);
    assert_eq!(
        edited,
        "behavior first \"First\" {\n  contract \"c\"\n  verify unit \"first — TODO\"\n}\n\n\
         behavior second \"Second\" {\n  contract \"c\"\n  verify unit \"s\"\n}\n"
    );
    let parsed = specforge_parser::parse(&edited, "flows.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let first = parsed
        .entities
        .iter()
        .find(|e| e.id.raw == "first")
        .unwrap();
    assert!(first.fields.get("verify").is_some(), "{:?}", first.fields);
}

/// `text` with the zero-width insertion `edit` applied.
fn insert(text: &str, edit: &serde_json::Value) -> String {
    let (start, end) = (&edit["range"]["start"], &edit["range"]["end"]);
    assert_eq!(start, end, "an insertion, not a replacement: {edit}");
    let line = start["line"].as_u64().unwrap() as usize;
    let character = start["character"].as_u64().unwrap() as usize;
    let offset: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
    let offset = offset + character;
    format!(
        "{}{}{}",
        &text[..offset],
        edit["newText"].as_str().unwrap(),
        &text[offset..]
    )
}

/// The actions offered at a range are those whose diagnostic or entity
/// overlaps it: the reference's line offers its fixes and its entity's
/// verify stub, another entity's line only that entity's.
#[tokio::test]
async fn actions_are_those_overlapping_the_requested_range() {
    let text = "behavior login \"Login\" {\n  contract \"c\"\n  invariants [sesion_limit]\n}\n\n\
                behavior logout \"Logout\" {\n  contract \"c\"\n}\n";
    let (mut client, uri, _dir) = crate::session::Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "login.spec",
        text,
    )
    .await;
    let titles = |resp: &serde_json::Value| -> Vec<String> {
        let mut titles: Vec<String> = resp["result"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|a| a["title"].as_str().unwrap().to_string())
            .collect();
        titles.sort();
        titles
    };
    let on_reference = client.code_action(&uri, 2, 0, 2, 29).await;
    assert_eq!(
        titles(&on_reference),
        // No entity is close to sesion_limit here: no replacement.
        [
            "Add verify stub for login",
            "Create invariant stub for sesion_limit"
        ],
        "{on_reference}"
    );
    let in_logout = client.code_action(&uri, 6, 0, 6, 1).await;
    assert_eq!(
        titles(&in_logout),
        ["Add verify stub for logout"],
        "{in_logout}"
    );
    let between = client.code_action(&uri, 4, 0, 4, 0).await;
    assert!(between["result"].is_null(), "{between}");
}
