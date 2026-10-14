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

/// `text` parsed on a thread with an editor's stack (tokio's 2 MiB).
fn parsed_on_a_small_stack(text: String) -> specforge_parser::SpecFile {
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || parse(&text, "deep.spec"))
        .unwrap()
        .join()
        .unwrap()
}

#[test]
#[ignore = "fixed by plan 10 T3: aborts the test process (stack overflow) until then"]
fn brackets_nested_past_the_limit_are_one_parse_error() {
    let parens = format!(
        "invariant deep_x \"Deep\" {{\n  expression expr {{\n    {}x < 1{}\n  }}\n}}\n",
        "(".repeat(1000),
        ")".repeat(1000)
    );
    let blocks = format!(
        "thing deep_x \"Deep\" {{\n  meta {} v \"x\" {}\n}}\n",
        "k {".repeat(3000),
        "}".repeat(3000)
    );
    for text in [parens, blocks] {
        let file = parsed_on_a_small_stack(text);
        assert_eq!(file.errors.len(), 1, "{:?}", file.errors);
        assert!(
            file.errors[0]
                .message
                .starts_with("brackets nested more than 32 deep"),
            "{:?}",
            file.errors
        );
    }
}
