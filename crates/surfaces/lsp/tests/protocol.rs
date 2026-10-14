//! What the LSP answers and sends over the protocol, with the decisions in `answers`, `changes`
//! and `reaction`: the same behaviours as the synchronous tests beside them, asked through a
//! client, and the `Editor` port's contract between its two adapters. The tests that start
//! `pin_` assert behaviour kept as it was before those modules.

use crate::recorder::{Sent, codes as sent_codes};
use crate::served::Served;
use crate::session::{NAMES_GUIDE, Session, codes, docref_project, registered_globs, uri_of};
use serde_json::{Value, json};
use specforge_lsp::editor::WorkDone;
use specforge_test_macros::test as spec;
use std::time::Duration;
use tempfile::TempDir;

const CYCLE: &str =
    "behavior alpha \"A\" {\n  types [beta]\n}\nbehavior beta \"B\" {\n  types [alpha]\n}\n";
const A_DANGLING: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";
const B_PLAIN: &str = "type other \"O\" {}\n";
const LINKED: &str = "type token \"T\" {}\nbehavior login \"L\" {\n  types [token]\n}\n";

/// A project with no extension holding `files`.
fn project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    for (name, text) in files {
        std::fs::write(dir.path().join(name), text).unwrap();
    }
    dir
}

/// Read and drop every diagnostics publication until none arrives for a
/// while: what the server sent so far is behind us.
async fn settle(client: &mut Session) {
    while client
        .wait_for_notification("textDocument/publishDiagnostics", 500)
        .await
        .is_some()
    {}
}

/// Whether diagnostics with `code` are published for `uri` within a few
/// seconds.
async fn publishes(client: &mut Session, uri: &str, code: &str) -> bool {
    client
        .notification_within(
            "textDocument/publishDiagnostics",
            Duration::from_secs(5),
            |p| {
                p["uri"] == uri
                    && p["diagnostics"]
                        .as_array()
                        .is_some_and(|d| codes(d).contains(&code))
            },
        )
        .await
        .is_some()
}

#[tokio::test]
async fn pin_hover_puts_the_diagnostic_under_the_cursor_first() {
    let (mut client, uri, _dir) = Session::with_extensions(&[], "main.spec", CYCLE).await;
    let hover = client.hover(&uri, 0, 10).await;
    let value = hover["result"]["contents"]["value"].as_str().unwrap();
    assert!(
        value.starts_with("**W061** · Reference cycle detected"),
        "{value}"
    );
    assert!(
        value.contains("\n\n---\n\n**behavior** `alpha` — A"),
        "{value}"
    );
    assert!(!value.contains("**Diagnostics**"), "{value}");
}

#[tokio::test]
async fn pin_definition_is_a_link_only_for_a_link_client() {
    let links = json!({"textDocument": {"definition": {"linkSupport": true}}});
    let (mut client, uri, _dir, _) =
        Session::with_extensions_as(&[], "main.spec", LINKED, links).await;
    let result = client.goto_definition(&uri, 2, 10).await["result"].clone();
    let links = result.as_array().expect("a link client gets an array");
    assert_eq!(links.len(), 1, "{result}");
    let range = |v: &Value| {
        (
            v["start"]["line"].as_u64().unwrap(),
            v["start"]["character"].as_u64().unwrap(),
            v["end"]["line"].as_u64().unwrap(),
            v["end"]["character"].as_u64().unwrap(),
        )
    };
    assert_eq!(range(&links[0]["targetRange"]), (0, 0, 0, 17));
    assert_eq!(range(&links[0]["targetSelectionRange"]), (0, 5, 0, 10));
    assert_eq!(range(&links[0]["originSelectionRange"]), (2, 9, 2, 14));

    let (mut client, uri, _dir) = Session::with_extensions(&[], "main.spec", LINKED).await;
    let result = client.goto_definition(&uri, 2, 10).await["result"].clone();
    assert!(result.is_object(), "{result}");
    assert_eq!(range(&result["range"]), (0, 5, 0, 10));
}

#[spec(
    behavior = "document_open_close",
    verify = "closing a project source publishes what the project reports for its file"
)]
#[tokio::test]
async fn closing_a_clean_source_keeps_its_errors_over_the_protocol() {
    let dir = project(&[("a.spec", A_DANGLING), ("b.spec", B_PLAIN)]);
    let (mut client, _) = Session::start(Some(dir.path())).await;
    let a = uri_of(&dir.path().join("a.spec"));
    client.open(&a, A_DANGLING).await;
    assert!(publishes(&mut client, &a, "E003").await);
    settle(&mut client).await;

    client.close(&a).await;
    // The closed file is published as the project reports it: its errors stay.
    let published = client
        .notification_within(
            "textDocument/publishDiagnostics",
            Duration::from_secs(5),
            |p| p["uri"] == a,
        )
        .await
        .expect("the close publishes");
    assert_eq!(
        codes(published["diagnostics"].as_array().unwrap()),
        ["E003"]
    );
    assert!(
        client
            .wait_for_notification("textDocument/publishDiagnostics", 500)
            .await
            .is_none(),
        "nothing follows"
    );
}

/// What the editor was told, in the order it was told, without the protocol's own detail.
#[derive(Debug, PartialEq)]
enum Shape {
    Watched(Vec<String>),
    Begin(String),
    Published(String, Vec<String>, Option<i64>),
    Logged(i64, String),
    End(Option<String>),
}

/// The shapes of the messages a client received. The unregistration that precedes a
/// registration and the progress token's creation are the adapter's own protocol.
fn shapes_of_wire(messages: &[Value]) -> Vec<Shape> {
    messages
        .iter()
        .filter_map(|m| {
            let params = &m["params"];
            match m["method"].as_str()? {
                "client/registerCapability" => Some(Shape::Watched(registered_globs(m))),
                "$/progress" => match params["value"]["kind"].as_str()? {
                    "begin" => Some(Shape::Begin(params["value"]["title"].as_str()?.to_string())),
                    "end" => Some(Shape::End(
                        params["value"]["message"].as_str().map(str::to_string),
                    )),
                    other => panic!("progress {other}"),
                },
                "textDocument/publishDiagnostics" => Some(Shape::Published(
                    params["uri"].as_str()?.to_string(),
                    codes(params["diagnostics"].as_array()?)
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                    params["version"].as_i64(),
                )),
                "window/logMessage" => Some(Shape::Logged(
                    params["type"].as_i64()?,
                    params["message"].as_str()?.to_string(),
                )),
                _ => None,
            }
        })
        .collect()
}

/// The shapes of what the recorder was told, one for one.
fn shapes_of_sent(sent: &[Sent]) -> Vec<Shape> {
    sent.iter()
        .map(|s| match s {
            Sent::Published {
                uri,
                diagnostics,
                version,
            } => Shape::Published(
                uri.to_string(),
                sent_codes(diagnostics),
                version.map(i64::from),
            ),
            Sent::Watched { accepted, .. } => {
                assert!(accepted, "the recorder accepts every watcher here");
                Shape::Watched(crate::recorder::watched(std::slice::from_ref(s)).remove(0))
            }
            Sent::Logged(level, message) => Shape::Logged(
                serde_json::to_value(level).unwrap().as_i64().unwrap(),
                message.clone(),
            ),
            Sent::Progress(WorkDone::Begin { title }) => Shape::Begin(title.clone()),
            Sent::Progress(WorkDone::End { message }) => Shape::End(message.clone()),
            Sent::TokensRefreshed => panic!("no client here declared refresh support"),
        })
        .collect()
}

/// The open sequence of the docref project told to the editor through both adapters of the
/// `Editor` port: the client adapter over JSON-RPC, and the recorder. Both say the same.
#[spec(port = "Editor", verify = "Editor contract is satisfied")]
#[tokio::test]
async fn the_editor_adapters_send_the_same_open_sequence() {
    let dir = docref_project(NAMES_GUIDE);
    let root = dir.path().to_str().unwrap().to_string();
    let (mut client, _) = Session::launch(Some(&root), json!({})).await;
    let end = |m: &Value| m["method"] == "$/progress" && m["params"]["value"]["kind"] == "end";
    let wire = client
        .messages_until(Duration::from_secs(20), end)
        .await
        .expect("workspace indexing never ended");
    drop(client);

    // The reaction takes the state's blocking locks: it runs off the async runtime.
    let recorded =
        tokio::task::spawn_blocking(move || Served::at(dir).open(&[]).opening().to_vec())
            .await
            .unwrap();
    let (wire, recorded) = (shapes_of_wire(&wire), shapes_of_sent(&recorded));
    assert!(
        wire.iter().any(|s| matches!(s, Shape::Published(..))),
        "the project has a diagnostic to publish: {wire:?}"
    );
    assert_eq!(wire, recorded);
}
