//! What the parser does with text a user is still typing.

use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "a syntax error quoting a long first line cuts it at a character boundary"
)]
fn a_long_error_line_is_cut_at_a_character_boundary() {
    // The 57th byte falls inside '─' (3 bytes): the cut used to panic.
    let line = "ref rust.fn:x// ── Property Entity Kind ──────────────────────";
    let file = parse(&format!("{line}\n"), "a.spec");
    let found = file
        .errors
        .iter()
        .filter_map(|e| e.found.as_deref())
        .find(|f| f.ends_with("..."))
        .unwrap_or_else(|| panic!("an error quoting the cut line: {:?}", file.errors));
    let quoted = found.trim_end_matches("...");
    assert!(quoted.len() <= 57, "{quoted:?}");
    assert!(
        line.starts_with(quoted),
        "{quoted:?} is a prefix of the line"
    );
}
