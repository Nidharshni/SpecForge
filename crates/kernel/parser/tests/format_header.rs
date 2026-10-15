//! The format header's rule: where it stands, what it may declare, and how it is rewritten.

use proptest::test_runner::TestRunner;
use specforge_parser::testing::{edited_corpus, property_config};
use specforge_parser::{
    CURRENT_FORMAT_VERSION, FormatVersion, detect_format_version, with_format_version,
};
use specforge_test_macros::test as specforge_test;

const BODY: &str = "behavior foo \"Foo\" {\n  contract \"x\"\n}\n";

#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "unsupported format version produces E019 with upgrade guidance"
)]
fn the_e019_hint_names_the_versions_a_file_may_declare() {
    let (_, diagnostics) =
        detect_format_version(&format!("// specforge-format: 99.0\n{BODY}"), "a.spec");
    assert_eq!(
        diagnostics[0].suggestion.as_deref(),
        Some("Use a format version between 0.0 and 1.0.")
    );

    let (version, diagnostics) =
        detect_format_version(&format!("// specforge-format: 0.1\n{BODY}"), "a.spec");
    assert_eq!((version.major, version.minor), (0, 1));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "I007");
}

#[test]
fn the_header_is_rewritten_where_the_parser_reads_it() {
    let cases = [
        ("// specforge-format: 0.9\nB", "// specforge-format: 1.0\nB"),
        (
            "\n\n// specforge-format: 0.9\nB",
            "\n\n// specforge-format: 1.0\nB",
        ),
        (
            "// specforge-format: 0.9  \nB",
            "// specforge-format: 1.0\nB",
        ),
        ("// specforge-format: abc\nB", "// specforge-format: 1.0\nB"),
        ("B", "// specforge-format: 1.0\nB"),
        ("", "// specforge-format: 1.0\n"),
        (
            "// a note\n// specforge-format: 0.9\nB",
            "// specforge-format: 1.0\n// a note\n// specforge-format: 0.9\nB",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(
            with_format_version(input, &CURRENT_FORMAT_VERSION),
            expected,
            "{input:?}"
        );
    }
}

#[test]
fn a_crlf_file_keeps_its_line_endings() {
    assert_eq!(
        with_format_version("// specforge-format: 0.9\r\nB\r\n", &CURRENT_FORMAT_VERSION),
        "// specforge-format: 1.0\r\nB\r\n"
    );
    assert_eq!(
        with_format_version("B\r\n", &CURRENT_FORMAT_VERSION),
        "// specforge-format: 1.0\r\nB\r\n"
    );
}

/// The lines of `text` without its header line (the first non-blank line, when it is a header),
/// and whether there was one.
fn without_header(text: &str) -> (Vec<&str>, bool) {
    let mut lines: Vec<&str> = text.lines().collect();
    match lines.iter().position(|l| !l.trim().is_empty()) {
        Some(at) if lines[at].starts_with("// specforge-format: ") => {
            lines.remove(at);
            (lines, true)
        }
        _ => (lines, false),
    }
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "the header migrate writes is the header the parser reads"
)]
fn the_header_written_is_the_header_read() {
    let mut runner = TestRunner::new(property_config());
    let result = runner.run(&edited_corpus(), |case| {
        for v in [FormatVersion { major: 0, minor: 9 }, CURRENT_FORMAT_VERSION] {
            let w = with_format_version(&case.edited, &v);
            proptest::prop_assert_eq!(detect_format_version(&w, "p.spec").0, v.clone());
            let (rest, had) = without_header(&case.edited);
            let (written, wrote) = without_header(&w);
            proptest::prop_assert!(wrote);
            let kept: Vec<&str> = if had {
                written
            } else {
                // No header before: the one written is the added first line.
                w.lines().skip(1).collect()
            };
            proptest::prop_assert_eq!(kept, rest);
            proptest::prop_assert_eq!(with_format_version(&w, &v), w);
        }
        Ok(())
    });
    if let Err(failure) = result {
        panic!("{failure}");
    }
}

#[test]
#[ignore = "flipped by T9"]
fn a_header_after_a_byte_order_mark_is_read_and_rewritten() {
    let text = "\u{feff}// specforge-format: 0.9\nB";
    let (version, diagnostics) = detect_format_version(text, "a.spec");
    assert_eq!((version.major, version.minor), (0, 9));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "I007");
    assert_eq!(diagnostics[0].span.as_ref().unwrap().start_line, 1);
    assert_eq!(
        with_format_version(text, &CURRENT_FORMAT_VERSION),
        "\u{feff}// specforge-format: 1.0\nB"
    );
}
