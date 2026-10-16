//! The candidate vocabulary a build is handed (ADR 0122) changes the suggestions of E024, W151 and
//! W020 and nothing else. Every test starts from `.spec` text (ADR 0115).

use specforge_common::Diagnostic;
use specforge_test_macros::test as spec;

use crate::support::written::checked;
use crate::support::{build, candidates, coded_in, declare, formal, product, software};

const TEXT: &str =
    "feature f1 \"F\" {\n}\n\ninvariant i1 \"I\" {\n  expression \"x\"\n}\n\nxyzzy x1 \"X\" {\n}\n";

/// `diagnostics` without their suggestions.
fn without_suggestions(diagnostics: &[Diagnostic]) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .cloned()
        .map(|mut d| {
            d.suggestion = None;
            d
        })
        .collect()
}

#[spec(
    behavior = "suggest_missing_extensions",
    verify = "the candidate vocabulary changes only suggestions, never which diagnostics a check reports"
)]
fn the_candidate_vocabulary_changes_only_suggestions() {
    let bare = checked(&build([software()]), &[("t.spec", TEXT)]);
    let handed = build([software()]).with_candidates(candidates(&[product(), formal()]));
    let with = checked(&handed, &[("t.spec", TEXT)]);

    // The same codes, messages, spans and order.
    assert_eq!(without_suggestions(&bare), without_suggestions(&with));
    assert_eq!(coded_in(&with, "E024").len(), 2, "{with:?}");
    assert_eq!(coded_in(&with, "W020").len(), 1, "{with:?}");

    // Only the suggestions of the two names the vocabulary knows differ.
    let differing: Vec<&str> = bare
        .iter()
        .zip(&with)
        .filter(|(a, b)| a.suggestion != b.suggestion)
        .map(|(a, _)| a.message.as_str())
        .collect();
    assert_eq!(differing.len(), 2, "{differing:?}");
    assert!(
        differing.iter().any(|m| m.contains("'feature'")),
        "{differing:?}"
    );
    assert!(
        differing.iter().any(|m| m.contains("'expression'")),
        "{differing:?}"
    );

    // W151, over a build that declares no kind.
    let kindless = || build([declare("@test/kindless", |_| {})]);
    let text = "behavior b1 \"B\" {\n}\n";
    let bare = checked(&kindless(), &[("t.spec", text)]);
    let with = checked(
        &kindless().with_candidates(candidates(&[software()])),
        &[("t.spec", text)],
    );
    assert_eq!(coded_in(&with, "W151").len(), 1, "{with:?}");
    assert_eq!(without_suggestions(&bare), without_suggestions(&with));
    assert_ne!(bare[0].suggestion, with[0].suggestion);
}

#[test]
fn a_build_handed_no_vocabulary_suggests_search() {
    let diags = checked(&build([software()]), &[("t.spec", TEXT)]);
    let e024 = coded_in(&diags, "E024");
    assert!(
        e024[0]
            .suggestion
            .as_deref()
            .unwrap()
            .contains("specforge search feature"),
        "{diags:?}"
    );
    let w020 = coded_in(&diags, "W020");
    assert_eq!(w020[0].suggestion, None, "{diags:?}");
}
