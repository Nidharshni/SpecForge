//! Where a block's doc comment is written: the whole-line comments directly above it, the format header never one
//! (ADR 0202).

use specforge_parser::parse;

/// `(start line, start column, end line, end column)`.
type Cols = (usize, usize, usize, usize);

/// Each entity's id and its doc span.
fn docs(source: &str) -> Vec<(String, Option<Cols>)> {
    parse(source, "a.spec")
        .entities
        .iter()
        .map(|e| {
            let doc = e
                .doc_span
                .as_ref()
                .map(|s| (s.start_line, s.start_col, s.end_line, s.end_col));
            (e.id.raw.to_string(), doc)
        })
        .collect()
}

fn last(source: &str) -> Option<Cols> {
    docs(source).pop().unwrap().1
}

#[test]
fn a_doc_comment_is_the_comment_lines_directly_above_a_block() {
    let got = docs("type A \"A\" {\n}\n\n// one\n// two\ntype B \"B\" {\n}\n");
    assert_eq!(
        got,
        [
            ("A".to_string(), None),
            ("B".to_string(), Some((4, 1, 5, 7)))
        ]
    );
}

#[test]
fn the_format_header_is_never_a_doc_comment() {
    assert_eq!(last("// specforge-format: 0.9\ntype S \"S\" {\n}\n"), None);
    assert_eq!(
        last("// specforge-format: 1.0\n// The session.\ntype S \"S\" {\n}\n"),
        Some((2, 1, 2, 16))
    );
    assert_eq!(
        last("\n\n// specforge-format: 1.0\ntype S \"S\" {\n}\n"),
        None
    );
}

#[test]
fn a_comment_spelled_like_the_header_below_it_is_a_doc_comment() {
    assert_eq!(
        last("// specforge-format: 1.0\n\n// specforge-format: 0.9\ntype S \"S\" {\n}\n"),
        Some((3, 1, 3, 25))
    );
}

#[test]
fn a_comment_after_a_blank_line_is_no_doc_comment() {
    assert_eq!(last("// note\n\ntype A \"A\" {\n}\n"), None);
}

#[test]
fn a_comment_ending_the_line_of_the_item_above_trails_it() {
    assert_eq!(
        last("type A \"A\" {\n} // end of A\ntype B \"B\" {\n}\n"),
        None
    );
    assert_eq!(
        last("type s \"S\" = a | b // note\ntype B \"B\" {\n}\n"),
        None
    );
}

#[test]
fn a_comment_above_an_import_documents_no_block() {
    assert_eq!(
        last("// about x\nuse \"x.spec\"\ntype A \"A\" {\n}\n"),
        None
    );
}

#[test]
fn every_block_kind_takes_its_doc_comment() {
    let source = "// U\ntype s \"S\" = a | b\n\n// D\ndefine d {\n}\n\n// P\nspec \"P\" {\n}\n\n// R\nref gh.issue:1 \"r\"\n";
    let got: Vec<_> = docs(source).into_iter().map(|(_, d)| d).collect();
    assert_eq!(
        got,
        [
            Some((1, 1, 1, 5)),
            Some((4, 1, 4, 5)),
            Some((8, 1, 8, 5)),
            Some((12, 1, 12, 5))
        ]
    );
}

#[test]
fn a_doc_comment_beside_a_broken_form_is_read() {
    let source = "type A \"A\" {\n  user string\n  = = =\n}\n// doc of B\ntype B \"B\" {\n  user string\n}\n";
    assert_eq!(last(source), Some((5, 1, 5, 12)));
}
