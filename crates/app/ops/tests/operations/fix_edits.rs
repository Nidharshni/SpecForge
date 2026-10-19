//! A stub fix is a typed edit (ADR 0161): laid out with the file's format configuration, placed after the last block
//! of its kind, and offered only when its plan adds no error (E080). Each pin names the defect it holds and the ticket
//! that flips it.

use crate::fixes::{apply, fixes_of};
use crate::navigate::{Compiled, compile};
use specforge_formatter::FormatConfig;
use specforge_ops::authoring::fix::{Fix, FixQuery};
use std::collections::BTreeMap;

const TESTING: &[&str] = &["@specforge/software", "@specforge/testing"];
const FOUR: &str = "indent_width = 4\n";

const VERIFY_4: &str = "behavior first \"First\" {\n    contract \"c\"\n}\n";
const EVENTS_4: &str = "event user_created \"User Created\" {\n}\n\n\
                        behavior place_order \"Place Order\" {\n    contract \"c\"\n    produces [order_placed]\n    verify unit \"y\"\n}\n";
const SHORT_ID: &str = "behavior place_order \"Place Order\" {\n  contract \"c\"\n  produces [e]\n  verify unit \"y\"\n}\n";
const DANGLING: &str =
    "behavior login \"L\" {\n  invariants [session_limit]\n  verify unit \"y\"\n}\n";
const PORTS: &str = "behavior login \"Login\" {\n  contract \"c\"\n  ports    [missing_port]\n  verify unit \"y\"\n}\n";

fn four(file: &str, text: &str) -> Compiled {
    compile(TESTING, &[(file, text), (".specforgefmt.toml", FOUR)])
}

/// The text of `file` once `fix` is applied to it.
fn fixed(p: &Compiled, file: &str, fix: &Fix) -> String {
    apply(&p.project.source_text(file).unwrap(), &fix.edits)
}

/// The fix titled `title`, if offered.
fn offered<'f>(fixes: &'f [Fix], title: &str) -> Option<&'f Fix> {
    fixes.iter().find(|f| f.title == title)
}

/// Whether `text` formats to itself with `config`.
fn canonical_with(p: &Compiled, config: &FormatConfig, text: &str) -> bool {
    let forms = p.project.environment().registries().field_forms();
    specforge_formatter::format_source(text, config, forms).formatted == text
}

fn indent_four() -> FormatConfig {
    FormatConfig {
        indent_width: 4,
        ..Default::default()
    }
}

/// The errors (code, message) `file` = `text` adds to `p`'s project, compared as E080 compares (without positions).
fn added_errors(p: &Compiled, file: &str, text: &str) -> Vec<(String, String)> {
    let mut had: Vec<_> = p
        .project
        .diagnostics()
        .into_iter()
        .map(|d| (d.code, d.message))
        .collect();
    let texts = BTreeMap::from([(file.to_string(), text.to_string())]);
    let mut added = Vec::new();
    for d in p.project.edited(&texts).diagnostics() {
        if !matches!(d.severity, specforge_common::Severity::Error) {
            continue;
        }
        let key = (d.code, d.message);
        match had.iter().position(|k| *k == key) {
            Some(i) => {
                had.remove(i);
            }
            None => added.push(key),
        }
    }
    added
}

#[test]
#[ignore = "flipped by T5"]
fn a_verify_stub_follows_the_files_indent() {
    let p = four("flows.spec", VERIFY_4);
    let fixes = fixes_of(&p, &FixQuery::default());
    let fix = offered(&fixes, "Add verify stub for first").expect("offered");
    let text = fixed(&p, "flows.spec", fix);
    assert!(
        text.contains("\n    verify unit \"first — TODO\"\n}"),
        "{text}"
    );
    assert!(canonical_with(&p, &indent_four(), &text), "{text}");
}

#[test]
#[ignore = "flipped by T6"]
fn an_entity_stub_follows_the_files_indent() {
    let p = four("a.spec", EVENTS_4);
    let fixes = fixes_of(&p, &FixQuery::default());
    let fix = offered(&fixes, "Create event stub for order_placed").expect("offered");
    let text = fixed(&p, "a.spec", fix);
    assert!(canonical_with(&p, &indent_four(), &text), "{text}");
}

#[test]
#[ignore = "flipped by T6"]
fn an_entity_stub_goes_after_the_last_block_of_its_kind() {
    let p = four("a.spec", EVENTS_4);
    let fixes = fixes_of(&p, &FixQuery::default());
    let fix = offered(&fixes, "Create event stub for order_placed").expect("offered");
    let text = fixed(&p, "a.spec", fix);
    assert!(
        text.starts_with(
            "event user_created \"User Created\" {\n}\n\nevent order_placed \"order_placed\" {\n}\n\nbehavior place_order"
        ),
        "{text}"
    );
}

#[test]
#[ignore = "flipped by T6"]
fn no_entity_stub_for_an_id_create_refuses() {
    let p = compile(TESTING, &[("a.spec", SHORT_ID)]);
    let fixes = fixes_of(&p, &FixQuery::default());
    assert!(offered(&fixes, "Create event stub for e").is_none());
}

#[test]
#[ignore = "flipped by T6"]
fn an_entity_stub_writes_todo_in_each_required_field() {
    let p = compile(TESTING, &[("a.spec", DANGLING)]);
    let fixes = fixes_of(&p, &FixQuery::default());
    let fix = offered(&fixes, "Create invariant stub for session_limit").expect("offered");
    let text = fixed(&p, "a.spec", fix);
    assert!(
        text.contains("invariant session_limit \"session_limit\" {\n  guarantee \"TODO\"\n}\n"),
        "{text}"
    );
    assert_eq!(added_errors(&p, "a.spec", &text), []);
}

#[test]
#[ignore = "flipped by T6"]
fn no_entity_stub_whose_placeholders_cannot_hold() {
    let p = compile(TESTING, &[("a.spec", PORTS)]);
    let fixes = fixes_of(&p, &FixQuery::default());
    assert!(offered(&fixes, "Create port stub for missing_port").is_none());
}

#[test]
#[ignore = "flipped by T6"]
fn every_offered_fix_adds_no_error() {
    for (file, text) in [
        ("flows.spec", VERIFY_4),
        ("a.spec", EVENTS_4),
        ("a.spec", DANGLING),
        ("a.spec", PORTS),
    ] {
        let p = compile(TESTING, &[(file, text)]);
        for fix in fixes_of(&p, &FixQuery::default()) {
            let after = fixed(&p, file, &fix);
            assert_eq!(added_errors(&p, file, &after), [], "{}", fix.title);
        }
    }
}
