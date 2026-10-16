//! The checks over `.spec` text (ADR 0115): a value reaches them as the graph build stored it.

use specforge_graph::ValueShape;

use crate::support::build;
use crate::support::written::{checked, snapshot_of, tickets};

const TEXT: &str = "ticket t1 \"T\" {\n  priority 3\n  points \"8\"\n  urgent \"true\"\n}\n";

#[test]
fn a_value_reaches_the_checks_as_the_build_stored_it() {
    let diagnostics = checked(&build([tickets()]), &[("main.spec", TEXT)]);
    let found: Vec<(&str, &str, Option<&str>)> = diagnostics
        .iter()
        .map(|d| (d.code.as_str(), d.message.as_str(), d.suggestion.as_deref()))
        .collect();
    // The bare integer on an enum field is stored as a string, so E061 quotes it; the quoted
    // integer and the quoted boolean are coerced, so admitted.
    assert_eq!(
        found,
        [(
            "E061",
            "field 'priority' of ticket 't1' is declared enum (low, medium, high), but was given \"3\"",
            Some("use one of: low, medium, high")
        )]
    );
}

#[test]
fn a_snapshot_of_text_holds_the_stored_shapes() {
    let snapshot = snapshot_of(&build([tickets()]), &[("main.spec", TEXT)]);
    let (record, _) = snapshot.get("t1").unwrap();
    let stored: Vec<(&str, ValueShape, Option<usize>)> = record
        .fields
        .iter()
        .map(|f| {
            (
                f.key.as_str(),
                f.shape,
                f.value_span.as_ref().map(|s| s.start_line),
            )
        })
        .collect();
    assert_eq!(
        stored,
        [
            ("priority", ValueShape::String, Some(2)),
            ("points", ValueShape::Integer, Some(3)),
            ("urgent", ValueShape::Boolean, Some(4)),
        ]
    );
}

#[test]
fn the_graph_builds_own_diagnostics_are_not_returned() {
    let text = "ticket t1 \"T\" {\n  blocker t9\n}\n";
    let diagnostics = checked(&build([tickets()]), &[("main.spec", text)]);
    assert!(
        diagnostics.iter().all(|d| d.code != "E003"),
        "{diagnostics:?}"
    );
}
