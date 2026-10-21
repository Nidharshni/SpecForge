//! Where a block's title and its verify statements are written (ADR 0187, amended).

use specforge_common::SourceSpan;
use specforge_parser::{Entity, parse};

/// `(start line, start column, end line, end column)`.
fn columns(span: &SourceSpan) -> (usize, usize, usize, usize) {
    (span.start_line, span.start_col, span.end_line, span.end_col)
}

fn first(source: &str) -> Entity {
    let result = parse(source, "a.spec");
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    result.entities.into_iter().next().expect("one entity")
}

#[test]
fn a_verify_statements_span_runs_from_verify_to_its_text() {
    let entity = first("behavior b \"B\" {\n  verify unit \"x\"\n}\n");
    let spans: Vec<_> = entity.obligation_spans.iter().map(columns).collect();
    assert_eq!(spans, [(2, 3, 2, 18)]);
}

#[test]
fn an_entry_named_verify_in_a_block_field_is_no_statement() {
    let entity = first(
        "port p \"P\" {\n  methods {\n    login \"l\"\n    verify \"entry\"\n  }\n  verify \"a\"\n  verify unit \"b\"\n}\n",
    );
    assert_eq!(entity.obligations.len(), 2);
    let starts: Vec<_> = entity
        .obligation_spans
        .iter()
        .map(|s| s.start_line)
        .collect();
    assert_eq!(starts, [6, 7]);
}

#[test]
fn there_is_one_span_per_obligation_in_order() {
    let entity = first(
        "port p \"P\" {\n  verify \"a\"\n  methods {\n    verify \"entry\"\n  }\n  verify unit \"b\"\n  verify integration \"c\"\n}\n",
    );
    assert_eq!(entity.obligation_spans.len(), entity.obligations.len());
    assert_eq!(entity.obligations.len(), 3);
    let starts: Vec<_> = entity
        .obligation_spans
        .iter()
        .map(|s| s.start_line)
        .collect();
    assert!(starts.windows(2).all(|w| w[0] < w[1]), "{starts:?}");
}

#[test]
fn a_titles_span_is_its_string_with_the_quotes() {
    let entity = first("behavior b \"Bee\" {\n}\n");
    assert_eq!(columns(entity.title_span.as_ref().unwrap()), (1, 12, 1, 17));
}

#[test]
fn a_title_below_the_name_keeps_its_line() {
    let entity = first("behavior\n  b\n  \"Bee\" {\n}\n");
    assert_eq!(columns(entity.title_span.as_ref().unwrap()), (3, 3, 3, 8));
}

#[test]
fn an_untitled_unions_title_span_is_none() {
    assert!(
        first("type stage = \"draft\" | \"final\"\n")
            .title_span
            .is_none()
    );
    let titled = first("type stage \"Stage\" = \"draft\"\n");
    assert_eq!(columns(titled.title_span.as_ref().unwrap()), (1, 12, 1, 19));
}

#[test]
fn a_spec_blocks_title_span_is_its_name() {
    let entity = first("spec \"Payments\" {\n}\n");
    assert_eq!(entity.title_span, entity.name_span);
    assert!(entity.title_span.is_some());
}
