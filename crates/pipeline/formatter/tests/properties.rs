//! What the formatter does with text a user is still typing, and with the
//! repository's own files laid out differently: properties over the corpus
//! (`specforge_parser::testing`, ADR 0090).

use super::regressions::fmt;
use proptest::test_runner::TestRunner;
use proptest::{prop_assert, prop_assume};
use specforge_formatter::{FormatConfig, format_range};
use specforge_parser::testing::{edited_corpus, property_config, relaid_corpus};
use specforge_test_macros::test as specforge_test;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// How often each line with a letter or digit in it occurs, indentation and
/// every gap inside it aside (formatting re-aligns both). A line of
/// brackets alone is left out: formatting writes one for every block it closes.
/// A line formatting splits in two (`name str  version string`) is the whole
/// text's, so the duplication guard counts each line in the source's squeezed text.
fn line_counts(text: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for line in text.lines() {
        let line: String = line.split_whitespace().collect();
        if line.chars().any(char::is_alphanumeric) {
            *counts.entry(line).or_insert(0) += 1;
        }
    }
    counts
}

/// Where two texts first differ, quoted: a failure message that fits a screen.
fn first_difference(a: &str, b: &str) -> String {
    let at = a
        .char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map_or(a.len().min(b.len()), |((i, _), _)| i);
    let window = |t: &str| {
        let from = t.floor_char_boundary(at.saturating_sub(120));
        let to = t.floor_char_boundary((at + 120).min(t.len()));
        t[from..to].to_string()
    };
    format!(
        "first difference at byte {at}:\n  {:?}\n  {:?}",
        window(a),
        window(b)
    )
}

/// `entities`' Debug text without source positions, and without
/// the whitespace of a method's types: what formatting may change.
fn layout_free(entities: &[specforge_parser::Entity]) -> String {
    static SPANS: OnceLock<regex::Regex> = OnceLock::new();
    let spans = SPANS.get_or_init(|| {
        regex::Regex::new(r#"(SourceSpan|ExprSpan) \{[^}]*\}|span: \d+\.\.\d+"#).unwrap()
    });
    static TYPES: OnceLock<regex::Regex> = OnceLock::new();
    let types = TYPES
        .get_or_init(|| regex::Regex::new(r#"(ty: |returns: Some\()"((?:[^"\\]|\\.)*)""#).unwrap());
    let text = spans.replace_all(&format!("{entities:?}"), "").into_owned();
    types
        .replace_all(&text, |c: &regex::Captures| {
            let squeezed = c[2].replace("\\t", "").replace(' ', "");
            format!("{}\"{squeezed}\"", &c[1])
        })
        .into_owned()
}

fn fail<V: std::fmt::Debug>(result: Result<(), proptest::test_runner::TestError<V>>) {
    if let Err(failure) = result {
        panic!("{failure}");
    }
}

#[specforge_test(
    behavior = "format_with_parse_errors",
    verify = "formatting any text never panics, and formatting its output again changes nothing"
)]
fn formatting_any_edit_of_the_corpus_reaches_a_fixed_point() {
    let mut runner = TestRunner::new(property_config());
    let lines = (0usize..400, 0usize..8);
    let result = runner.run(&(edited_corpus(), lines), |(case, (start, span))| {
        let once = fmt(&case.edited);
        let twice = fmt(&once);
        prop_assert!(
            twice == once,
            "not a fixed point: {}",
            first_difference(&once, &twice)
        );
        let source = case.edited.split_whitespace().collect::<String>();
        for (line, n) in line_counts(&once) {
            prop_assert!(
                n <= source.matches(&line).count().max(1),
                "{line:?} written {n} times"
            );
        }
        let _ = format_range(
            &case.edited,
            start,
            start + span,
            &FormatConfig::default(),
            &specforge_graph::FieldForms::default(),
        );
        Ok(())
    });
    fail(result);
}

#[specforge_test(
    invariant = "formatting_semantic_preservation",
    verify = "format(spec) parses to an identical entity graph as spec"
)]
#[specforge_test(
    invariant = "formatting_idempotency",
    verify = "random valid .spec files satisfy format(format(x)) == format(x)"
)]
fn formatting_a_valid_edit_keeps_its_entities() {
    let mut config = property_config();
    // About one edit in ten still parses without an error.
    config.cases *= 4;
    config.max_global_rejects = config.cases * 20;
    let mut runner = TestRunner::new(config);
    let result = runner.run(&edited_corpus(), |case| {
        let parsed = specforge_parser::parse(&case.edited, "p.spec");
        prop_assume!(parsed.errors.is_empty());
        let once = fmt(&case.edited);
        let reparsed = specforge_parser::parse(&once, "p.spec");
        let (after, before) = (
            layout_free(&reparsed.entities),
            layout_free(&parsed.entities),
        );
        prop_assert!(
            after == before,
            "formatting changed the entities: {}",
            first_difference(&before, &after)
        );
        let twice = fmt(&once);
        prop_assert!(
            twice == once,
            "not a fixed point: {}",
            first_difference(&once, &twice)
        );
        Ok(())
    });
    fail(result);
}

#[specforge_test(
    invariant = "formatting_consistency",
    verify = "two files differing only in whitespace produce identical formatted output"
)]
#[specforge_test(
    invariant = "formatting_consistency",
    verify = "tab-indented and space-indented inputs produce the same output"
)]
fn a_relaid_corpus_file_formats_as_the_file_does() {
    let mut runner = TestRunner::new(property_config());
    let result = runner.run(&relaid_corpus(), |case| {
        let (relaid, original) = (fmt(&case.edited), fmt(&case.original));
        prop_assert!(
            relaid == original,
            "layout changed the output: {}",
            first_difference(&original, &relaid)
        );
        Ok(())
    });
    fail(result);
}
