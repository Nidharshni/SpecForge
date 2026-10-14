use super::*;

#[tokio::test]
async fn e2e_completion_entity_ids() {
    let text = concat!(
        "type token \"Token\" {}\n",
        "behavior login \"Login\" {\n",
        "  types [tok]\n",
        "}\n",
    );
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Completion inside ref list at "tok" (line 2, col 11)
    let resp = client.completion(&uri, 2, 11).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected completion result");
    let items = result.as_array().unwrap();
    let ids: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    assert!(
        ids.contains(&"token"),
        "Expected 'token' in completions, got: {ids:?}"
    );
}

#[spec(
    behavior = "autocomplete_entity_ids",
    verify = "suggestions include entity titles and kinds"
)]
#[tokio::test]
async fn e2e_completion_entity_with_title() {
    let text = "type token \"Auth Token\" {}\nbehavior b \"B\" {\n  types [tok]\n}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    let resp = client.completion(&uri, 2, 11).await;
    let items = resp["result"].as_array().unwrap();
    let token_item = items.iter().find(|i| i["label"] == "token");
    assert!(token_item.is_some(), "Expected token completion item");
    let detail = token_item.unwrap()["detail"].as_str().unwrap();
    assert!(
        detail.contains("Auth Token"),
        "Expected title in detail, got: {detail}"
    );
    assert!(
        detail.starts_with("type"),
        "Expected kind in detail, got: {detail}"
    );
}

#[tokio::test]
async fn e2e_completion_keywords_at_top_level() {
    let text = "behavior foo \"Foo\" {}\n";
    let (mut client, uri, _dir) =
        Session::with_extensions(&["@specforge/software"], "test.spec", text).await;
    // Completion at column 0 (top level, line start)
    let resp = client.completion(&uri, 1, 0).await;
    let result = &resp["result"];
    assert!(!result.is_null());
    let items = result.as_array().unwrap();
    let labels: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    assert!(
        labels.contains(&"behavior"),
        "Expected 'behavior' keyword, got: {labels:?}"
    );
    assert!(
        labels.contains(&"type"),
        "Expected 'type' keyword, got: {labels:?}"
    );
}

#[tokio::test]
async fn e2e_completion_no_keywords_inside_block() {
    let text = "behavior foo \"Foo\" {\n  contract \"test\"\n}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    // Completion inside entity body (line 1, col 5) — character >= 2
    let resp = client.completion(&uri, 1, 5).await;
    let result = &resp["result"];
    if !result.is_null() {
        let items = result.as_array().unwrap();
        let keyword_items: Vec<&Value> = items
            .iter()
            .filter(|i| i["kind"] == 14) // CompletionItemKind::KEYWORD = 14
            .collect();
        assert!(
            keyword_items.is_empty(),
            "Expected no keyword completions inside block"
        );
    }
}

/// Completion items at (line, col) of `text`, with software enabled.
async fn items_at(text: &str, line: u32, col: u32) -> Vec<Value> {
    let (mut client, uri, _dir) = Session::with_extensions(
        &["@specforge/software", "@specforge/testing"],
        "test.spec",
        text,
    )
    .await;
    let resp = client.completion(&uri, line, col).await;
    resp["result"].as_array().cloned().unwrap_or_default()
}

fn labels(items: &[Value]) -> Vec<&str> {
    items.iter().filter_map(|i| i["label"].as_str()).collect()
}

#[spec(
    behavior = "complete_field_names",
    verify = "field name completion uses FieldRegistry for entity kind"
)]
#[tokio::test]
async fn e2e_completion_offers_the_kinds_fields_inside_a_block() {
    let items = items_at("behavior login \"L\" {\n  \n}\n", 1, 2).await;

    let names = labels(&items);
    assert!(names.contains(&"contract"), "{names:?}");
    assert!(names.contains(&"invariants"), "{names:?}");
    let invariants = items.iter().find(|i| i["label"] == "invariants").unwrap();
    // A reference list scaffolds its brackets.
    assert_eq!(invariants["insertText"], "invariants [$1]", "{invariants}");
    assert_eq!(invariants["insertTextFormat"], 2, "{invariants}");
}

#[spec(
    behavior = "complete_field_names",
    verify = "suggestions are filtered by entity kind"
)]
#[tokio::test]
async fn e2e_completion_fields_follow_the_enclosing_kind() {
    let items = items_at("invariant unique \"U\" {\n  \n}\n", 1, 2).await;

    let names = labels(&items);
    assert!(names.contains(&"guarantee"), "{names:?}");
    assert!(
        !names.contains(&"contract"),
        "behavior's field offered: {names:?}"
    );
}

#[spec(
    behavior = "complete_field_names",
    verify = "no field name suggestions outside entity blocks"
)]
#[tokio::test]
async fn e2e_completion_offers_no_fields_at_top_level() {
    let items = items_at("behavior login \"L\" {\n  contract \"c\"\n}\n\n", 3, 0).await;

    let names = labels(&items);
    assert!(names.contains(&"behavior"), "{names:?}");
    assert!(!names.contains(&"contract"), "{names:?}");
    assert!(!names.contains(&"guarantee"), "{names:?}");
}

#[spec(
    behavior = "complete_keywords",
    verify = "no keyword suggestions inside entity blocks"
)]
#[tokio::test]
async fn e2e_completion_offers_no_keywords_inside_a_block() {
    let items = items_at("behavior login \"L\" {\n  \n}\n", 1, 2).await;

    let keywords: Vec<&Value> = items.iter().filter(|i| i["kind"] == 14).collect();
    assert!(keywords.is_empty(), "{keywords:?}");
}

#[spec(
    behavior = "complete_keywords",
    verify = "snippet templates based on kind field definitions"
)]
#[tokio::test]
async fn e2e_keyword_snippet_scaffolds_required_fields() {
    let items = items_at("\n", 0, 0).await;

    let behavior = items
        .iter()
        .find(|i| i["label"] == "behavior")
        .unwrap_or_else(|| panic!("no behavior keyword in {items:?}"));
    assert_eq!(behavior["insertTextFormat"], 2, "{behavior}");
    let snippet = behavior["insertText"].as_str().unwrap();
    assert!(
        snippet.starts_with("behavior ${1:id} \"${2:Title}\" {\n"),
        "{snippet}"
    );
    // contract is required on a behavior.
    assert!(snippet.contains("\n  contract "), "{snippet}");
    assert_eq!(behavior["detail"], "@specforge/software", "{behavior}");
}

#[spec(
    behavior = "autocomplete_entity_ids",
    verify = "a single-reference field's value suggests the IDs of its target kind"
)]
#[tokio::test]
async fn a_single_reference_value_completes_ids() {
    // `extends` is a single reference of a type, to another type.
    let text = concat!(
        "type base \"Base\" {}\n",
        "behavior login \"Login\" {\n  contract \"x\"\n}\n",
        "type child \"Child\" {\n  extends \n}\n",
    );
    let items = items_at(text, 5, 10).await;
    let names = labels(&items);
    assert!(names.contains(&"base"), "{names:?}");
    assert!(!names.contains(&"login"), "only types: {names:?}");
    assert!(items.iter().all(|i| i["kind"] == 18), "{items:?}");
}

/// The completion items at `refs [gh.is|]` for a client declaring (or
/// not) insert-and-replace support.
async fn scheme_ref_items(insert_replace: bool) -> Vec<Value> {
    let text = concat!(
        "ref gh.issue:42 \"Support Wasm\"\n",
        "\n",
        "behavior login \"Login\" {\n",
        "  contract \"x\"\n",
        "  refs [gh.is]\n",
        "}\n",
    );
    let capabilities = json!({
        "textDocument": {"completion": {"completionItem": {"insertReplaceSupport": insert_replace}}}
    });
    let (mut client, uri, _dir, _) =
        Session::with_extensions_as(&["@specforge/software"], "test.spec", text, capabilities)
            .await;
    let resp = client.completion(&uri, 4, 13).await;
    resp["result"].as_array().cloned().unwrap_or_default()
}

#[spec(
    behavior = "autocomplete_entity_ids",
    verify = "accepting an ID replaces the word under the cursor, a scheme ref ID whole"
)]
#[tokio::test]
async fn completion_replaces_the_whole_scheme_ref_id() {
    let range = |line, start, end| json!({"start": {"line": line, "character": start}, "end": {"line": line, "character": end}});
    let items = scheme_ref_items(true).await;
    let item = items
        .iter()
        .find(|i| i["label"] == "gh.issue:42")
        .unwrap_or_else(|| panic!("no ref in {items:?}"));
    // `  refs [gh.is]`: the word starts at `g`, column 8, the cursor is at 13.
    assert_eq!(item["textEdit"]["insert"], range(4, 8, 13), "{item}");
    assert_eq!(item["textEdit"]["replace"], range(4, 8, 13), "{item}");
    assert_eq!(item["textEdit"]["newText"], "gh.issue:42", "{item}");
    assert_eq!(item["filterText"], "gh.issue:42", "{item}");

    let items = scheme_ref_items(false).await;
    let item = items.iter().find(|i| i["label"] == "gh.issue:42").unwrap();
    assert_eq!(item["textEdit"]["range"], range(4, 8, 13), "{item}");
    assert_eq!(item["textEdit"]["newText"], "gh.issue:42", "{item}");
}
