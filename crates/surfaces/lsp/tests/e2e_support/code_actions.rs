use super::*;

#[tokio::test]
async fn e2e_code_action_missing_verify() {
    let text = "behavior foo \"Foo\" {\n  contract \"test\"\n}\n";
    let (mut client, uri, _dir) = Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "test.spec",
        text,
    )
    .await;
    let resp = client.code_action(&uri, 0, 0, 2, 1).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected code actions");
    let actions = result.as_array().unwrap();
    assert!(
        !actions.is_empty(),
        "Expected at least one code action for missing verify"
    );
    // Check that at least one action has an edit containing "verify"
    let has_verify_action = actions.iter().any(|a| {
        let edit = &a["edit"];
        if edit.is_null() {
            return false;
        }
        let changes = &edit["changes"];
        if changes.is_null() {
            return false;
        }
        changes.as_object().is_some_and(|m| {
            m.values().any(|edits| {
                edits.as_array().is_some_and(|arr| {
                    arr.iter()
                        .any(|e| e["newText"].as_str().is_some_and(|t| t.contains("verify")))
                })
            })
        })
    });
    assert!(has_verify_action, "Expected a verify code action");
}

#[tokio::test]
async fn e2e_code_action_quickfix_kind() {
    let text = "behavior foo \"Foo\" {\n  contract \"test\"\n}\n";
    let (mut client, uri, _dir) = Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "test.spec",
        text,
    )
    .await;
    let resp = client.code_action(&uri, 0, 0, 2, 1).await;
    let actions = resp["result"].as_array().unwrap();
    for action in actions {
        assert_eq!(
            action["kind"], "quickfix",
            "Code action should have kind=quickfix"
        );
    }
}

#[tokio::test]
async fn e2e_code_action_verify_stub_format() {
    let text = "behavior foo \"Foo\" {\n  contract \"test\"\n}\n";
    let (mut client, uri, _dir) = Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "test.spec",
        text,
    )
    .await;
    let resp = client.code_action(&uri, 0, 0, 2, 1).await;
    let actions = resp["result"].as_array().unwrap();
    // Find a verify-related action and check its edit text format
    let verify_action = actions.iter().find(|a| {
        a["title"]
            .as_str()
            .is_some_and(|t| t.to_lowercase().contains("verify"))
    });
    assert!(verify_action.is_some(), "Expected a verify action");
    let edit_text = verify_action.unwrap()["edit"]["changes"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .as_array()
        .unwrap()[0]["newText"]
        .as_str()
        .unwrap();
    assert!(edit_text.contains("verify"), "Stub should contain 'verify'");
    assert!(
        edit_text.contains("foo"),
        "Stub should reference entity ID 'foo'"
    );
}

#[tokio::test]
async fn e2e_no_code_action_when_verify_exists() {
    let text = "behavior bar \"Bar\" {\n  contract \"test\"\n  verify unit \"bar test\"\n}\n";
    let (mut client, uri, _dir) = Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "test.spec",
        text,
    )
    .await;
    let resp = client.code_action(&uri, 0, 0, 3, 1).await;
    let result = &resp["result"];
    // Should be null or empty — entity already has verify
    if !result.is_null() {
        let actions = result.as_array().unwrap();
        let verify_actions: Vec<&Value> = actions
            .iter()
            .filter(|a| {
                a["title"]
                    .as_str()
                    .is_some_and(|t| t.to_lowercase().contains("verify"))
            })
            .collect();
        assert!(
            verify_actions.is_empty(),
            "Expected no verify actions when verify exists"
        );
    }
}

/// `login` misspells `token_unique`: the published E003 carries a
/// did-you-mean suggestion.
const MISSPELLED: &str = "invariant token_unique \"T\" {\n  guarantee \"g\"\n}\n\nbehavior login \"L\" {\n  invariants [tokn_unique]\n}\n";

#[spec(
    behavior = "emit_live_diagnostics",
    verify = "code actions act on the diagnostics last published for the document"
)]
#[tokio::test]
async fn e2e_did_you_mean_quickfix_from_the_published_diagnostic() {
    let (mut client, uri, _dir) = Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "auth.spec",
        MISSPELLED,
    )
    .await;

    let resp = client.code_action(&uri, 0, 0, 7, 0).await;

    let actions = resp["result"].as_array().cloned().unwrap_or_default();
    let fix = actions
        .iter()
        .find(|a| a["title"] == "Replace with 'token_unique'")
        .unwrap_or_else(|| panic!("no did-you-mean quickfix in {resp}"));
    let edits = fix["edit"]["changes"][&uri].as_array().unwrap();
    assert_eq!(edits.len(), 1, "{fix}");
    assert_eq!(edits[0]["newText"], "token_unique");
    // `  invariants [tokn_unique]` is line 5; the id spans columns 14..25.
    assert_eq!(
        edits[0]["range"]["start"],
        json!({"line": 5, "character": 14})
    );
    assert_eq!(
        edits[0]["range"]["end"],
        json!({"line": 5, "character": 25})
    );
}

/// `login` names `session_limit`, which exists nowhere and resembles nothing.
const DANGLING: &str =
    "behavior login \"L\" {\n  invariants [session_limit]\n  verify unit \"y\"\n}\n";

async fn stub_action(uri: &str, client: &mut Session) -> Value {
    let resp = client.code_action(uri, 0, 0, 4, 0).await;
    resp["result"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|a| a["kind"] == "refactor")
        .unwrap_or_else(|| panic!("no stub action in {resp}"))
}

#[spec(
    behavior = "code_action_create_entity_stub",
    verify = "code action offered on E003 for non-existent entity"
)]
#[tokio::test]
async fn e2e_stub_offered_for_a_dangling_reference() {
    let (mut client, uri, _dir) = Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "auth.spec",
        DANGLING,
    )
    .await;
    let action = stub_action(&uri, &mut client).await;
    assert_eq!(action["title"], "Create invariant stub for session_limit");
}

#[spec(
    behavior = "code_action_create_entity_stub",
    verify = "stub uses correct entity kind from FieldRegistry target_kind"
)]
#[tokio::test]
async fn e2e_stub_kind_comes_from_the_enclosing_field() {
    let (mut client, uri, _dir) = Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "auth.spec",
        DANGLING,
    )
    .await;
    let action = stub_action(&uri, &mut client).await;
    let edit = &action["edit"]["changes"][&uri][0];
    // `invariants` targets the invariant kind.
    assert!(
        edit["newText"]
            .as_str()
            .unwrap()
            .contains("invariant session_limit \"session_limit\" {"),
        "{edit}"
    );
}

#[spec(
    behavior = "code_action_create_entity_stub",
    verify = "stub is inserted at end of current file"
)]
#[tokio::test]
async fn e2e_stub_is_appended_to_the_file() {
    let (mut client, uri, _dir) = Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "auth.spec",
        DANGLING,
    )
    .await;
    let action = stub_action(&uri, &mut client).await;
    let edit = &action["edit"]["changes"][&uri][0];
    // The file has 4 lines; the stub goes after them.
    assert_eq!(edit["range"]["start"], json!({"line": 4, "character": 0}));
    assert_eq!(edit["range"]["end"], json!({"line": 4, "character": 0}));
    assert!(
        edit["newText"].as_str().unwrap().starts_with('\n'),
        "{edit}"
    );
}
