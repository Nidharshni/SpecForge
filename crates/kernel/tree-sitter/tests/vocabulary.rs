//! The CST vocabulary (`kind`, `field`) is the compiled grammar's.

use std::collections::BTreeSet;
use tree_sitter_specforge::{LANGUAGE, field, kind};

#[specforge_test_macros::test(
    invariant = "cst_vocabulary_grammar_consistency",
    verify = "every named node kind and field name of the grammar has one constant, and every constant names one"
)]
fn the_vocabulary_is_the_grammars() {
    let language: tree_sitter::Language = LANGUAGE.into();
    let grammar_kinds: BTreeSet<&str> = (0..language.node_kind_count() as u16)
        .filter(|&id| language.node_kind_is_named(id) && language.node_kind_is_visible(id))
        .filter_map(|id| language.node_kind_for_id(id))
        .filter(|kind| *kind != "ERROR")
        .collect();
    let kinds: BTreeSet<&str> = kind::ALL.iter().copied().collect();
    assert_eq!(kinds.len(), kind::ALL.len(), "a node kind is listed twice");
    assert_eq!(
        kinds, grammar_kinds,
        "kind::ALL against the grammar's named node kinds"
    );
    let grammar_fields: BTreeSet<&str> = (1..=language.field_count() as u16)
        .filter_map(|id| language.field_name_for_id(id))
        .collect();
    let fields: BTreeSet<&str> = field::ALL.iter().copied().collect();
    assert_eq!(
        fields.len(),
        field::ALL.len(),
        "a field name is listed twice"
    );
    assert_eq!(
        fields, grammar_fields,
        "field::ALL against the grammar's field names"
    );
}
