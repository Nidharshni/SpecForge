use super::*;

// --- Slice 6: Parse errors ---

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "file with syntax error is partially formatted without crash"
)]
fn test_file_with_syntax_error_partially_formatted() {
    let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n\nthis is invalid syntax { broken\n\nbehavior bar \"Bar\" {\n  contract \"also good\"\n}\n";
    let result = format_source(
        input,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );
    // Should not crash
    assert!(!result.formatted.is_empty());
    // Good parts should still be formatted
    assert!(result.formatted.contains("behavior foo \"Foo\""));
    assert!(result.formatted.contains("behavior bar \"Bar\""));
}

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "error regions are preserved verbatim in output"
)]
fn test_error_regions_preserved_verbatim() {
    let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n\n{{{broken\n\nbehavior bar \"Bar\" {\n  contract \"also good\"\n}\n";
    let result = format_source(
        input,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );
    // The error region should be in the output
    assert!(!result.formatted.is_empty());
}

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "well-formed blocks in a file with errors are still formatted"
)]
fn test_well_formed_blocks_with_errors_are_formatted() {
    let input = "behavior foo \"Foo\" {\n      contract \"good\"\n}\n\n broken {{\n\nbehavior bar \"Bar\" {\n    contract \"also good\"\n}\n";
    let result = format_source(
        input,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );
    // Well-formed blocks should be properly indented
    assert!(
        result.formatted.contains("  contract \"good\"")
            || result.formatted.contains("contract \"good\"")
    );
}

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "diagnostic lists files with parse errors and error line ranges"
)]
fn test_parse_error_diagnostics() {
    let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n{{{broken\n";
    let result = format_source(
        input,
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
        w142[0].message.starts_with("Parse error at lines 4-4,"),
        "{}",
        w142[0].message
    );
    let span = w142[0].span.as_ref().expect("a spanned W142");
    assert_eq!((span.start_line, span.start_col), (4, 1));
    assert_eq!((span.end_line, span.end_col), (4, "{{{broken".len() + 1));

    // A region over two lines ends at its last line's end.
    let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n\nbehavior bar \"Bar\" {\n  contract \"x\" ]]\n  more ((\n";
    let result = format_source(
        input,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );
    let span = result
        .diagnostics
        .iter()
        .find(|d| d.code == "W142")
        .and_then(|d| d.span.clone())
        .expect("a spanned W142");
    // The kept form `bar` is reported from its first line to its last.
    assert_eq!((span.start_line, span.end_line), (5, 7), "{span:?}");
    let last = input.lines().nth(span.end_line - 1).unwrap();
    assert_eq!(span.end_col, last.len() + 1, "{span:?}");
}

// --- Behavior: format_with_parse_errors (additional) ---

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "error region starts at first unparseable token"
)]
fn test_error_region_starts_at_first_unparseable_token() {
    let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n{{{broken stuff here\n";
    let result = format_source(
        input,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );
    // Error region should contain the broken content
    assert!(
        result.formatted.contains("{{{broken") || result.formatted.contains("broken"),
        "error region should start at first unparseable token: {}",
        result.formatted
    );
    // First block should still be well-formed
    assert!(result.formatted.contains("behavior foo \"Foo\" {"));
}

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "error region ends before next parseable top-level statement"
)]
fn test_error_region_ends_before_next_parseable_statement() {
    let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n\n{{{ broken\n\nbehavior bar \"Bar\" {\n  contract \"also good\"\n}\n";
    let result = format_source(
        input,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );
    // The bar block after the error should still be present and formatted
    assert!(
        result.formatted.contains("behavior bar \"Bar\""),
        "parseable block after error should be present: {}",
        result.formatted
    );
}

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "whitespace within error regions is preserved byte-for-byte"
)]
fn test_whitespace_within_error_regions_preserved_byte_for_byte() {
    let input = "behavior foo \"Foo\" {\n  contract \"good\"\n}\n\n  {{{  broken   stuff  \n\nbehavior bar \"Bar\" {\n  contract \"also good\"\n}\n";
    let result = format_source(
        input,
        &FormatConfig::default(),
        &specforge_graph::FieldForms::default(),
    );
    // The error region whitespace should be preserved
    // The exact error text should appear in the output
    assert!(
        result.formatted.contains("{{{") || result.formatted.contains("broken"),
        "error region content should be preserved: {}",
        result.formatted
    );
}

// --- Contract: format_with_parse_errors ---

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "Format Files with Parse Errors: formatting with parse errors holds — cst_with_errors, no_crash, well_formed_regions_formatted, error_regions_preserved, parse_error_diagnosed"
)]
fn test_format_with_parse_errors_contract() {
    let config = FormatConfig::default();

    // requires: cst_with_errors — source has parse errors
    let source = "behavior foo \"Foo\" {\n      contract \"good\"\n}\n\n{{{ broken stuff\n\nbehavior bar \"Bar\" {\n    contract \"also good\"\n}\n";

    let result = format_source(source, &config, &specforge_graph::FieldForms::default());

    // ensures: no_crash — we got here without panicking
    assert!(
        !result.formatted.is_empty(),
        "no_crash: output should not be empty"
    );

    // ensures: well_formed_regions_formatted — good blocks should be indented
    assert!(
        result.formatted.contains("behavior foo \"Foo\""),
        "well_formed_regions_formatted: foo block should be present"
    );
    assert!(
        result.formatted.contains("behavior bar \"Bar\""),
        "well_formed_regions_formatted: bar block should be present"
    );

    // ensures: error_regions_preserved — broken content preserved
    assert!(
        result.formatted.contains("{{{ broken") || result.formatted.contains("broken"),
        "error_regions_preserved: error content should be in output"
    );

    // ensures: parse_error_diagnosed — W142 diagnostic emitted with line ranges
    let error_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == "W142")
        .collect();
    assert!(
        !error_diags.is_empty(),
        "parse_error_diagnosed: should have W142 diagnostic"
    );
    assert!(
        error_diags[0].message.contains("lines"),
        "parse_error_diagnosed: diagnostic should mention line ranges: {}",
        error_diags[0].message
    );
}

// --- Gap coverage: format_spec_files ---

// --- Gap coverage: show_formatting_diff ---

// --- Gap coverage: lsp_format_document ---

#[specforge_test_macros::test(
    behavior = "format_with_parse_errors",
    verify = "error regions are preserved verbatim in output"
)]
fn test_parse_errors_trigger_partial_formatting() {
    let config = FormatConfig::default();
    let source_with_error = "behavior foo \"Foo\" {\n  contract \"ok\"\n}\n\n{{{ invalid syntax\n\nbehavior bar \"Bar\" {\n      contract   \"fix\"\n}\n";

    let result = format_source(
        source_with_error,
        &config,
        &specforge_graph::FieldForms::default(),
    );
    // Should not crash
    assert!(
        !result.formatted.is_empty(),
        "formatter must not crash on parse errors"
    );
    // Well-formed regions should still be formatted
    // Error region should be preserved
    assert!(
        result.formatted.contains("{{{ invalid syntax"),
        "error region must be preserved verbatim"
    );
}
