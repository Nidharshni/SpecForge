//! The builtins' candidate vocabulary is generated beside them (ADR 0122).

use specforge_component::builtins::BUILTIN_VOCABULARY;
use specforge_component::committed::pinned_builtin_declarations;
use specforge_registry::CandidateVocabulary;

#[specforge_test_macros::test(
    behavior = "suggest_missing_extensions",
    verify = "the builtin vocabulary is what the builtins' pinned declarations declare"
)]
fn the_builtin_vocabulary_is_what_the_builtins_declare() {
    let declarations = pinned_builtin_declarations().unwrap();
    let expected = CandidateVocabulary::of(&declarations).unwrap().to_json();
    assert_eq!(
        BUILTIN_VOCABULARY, expected,
        "the builtin vocabulary is stale: re-vendor with cargo run -p xtask --bin revendor"
    );
    assert!(!CandidateVocabulary::from_json(BUILTIN_VOCABULARY).is_empty());
}
