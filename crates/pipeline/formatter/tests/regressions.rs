//! Inputs that once broke the formatter, each pinned by the obligation it broke.

use specforge_formatter::{FormatConfig, format_source};
use specforge_test_macros::test as specforge_test;
use std::collections::BTreeMap;

pub(crate) fn fmt(source: &str) -> String {
    format_source(
        source,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    )
    .formatted
}

/// How often each non-blank line occurs.
pub(crate) fn line_counts(text: &str) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for line in text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
    {
        *counts.entry(line).or_insert(0) += 1;
    }
    counts
}

#[specforge_test(
    behavior = "preserve_comments",
    verify = "the format header stays where it is and documents no block"
)]
fn the_format_header_stays_where_it_is_and_documents_no_block() {
    let bare = "// specforge-format: 0.9\ntype S \"S\" {\n  user string\n}\n";
    let documented =
        "// specforge-format: 1.0\n// The session.\ntype S \"S\" {\n  user string\n}\n";
    for source in [bare, documented] {
        assert_eq!(fmt(source), source);
    }
    let first_doc_line = |s: &str| {
        let file = specforge_parser::parse(s, "a.spec");
        file.entities[0].doc_span.as_ref().map(|d| d.start_line)
    };
    assert_eq!(first_doc_line(bare), None);
    assert_eq!(first_doc_line(documented), Some(2));
}

#[specforge_test(
    behavior = "format_with_parse_errors",
    verify = "forms that share a line with a form left unchanged are kept with it, each line written once"
)]
fn a_line_shared_with_a_broken_form_is_written_once() {
    for source in [
        "}invariatype t \"T\" = a | bn \"x\"\n}\n",
        "use \"types\"\"\"\"\n\nbehavior greet \"Greet\" {\n  contract \"c\"\n}\n",
        "behavior a \"A\" {\n  contract \"c\"\n}(\n",
    ] {
        let once = format_source(
            source,
            &FormatConfig::default(),
            &specforge_graph::FieldForms::default(),
        );
        assert_eq!(
            line_counts(&once.formatted),
            line_counts(source),
            "{source:?} -> {:?}",
            once.formatted
        );
        assert!(
            !once.diagnostics.is_empty(),
            "the kept region is reported: {source:?}"
        );
        assert_eq!(
            fmt(&once.formatted),
            once.formatted,
            "a second run changes nothing: {source:?}"
        );
    }
}

#[specforge_test(
    behavior = "apply_format_rules",
    verify = "multiline string literals are kept byte-for-byte"
)]
fn a_strings_trailing_whitespace_is_its_value() {
    for source in [
        "behavior a \"A\" {\n  contract \"\"\"\n    one \n    two\n  \"\"\"\n}\n",
        "behavior a \"A\" {\n  contract \"one \n  two\"\n}\n",
        // A statement kept as written (it holds a comment).
        "behavior a \"A\" {\n  contract \"one \n  two\" // c\n}\n",
        // A list kept as written (it holds a comment).
        "behavior a \"A\" {\n  tags [\n    // c\n    \"one \n    two\"\n  ]\n}\n",
    ] {
        let value = |text: &str| {
            format!(
                "{:?}",
                specforge_parser::parse(text, "a.spec").entities[0].fields
            )
        };
        assert_eq!(value(&fmt(source)), value(source), "{source:?}");
        assert_eq!(
            fmt(source),
            source,
            "a canonical file with a trailing space in a string is a fixed point"
        );
    }
}

#[specforge_test(
    behavior = "apply_format_rules",
    verify = "an expression block's lines are indented one level inside it, whatever their indentation"
)]
fn an_expression_block_is_reindented() {
    let canonical =
        "constraint c \"C\" {\n  expression expr {\n    a < 250ms\n    b < 75MB\n  }\n}\n";
    let relaid =
        "constraint c \"C\" {\n  expression expr {\n          a < 250ms\n\tb < 75MB\n     }\n}\n";
    assert_eq!(fmt(relaid), fmt(canonical));
    assert_eq!(fmt(canonical), canonical);
}

#[specforge_test(
    behavior = "format_with_parse_errors",
    verify = "formatting any text never panics, and formatting its output again changes nothing"
)]
fn the_last_line_before_an_unclosed_strings_delimiter_is_written_once() {
    // The closing quote that runs into text leaves a `"` unclosed: the piece
    // ends with an appended delimiter on a row of its own. That row held no
    // source, and it wrote the last source row again, on every run.
    let source = "  contract   \"\"\"\n  \"\"\"\n  verify unit \"s\"pecforge.query tool returns subgraph for valid entityId\"\n  verify unit \"the default query is the document specforge://graph/{entityId} serves\"\n";
    let once = fmt(source);
    assert_eq!(line_counts(&once), line_counts(source), "{once:?}");
    assert_eq!(fmt(&once), once, "a second run changes nothing");
}

#[specforge_test(
    invariant = "formatting_semantic_preservation",
    verify = "format(spec) parses to an identical entity graph as spec"
)]
fn an_annotation_after_a_multiline_value_stays_after_it() {
    // `@optional` once moved to the first line, inside the string.
    let source = "feature f \"F\" {\n  solution \"\"\"\n    text\n  \"\"\" @optional\n}\n";
    let value = |text: &str| {
        let file = specforge_parser::parse(text, "a.spec");
        format!("{:?}", file.entities[0].fields)
    };
    let once = fmt(source);
    assert_eq!(value(&once), value(source), "{once:?}");
    assert_eq!(once, source);
}
