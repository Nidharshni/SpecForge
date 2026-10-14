//! What the parser does with text a user is still typing.

use proptest::test_runner::TestRunner;
use specforge_parser::lex::{MAX_NESTING, nested_too_deep};
use specforge_parser::testing::{edited_corpus, property_config};
use specforge_parser::{Previous, parse, parse_incremental};
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "parsing any text yields a file and never panics"
)]
fn parsing_any_edit_of_the_corpus_yields_a_file() {
    let mut runner = TestRunner::new(property_config());
    let result = runner.run(&edited_corpus(), |case| {
        // A panic is a failure that proptest shrinks to the few bytes that matter.
        let file = parse(&case.edited, "p.spec");
        proptest::prop_assert!(
            file.errors.iter().all(|e| e.span.start_line >= 1),
            "an error without a line"
        );
        Ok(())
    });
    if let Err(failure) = result {
        panic!("{failure}");
    }
}

#[specforge_test(
    invariant = "incremental_correctness",
    verify = "a file parsed with its previous text and tree reads as the same file parsed cold"
)]
fn a_reparse_reads_as_a_cold_parse() {
    let mut runner = TestRunner::new(property_config());
    let result = runner.run(&edited_corpus(), |case| {
        let (_, tree) = parse_incremental(&case.original, "p.spec", None);
        let tree = tree.expect("a corpus file has a tree");
        let previous = Previous {
            text: &case.original,
            tree: &tree,
        };
        let (reparsed, _) = parse_incremental(&case.edited, "p.spec", Some(previous));
        let cold = parse(&case.edited, "p.spec");
        let read =
            |file: &specforge_parser::SpecFile| format!("{:?}{:?}", file.entities, file.errors);
        proptest::prop_assert!(
            read(&reparsed) == read(&cold),
            "the reparse differs from a cold parse"
        );
        Ok(())
    });
    if let Err(failure) = result {
        panic!("{failure}");
    }
}

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

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "brackets nested past 32 levels are one parse error, and no tree is read"
)]
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

#[test]
fn the_nesting_limit_counts_brackets_outside_strings_and_comments() {
    let nested = |n: usize| format!("{}x{}", "(".repeat(n), ")".repeat(n));
    assert_eq!(nested_too_deep(&nested(MAX_NESTING)), None);
    // The 33rd opener is the 33rd byte.
    assert_eq!(nested_too_deep(&nested(MAX_NESTING + 1)), Some(MAX_NESTING));
    // `(`, `[` and `{` count together.
    let mixed = "([{".repeat(11);
    assert_eq!(nested_too_deep(&mixed), Some(MAX_NESTING));
    // Brackets in strings and comments are no brackets.
    let quoted = format!("\"{}\"", "(".repeat(100));
    let triple = format!("\"\"\"{}\"\"\"", "{".repeat(100));
    let comment = format!("// {}\n", "[".repeat(100));
    for text in [quoted, triple, comment] {
        assert_eq!(nested_too_deep(&text), None, "{text:?}");
    }
    // A closer never takes the depth below zero: a stray run of them is not credit.
    let stray = format!("{}{}", ")".repeat(100), nested(MAX_NESTING));
    assert_eq!(nested_too_deep(&stray), None);
    let stray = format!("{}{}", ")".repeat(100), nested(MAX_NESTING + 1));
    assert_eq!(nested_too_deep(&stray), Some(100 + MAX_NESTING));
}

#[test]
fn a_source_nested_past_the_limit_reports_the_first_bracket_past_it() {
    let text = format!("x {}\n", "(".repeat(MAX_NESTING + 5));
    let file = parse(&text, "deep.spec");
    assert_eq!(file.errors.len(), 1, "{:?}", file.errors);
    let error = &file.errors[0];
    assert!(
        error
            .message
            .starts_with("brackets nested more than 32 deep")
    );
    // `x ` is two bytes: the 33rd bracket is column 2 + 33.
    assert_eq!(
        (error.span.start_line, error.span.start_col),
        (1, 2 + MAX_NESTING + 1)
    );
}
