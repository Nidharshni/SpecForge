//! The format header's rule: where it stands, what it may declare, and how it is rewritten.

use specforge_parser::detect_format_version;
use specforge_test_macros::test as specforge_test;

const BODY: &str = "behavior foo \"Foo\" {\n  contract \"x\"\n}\n";

#[specforge_test(
    behavior = "detect_format_version_mismatch",
    verify = "unsupported format version produces E019 with upgrade guidance"
)]
#[ignore = "flipped by T6"]
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
