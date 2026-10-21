//! Where a block's name and a method's name are written (ADR 0187).

use specforge_common::SourceSpan;
use specforge_parser::parse;

/// `(line, start column, end column)` of a span on one line.
fn columns(span: &SourceSpan) -> (usize, usize, usize) {
    assert_eq!(span.start_line, span.end_line);
    (span.start_line, span.start_col, span.end_col)
}

fn name_of(source: &str) -> (usize, usize, usize) {
    let result = parse(source, "a.spec");
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    columns(result.entities[0].name_span.as_ref().expect("a name span"))
}

#[test]
fn a_blocks_name_span_is_where_its_name_is_written() {
    assert_eq!(
        name_of("behavior beh_a \"A\" {\n  contract \"c\"\n}\n"),
        (1, 10, 15)
    );
}

#[test]
fn a_name_written_below_its_kind_keeps_its_line() {
    let source = "behavior\n  beh_b \"B\" {\n  contract \"c\"\n}\n";
    assert_eq!(name_of(source), (2, 3, 8));
    let result = parse(source, "a.spec");
    assert_eq!(result.entities[0].span.start_line, 1);
    assert_eq!(result.entities[0].span.start_col, 1);
}

#[test]
fn a_spec_blocks_name_span_is_its_string() {
    assert_eq!(name_of("spec \"Payments\" {\n}\n"), (1, 6, 16));
}

#[test]
fn a_refs_name_span_is_its_scheme_id() {
    assert_eq!(name_of("ref gh.issue:7 \"Seven\"\n"), (1, 5, 15));
}

#[test]
fn a_unions_name_span_is_its_name() {
    assert_eq!(name_of("type Kind = A | B\n"), (1, 6, 10));
}

#[test]
fn a_methods_name_span_is_its_name_on_any_line() {
    let below = parse(
        "port P {\n  method\n    save(x: string) -> bool\n}\n",
        "p.spec",
    );
    assert!(below.errors.is_empty(), "errors: {:?}", below.errors);
    let name = below.entities[0].methods[0]
        .name_span
        .as_ref()
        .expect("span");
    assert_eq!(columns(name), (3, 5, 9));

    let same = parse(
        "port P {\n    method detect(config: Config) -> Result<Project, Error>\n}\n",
        "p.spec",
    );
    let name = same.entities[0].methods[0]
        .name_span
        .as_ref()
        .expect("span");
    assert_eq!(columns(name), (2, 12, 18));
}
