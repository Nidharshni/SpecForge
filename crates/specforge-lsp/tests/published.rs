//! What the editor is shown for a file has one writer: the compile's layer and the formatter's,
//! each with the buffer version it belongs to (ADR 0056).

use serde_json::Value;
use specforge_lsp::answers;
use tower_lsp::lsp_types::{FormattingOptions, NumberOrString};

use crate::served::{buffers, uri_of_path};
use crate::session::{Session, codes, uri_of};

/// A parse error formatting keeps verbatim (W142), beside the unresolved reference.
const KEPT: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n\n}}}\n";

/// The next publication for `uri` labelled `version`.
async fn published(session: &mut Session, uri: &str, version: i64) -> Vec<Value> {
    let params = session
        .notification("textDocument/publishDiagnostics", |p| {
            p["uri"] == uri && p["version"] == version
        })
        .await
        .unwrap_or_else(|| panic!("nothing published for {uri} at version {version}"));
    params["diagnostics"].as_array().unwrap().clone()
}

#[specforge_test_macros::test(
    behavior = "lsp_format_document",
    verify = "the formatter's diagnostics stay published until the document changes"
)]
#[ignore = "fixed by 05-T5"]
#[tokio::test]
async fn the_formatters_findings_stay_published_until_the_document_changes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join("specforge.json"), "{}").unwrap();
    std::fs::write(root.join("f.spec"), KEPT).unwrap();
    std::fs::write(root.join("g.spec"), "type gamma \"G\" {}\n").unwrap();
    let (mut session, _) = Session::start(Some(&root)).await;
    let (f, g) = (uri_of(&root.join("f.spec")), uri_of(&root.join("g.spec")));

    session.open(&f, KEPT).await;
    published(&mut session, &f, 1).await;
    assert!(session.format(&f).await.is_array());
    let formatted = published(&mut session, &f, 1).await;
    assert!(codes(&formatted).contains(&"W142"), "{formatted:?}");

    session.open(&g, "type gamma \"G\" {}\n").await;
    let republished = published(&mut session, &f, 1).await;
    assert!(
        codes(&republished).contains(&"W142"),
        "another document's publish erased the formatter's findings: {republished:?}"
    );

    let hover = session.hover(&f, 4, 1).await;
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|text| text.contains("**W142**")),
        "{hover}"
    );

    session
        .did_change(
            &f,
            2,
            vec![serde_json::json!({"text": format!("// typed\n{KEPT}")})],
        )
        .await;
    let typed = published(&mut session, &f, 2).await;
    let typed = codes(&typed);
    assert!(
        typed.contains(&"E001") && !typed.contains(&"W142"),
        "{typed:?}"
    );
}

#[test]
fn a_formatting_publish_mixes_two_versions_today() {
    let mut state = buffers(&[("/buffer/f.spec", KEPT)]);
    let uri = uri_of_path("/buffer/f.spec");
    state.change_document(&uri, 2, &[crate::whole(&format!("// typed\n{KEPT}"))]);
    let options = FormattingOptions {
        tab_size: 2,
        insert_spaces: true,
        ..Default::default()
    };
    let formatted = answers::formatting(&state, &uri, &options, None).unwrap();
    let (diagnostics, version) = formatted.publish.expect("W142 is reported");
    let line = |code: &str| {
        diagnostics.iter().find_map(|d| {
            matches!(&d.code, Some(NumberOrString::String(c)) if c == code)
                .then_some(d.range.start.line)
        })
    };
    assert_eq!(
        (version, line("E001"), line("W142")),
        (Some(2), Some(4), Some(5))
    );
}
