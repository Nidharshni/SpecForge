//! The format version a file's header declares, read while the file is parsed
//! (`detect_format_version_mismatch`): the version, and the I007 / E019 the
//! header reports, spanned where it is declared.

use specforge_common::Severity;
use specforge_parser::{
    CURRENT_FORMAT_VERSION, FormatVersion, MIN_SUPPORTED_VERSION, detect_format_version, parse,
};
use specforge_test_macros::test as specforge_test;

const BODY: &str = "behavior foo \"Foo\" {\n  contract \"x\"\n}\n";

fn with_header(header: &str) -> String {
    format!("// specforge-format: {header}\n{BODY}")
}

#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "header comment format version detected correctly"
)]
fn the_header_comment_is_the_files_format_version() {
    let (version, diagnostics) = detect_format_version(&with_header("0.5"), "a.spec");
    assert_eq!(version, FormatVersion { major: 0, minor: 5 });
    // An older version is reported, but it is read all the same.
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");

    // The parse carries the same: the version, and what it reports.
    let file = parse(&with_header("0.5"), "a.spec");
    assert_eq!(file.format_version, version);
    assert_eq!(file.format_diagnostics, diagnostics);
}

#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "current format version produces no diagnostic"
)]
fn the_current_format_version_reports_nothing() {
    let (version, diagnostics) = detect_format_version(&with_header("1.0"), "a.spec");
    assert_eq!(version, CURRENT_FORMAT_VERSION);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let file = parse(&with_header("1.0"), "a.spec");
    assert!(file.format_diagnostics.is_empty());
    assert!(file.errors.is_empty());
}

#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "missing format version treated as the current version"
)]
fn a_file_with_no_header_is_at_the_current_version() {
    for content in [
        BODY,
        "",
        "\n\n",
        "// a plain comment\nbehavior a \"A\" {\n}\n",
    ] {
        let (version, diagnostics) = detect_format_version(content, "a.spec");
        assert_eq!(version, CURRENT_FORMAT_VERSION, "{content:?}");
        assert!(diagnostics.is_empty(), "{content:?}: {diagnostics:?}");
    }
    let file = parse(BODY, "a.spec");
    assert_eq!(file.format_version, CURRENT_FORMAT_VERSION);
    assert!(file.format_diagnostics.is_empty());
}

#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "older format version detected and reported as I007"
)]
fn an_older_version_is_i007_on_the_header_line() {
    let content = format!("\n// specforge-format: 0.9\n{BODY}");
    let file = parse(&content, "spec/old.spec");

    assert_eq!(file.format_version, FormatVersion { major: 0, minor: 9 });
    assert_eq!(file.format_diagnostics.len(), 1);
    let diagnostic = &file.format_diagnostics[0];
    assert_eq!(diagnostic.code, "I007");
    assert_eq!(diagnostic.severity, Severity::Info);
    assert!(diagnostic.message.contains("0.9"), "{diagnostic:?}");
    assert!(
        diagnostic
            .suggestion
            .as_deref()
            .unwrap()
            .contains("migrate")
    );
    // Where the file declares it: the first non-blank line.
    let span = diagnostic.span.as_ref().expect("the header line");
    assert_eq!(span.file, "spec/old.spec");
    assert_eq!((span.start_line, span.end_line), (2, 2));
    assert_eq!(span.start_col, 1);
    // A version note is not a parse error: the file parses.
    assert!(file.errors.is_empty(), "{:?}", file.errors);
    assert_eq!(file.entities.len(), 1);
}

#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "unsupported format version produces E019 with upgrade guidance"
)]
fn a_newer_version_is_e019_with_guidance() {
    let file = parse(&with_header("99.0"), "a.spec");

    assert_eq!(file.format_diagnostics.len(), 1);
    let diagnostic = &file.format_diagnostics[0];
    assert_eq!(diagnostic.code, "E019");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert!(diagnostic.message.contains("99.0"), "{diagnostic:?}");
    assert!(
        diagnostic
            .suggestion
            .as_deref()
            .unwrap()
            .contains("Use a format version between"),
        "{diagnostic:?}"
    );
    // The file still parses with best-effort compatibility.
    assert!(file.errors.is_empty());
    assert_eq!(file.entities.len(), 1);
}

#[test]
fn a_header_that_is_not_major_minor_is_e019() {
    for header in ["abc", "1.x", "1.0.0", "-1"] {
        let (version, diagnostics) = detect_format_version(&with_header(header), "a.spec");
        assert_eq!(version, MIN_SUPPORTED_VERSION, "{header}");
        assert_eq!(diagnostics.len(), 1, "{header}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, "E019", "{header}");
        assert!(
            diagnostics[0]
                .message
                .contains("invalid format version header"),
            "{header}: {:?}",
            diagnostics[0]
        );
    }
}

#[test]
fn only_the_first_non_blank_line_declares_the_version() {
    let content = format!("// a note\n// specforge-format: 99.0\n{BODY}");
    let (version, diagnostics) = detect_format_version(&content, "a.spec");
    assert_eq!(version, CURRENT_FORMAT_VERSION);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
