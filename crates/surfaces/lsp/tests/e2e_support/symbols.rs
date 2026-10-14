use super::*;

#[tokio::test]
async fn e2e_document_symbols_lists_all() {
    let text = concat!(
        "behavior alpha \"Alpha\" {}\n",
        "type beta \"Beta\" {}\n",
        "event gamma \"Gamma\" {}\n",
    );
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    let resp = client.document_symbol(&uri).await;
    let result = &resp["result"];
    assert!(!result.is_null(), "Expected document symbols");
    let symbols = result.as_array().unwrap();
    assert_eq!(symbols.len(), 3, "Expected 3 symbols");
    let names: Vec<&str> = symbols.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
    assert!(names.contains(&"gamma"));
}

#[tokio::test]
async fn e2e_document_symbols_include_kind() {
    let text = "behavior alpha \"Alpha\" {}\ntype beta \"Beta\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    let resp = client.document_symbol(&uri).await;
    let symbols = resp["result"].as_array().unwrap();
    // containerName should match the entity kind
    let alpha = symbols.iter().find(|s| s["name"] == "alpha").unwrap();
    assert_eq!(
        alpha["containerName"], "behavior",
        "Expected containerName='behavior'"
    );
    let beta = symbols.iter().find(|s| s["name"] == "beta").unwrap();
    assert_eq!(
        beta["containerName"], "type",
        "Expected containerName='type'"
    );
}

#[tokio::test]
async fn e2e_document_symbols_correct_location() {
    let text = "behavior alpha \"Alpha\" {}\ntype beta \"Beta\" {}\n";
    let (mut client, uri) = Session::with_doc(None, "test.spec", text).await;
    let resp = client.document_symbol(&uri).await;
    let symbols = resp["result"].as_array().unwrap();
    let alpha = symbols.iter().find(|s| s["name"] == "alpha").unwrap();
    // 0-based line
    assert_eq!(
        alpha["location"]["range"]["start"]["line"], 0,
        "alpha should be on line 0"
    );
    let beta = symbols.iter().find(|s| s["name"] == "beta").unwrap();
    assert_eq!(
        beta["location"]["range"]["start"]["line"], 1,
        "beta should be on line 1"
    );
}

#[tokio::test]
async fn e2e_workspace_symbol_by_id_prefix() {
    let text = "behavior user_login \"Login\" {}\ntype auth_token \"Token\" {}\n";
    let (mut client, _uri) = Session::with_doc(None, "test.spec", text).await;
    let resp = client.workspace_symbol("user").await;
    let result = &resp["result"];
    assert!(!result.is_null());
    let symbols = result.as_array().unwrap();
    let names: Vec<&str> = symbols.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(
        names.contains(&"user_login"),
        "Expected 'user_login' in results"
    );
    assert!(
        !names.contains(&"auth_token"),
        "'auth_token' should not match 'user' prefix"
    );
}

#[tokio::test]
async fn e2e_workspace_symbol_by_title() {
    let text = "behavior user_login \"User Login\" {}\ntype auth_token \"Token\" {}\n";
    let (mut client, _uri) = Session::with_doc(None, "test.spec", text).await;
    let resp = client.workspace_symbol("Login").await;
    let result = &resp["result"];
    assert!(!result.is_null());
    let symbols = result.as_array().unwrap();
    let names: Vec<&str> = symbols.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(
        names.contains(&"user_login"),
        "Expected title match for 'Login'"
    );
}

#[tokio::test]
async fn e2e_workspace_symbol_empty_query() {
    let text = "behavior alpha \"Alpha\" {}\ntype beta \"Beta\" {}\n";
    let (mut client, _uri) = Session::with_doc(None, "test.spec", text).await;
    let resp = client.workspace_symbol("").await;
    let result = &resp["result"];
    assert!(!result.is_null());
    let symbols = result.as_array().unwrap();
    assert!(symbols.len() >= 2, "Empty query should return all entities");
}
