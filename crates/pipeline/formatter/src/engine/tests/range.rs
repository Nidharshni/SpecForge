use super::*;

/// Three blocks, then a stray `}}}` on line 13 (0-based 12).
const RANGE_WITH_A_KEPT_REGION: &str = "behavior a \"A\" {\n  contract \"a\"\n}\n\nbehavior b \"B\" {\n  contract \"b\"\n}\n\nbehavior c \"C\" {\n      contract \"c\"\n}\n\n}}}\n";

const CRLF_TWO: &str = "behavior a \"A\" {\r\n  contract \"x\"\r\n}\r\n\r\nbehavior b \"B\" {\r\n      contract \"y\"\r\n}\r\n";

#[specforge_test_macros::test(
    behavior = "lsp_format_range",
    verify = "a region left unformatted is reported at its document lines"
)]
fn range_reports_a_kept_region_at_its_document_lines() {
    let result = format_range(
        RANGE_WITH_A_KEPT_REGION,
        8,
        12,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );

    let w142: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == "W142")
        .collect();
    assert_eq!(w142.len(), 1, "{:?}", result.diagnostics);
    assert!(
        w142[0].message.starts_with("Parse error at lines 13-13,"),
        "{}",
        w142[0].message
    );
    let span = w142[0].span.as_ref().expect("a spanned W142");
    assert_eq!(span.start_line, 13);
    assert_eq!(span.start_col, 1);
    assert_eq!(span.end_line, 13);
    // `}}}` is 3 bytes: the span covers the whole line, end exclusive.
    assert_eq!(span.end_col, 4);
}

#[specforge_test_macros::test(
    behavior = "lsp_format_range",
    verify = "range formatting changes no byte outside the blocks it touches"
)]
fn a_range_keeps_every_byte_outside_its_blocks() {
    let config = FormatConfig::default();
    assert_eq!(
        format_range(
            CRLF_TWO,
            4,
            6,
            &config,
            &specforge_graph::FieldForms::default()
        )
        .formatted,
        "behavior a \"A\" {\r\n  contract \"x\"\r\n}\r\n\r\nbehavior b \"B\" {\n  contract \"y\"\n}\n"
    );
    assert_eq!(
        format_range(
            CRLF_TWO,
            0,
            2,
            &config,
            &specforge_graph::FieldForms::default()
        )
        .formatted,
        "behavior a \"A\" {\n  contract \"x\"\n}\n\r\nbehavior b \"B\" {\r\n      contract \"y\"\r\n}\r\n"
    );
}

#[specforge_test_macros::test(
    behavior = "lsp_format_range",
    verify = "range formatting changes no byte outside the blocks it touches"
)]
fn a_range_on_the_unterminated_last_line_adds_no_newline() {
    let source =
        "behavior a \"A\" {\n  contract \"x\"\n}\n\nbehavior b \"B\" {\n      contract \"y\"\n}";
    assert_eq!(
        format_range(
            source,
            4,
            6,
            &FormatConfig::default(),
            &specforge_graph::FieldForms::default()
        )
        .formatted,
        "behavior a \"A\" {\n  contract \"x\"\n}\n\nbehavior b \"B\" {\n  contract \"y\"\n}"
    );
}

// --- Invariant: config_defaults_valid ---

// --- Invariant: discover_completeness (tested from engine for convenience) ---

#[specforge_test_macros::test(
    behavior = "lsp_format_range",
    verify = "range formatting matches full formatting for affected blocks"
)]
fn test_format_range_matches_full_formatting_for_affected_blocks() {
    let source = "behavior foo \"Foo\" {\n  contract \"a\"\n}\n\nbehavior bar \"Bar\" {\n      contract \"b\"\n}\n";
    let full = fmt(source);
    let range = format_range(
        source,
        4,
        6,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );

    // Extract the bar block from both
    let full_bar: String = full
        .lines()
        .skip_while(|l| !l.contains("behavior bar"))
        .collect::<Vec<_>>()
        .join("\n");
    let range_bar: String = range
        .formatted
        .lines()
        .skip_while(|l| !l.contains("behavior bar"))
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(
        full_bar, range_bar,
        "range formatting should match full formatting for affected blocks"
    );
}

#[specforge_test_macros::test(
    behavior = "lsp_format_range",
    verify = "range formatting matches full formatting for affected blocks"
)]
fn a_range_keeps_the_blank_lines_around_it() {
    let source = "behavior foo \"Foo\" {\n  contract \"a\"\n}\n\nbehavior bar \"Bar\" {\n      contract \"b\"\n}\n";
    // The range starts on the blank line between the blocks.
    let range = format_range(
        source,
        3,
        6,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );

    assert_eq!(
        range.formatted,
        "behavior foo \"Foo\" {\n  contract \"a\"\n}\n\nbehavior bar \"Bar\" {\n  contract \"b\"\n}\n"
    );
    // A range of blank lines only changes nothing.
    let blank = format_range(
        source,
        3,
        3,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );
    assert_eq!(blank.formatted, source);
}

// --- Gap coverage: format_spec_files ---

// --- Gap coverage: lsp_format_range ---

#[specforge_test_macros::test(
    behavior = "lsp_format_range",
    verify = "range is expanded to block boundaries"
)]
fn test_range_is_expanded_to_block_boundaries() {
    // Source with two blocks: request formatting in the MIDDLE of the second block
    let source = "behavior foo \"Foo\" {\n  contract \"a\"\n}\n\nbehavior bar \"Bar\" {\n      contract   \"b\"\n      types   [x]\n}\n";
    // Request only line 5 (contract line inside bar), which is inside the bar block (lines 4-7)
    let result = format_range(
        source,
        5,
        5,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );

    // The entire bar block should be formatted (expanded to block boundaries)
    assert!(
        result.formatted.contains("  contract \"b\""),
        "contract line should be formatted with correct indent: {}",
        result.formatted
    );
    assert!(
        result.formatted.contains("  types") && result.formatted.contains("[x]"),
        "types line should also be formatted (range expanded): {}",
        result.formatted
    );
    // The foo block should be untouched (not in the range)
    assert!(
        result.formatted.contains("behavior foo \"Foo\" {"),
        "foo block should be preserved"
    );
}

// --- Gap coverage: lsp_format_range ---

#[specforge_test_macros::test(
    behavior = "lsp_format_range",
    verify = "parse errors within range are left unchanged per format_with_parse_errors"
)]
fn test_range_parse_errors_left_unchanged() {
    let config = FormatConfig::default();
    let source = "behavior foo \"Foo\" {\n  contract \"ok\"\n}\n\n{{{ broken\n\nbehavior bar \"Bar\" {\n      contract   \"fix\"\n}\n";

    // Format range covering the error region (lines 4-5)
    let result = format_range(
        source,
        4,
        5,
        &config,
        &specforge_graph::FieldForms::default(),
    );
    // Error region should be preserved
    assert!(
        result.formatted.contains("{{{ broken"),
        "parse error region within range must be left unchanged"
    );
}
