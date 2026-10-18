//! A value is written in its field's one form (ADR 0147). Today `check` and
//! `format` take either form; the tests still ignored are flipped by the formatter ticket.

use crate::support::{Cli, TestProject};

fn project(file: &str, source: &str) -> TestProject {
    TestProject::new()
        .enabling(&["@specforge/software"])
        .config(|c| c["spec_root"] = "spec".into())
        .file(file, source)
}

const CART: &str = "behavior add_item \"Add\" {\n  category \"cart\"\n  contract \"x\"\n}\n";

#[specforge_test_macros::test(
    behavior = "check_field_value_types",
    verify = "a quoted word on a word field and a bare word on a text field are W159"
)]
fn check_reports_a_tag_written_as_text() {
    let dir = project("spec/cart.spec", CART);

    let diagnostics = Cli::on(&dir).document(&["check"]).json;

    let w159: Vec<_> = diagnostics
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "W159")
        .collect();
    assert_eq!(w159.len(), 1, "{diagnostics}");
    assert_eq!(w159[0]["span"]["start_line"], 2, "{diagnostics}");
    let message = w159[0]["message"].as_str().unwrap();
    assert!(
        message.contains("category") && message.contains("cart"),
        "{message}"
    );
}

#[test]
#[ignore = "flipped by T6"]
fn format_writes_a_tag_bare() {
    let dir = project("spec/cart.spec", CART);
    let cli = Cli::on(&dir);
    let before = cli.run(&["export", "--format", "json"]).exits(0).stdout;

    cli.run(&["format", "--check"]).exits(1);
    cli.run(&["format"]).exits(0);

    let written = std::fs::read_to_string(dir.root().join("spec/cart.spec")).unwrap();
    assert!(written.contains("\n  category cart\n"), "{written}");
    let after = cli.run(&["export", "--format", "json"]).exits(0).stdout;
    assert_eq!(before, after, "formatting the quotes keeps the graph");
}

#[test]
#[ignore = "flipped by T6"]
fn format_quotes_text_written_bare() {
    let dir = project(
        "spec/events.spec",
        "event e1 \"E\" {\n  channel compilation\n}\n",
    );

    Cli::on(&dir).run(&["format"]).exits(0);

    let written = std::fs::read_to_string(dir.root().join("spec/events.spec")).unwrap();
    assert!(
        written.contains("\n  channel \"compilation\"\n"),
        "{written}"
    );
}
