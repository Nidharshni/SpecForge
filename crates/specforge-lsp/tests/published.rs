//! What the editor is shown for a file has one writer: the compile's layer and the formatter's,
//! each with the buffer version it belongs to (ADR 0056).

use serde_json::Value;
use specforge_lsp::answers;
use specforge_lsp::changes::Change;
use specforge_lsp::files::Publish;
use specforge_test_macros::test as spec;
use tower_lsp::lsp_types::{Diagnostic, FormattingOptions, NumberOrString, Position, Url};

use crate::recorder::{Sent, codes as lsp_codes, publications};
use crate::served::{Served, hover_text, uri_of_path};
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

fn options() -> FormattingOptions {
    FormattingOptions {
        tab_size: 2,
        insert_spaces: true,
        ..Default::default()
    }
}

/// Format the open document `uri` and give the store what formatting found; what it answers
/// should be sent now.
fn format(served: &mut Served, uri: &Url) -> Option<Publish> {
    let formatted = answers::formatting(&served.state(), uri, &options(), None).expect("open");
    served.publish_formatting(formatted.findings)
}

#[spec(
    behavior = "lsp_format_document",
    verify = "a formatting publish carries diagnostics of one document version"
)]
fn a_formatting_publish_carries_one_version() {
    let mut served = Served::detached();
    let uri = uri_of_path("/buffer/f.spec");
    served.open_document(&uri, KEPT);
    served.sent();

    // The editor typed a line above, and the compile has not run: it is at version 1.
    served.change_to(&uri, 2, &format!("// typed\n{KEPT}"));
    let formatted = answers::formatting(&served.state(), &uri, &options(), None).unwrap();
    assert_eq!(formatted.findings.version, Some(2));
    let at: Vec<u32> = formatted
        .findings
        .sent
        .iter()
        .map(|d| d.range.start.line)
        .collect();
    assert_eq!(at, [5], "one W142, at the kept region of the typed text");
    assert_eq!(
        served.publish_formatting(formatted.findings),
        None,
        "the compile is at version 1: nothing is sent that mixes the two"
    );

    // The compile of version 2 sends both layers, positions in the same text.
    served.apply(Change::Edited(vec![uri.clone()]));
    let sent = served.sent();
    let at_two: Vec<_> = sent
        .iter()
        .filter_map(|s| match s {
            Sent::Published {
                uri: published,
                diagnostics,
                version: Some(2),
            } if *published == uri => Some(diagnostics),
            _ => None,
        })
        .collect();
    assert!(!at_two.is_empty(), "{sent:?}");
    let line = |diagnostics: &[Diagnostic], code: &str| {
        diagnostics
            .iter()
            .find(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == code))
            .map(|d| d.range.start.line)
    };
    for diagnostics in at_two {
        assert_eq!(line(diagnostics, "E001"), Some(5));
        assert_eq!(line(diagnostics, "W142"), Some(5));
    }
}

#[spec(
    behavior = "hover_diagnostic",
    verify = "hovering a region the formatter left unformatted shows its W142"
)]
fn hovering_the_kept_region_shows_its_w142() {
    let mut served = Served::detached();
    let uri = uri_of_path("/buffer/f.spec");
    served.open_document(&uri, KEPT);
    served.sent();

    assert!(format(&mut served, &uri).is_some());
    let hover = hover_text(&served.state(), &uri, Position::new(4, 1)).unwrap_or_default();
    let (e001, w142) = (hover.find("**E001**"), hover.find("**W142**"));
    assert!(e001.is_some() && w142 > e001, "{hover}");

    // Typed in since: the findings were about a text the editor no longer has.
    served.change_to(&uri, 2, &format!("// typed\n{KEPT}"));
    let hover = hover_text(&served.state(), &uri, Position::new(4, 1)).unwrap_or_default();
    assert!(!hover.contains("W142"), "{hover}");
}

#[spec(
    behavior = "lsp_format_document",
    verify = "the formatter's diagnostics stay published until the document changes"
)]
fn the_formatter_layer_survives_another_documents_publish() {
    let mut served =
        Served::new(&[("f.spec", KEPT), ("g.spec", "type gamma \"G\" {}\n")]).open(&["f.spec"]);
    let (f, g) = (served.uri("f.spec"), served.uri("g.spec"));

    assert!(format(&mut served, &f).is_some());
    served.sent();
    served.open_document(&g, "type gamma \"G\" {}\n");

    let sent = served.sent();
    let republished = publications(&sent, &f);
    assert!(
        republished
            .last()
            .is_some_and(|list| lsp_codes(list).contains(&"W142".to_string())),
        "{republished:?}"
    );
}
