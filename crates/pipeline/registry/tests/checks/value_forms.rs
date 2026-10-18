//! W159: a value written in the other form than its field's declared type gives it (ADR 0147),
//! read from `.spec` text, with the span it is written at (ADR 0115).

use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_test_macros::test as spec;

use crate::support::written::{checked, tickets};
use crate::support::{build, coded_in, declare};

/// `note` (a string field) on a kind that takes members, beside `tickets()`.
fn notes() -> ExtensionDeclaration {
    declare("@test/notes", |c| {
        c.kind("Note", |k| {
            k.keyword("note").open_fields(true);
            k.field("text", |f| {
                f.field_type(FieldType::String);
            });
        });
    })
}

/// The W159 messages with their `line:col` of `text`.
fn w159(text: &str) -> Vec<(String, usize, usize)> {
    let diags = checked(&build([tickets(), notes()]), &[("main.spec", text)]);
    coded_in(&diags, "W159")
        .into_iter()
        .map(|d| {
            let at = d.span.as_ref().expect("W159 has a span");
            (d.message.clone(), at.start_line, at.start_col)
        })
        .collect()
}

#[spec(
    behavior = "check_field_value_types",
    verify = "a quoted word on a word field and a bare word on a text field are W159"
)]
fn a_value_in_the_other_form_is_w159_at_the_value() {
    let found = w159(
        "ticket t1 \"T\" {\n  priority \"low\"\n  label \"ab\"\n  summary text\n  \
         watchers [\"t1\", t1]\n  labels [one, \"two\"]\n}\n",
    );
    let messages: Vec<&str> = found.iter().map(|(m, ..)| m.as_str()).collect();
    assert_eq!(
        messages,
        [
            "field 'priority' of ticket 't1' is a word: write it bare, as priority low",
            "field 'label' of ticket 't1' is a word: write it bare, as label ab",
            "field 'summary' of ticket 't1' is text: quote it, as summary \"text\"",
            "field 'watchers' of ticket 't1' is a word: write it bare, as watchers t1",
            "field 'labels' of ticket 't1' is text: quote it, as labels \"one\"",
        ]
    );
    // At the value, quotes included: `  priority "low"` starts at column 12 (1-based).
    assert_eq!((found[0].1, found[0].2), (2, 12), "{found:?}");
}

#[spec(
    behavior = "check_field_value_types",
    verify = "a bare word on a kind that takes members is not W159"
)]
fn a_bare_word_on_a_members_kind_is_not_w159() {
    assert!(w159("note n1 \"N\" {\n  text string\n}\n").is_empty());
}

#[spec(
    behavior = "check_field_value_types",
    verify = "a value E061 refuses is not W159"
)]
fn a_value_e061_refuses_is_not_w159() {
    assert!(w159("ticket t1 \"T\" {\n  label \"a/b\"\n  priority \"urgent\"\n}\n").is_empty());
}
