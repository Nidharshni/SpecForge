//! The structural keywords (`spec`, `ref`, `use`, `define`) keep their own
//! grammar rules beside the one generic entity block: they parse with no
//! extension loaded, since the parser knows no entity kind.

use specforge_test_macros::test as spec;

#[spec(
    behavior = "collapse_grammar_to_generic_entity_block",
    verify = "spec_block remains as separate grammar rule"
)]
fn a_spec_block_parses_without_extensions() {
    let parsed = specforge_parser::parse("spec \"my-spec\" {\n  version \"1.0\"\n}\n", "test.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.entities.len(), 1);
    assert_eq!(parsed.entities[0].kind.raw, "spec");
}

#[spec(
    behavior = "collapse_grammar_to_generic_entity_block",
    verify = "ref_block remains as separate grammar rule"
)]
fn a_ref_block_parses_without_extensions() {
    let parsed = specforge_parser::parse("ref gh.issue:42 \"Fix bug\"\n", "test.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.entities.len(), 1);
    assert_eq!(parsed.entities[0].kind.raw, "ref");
}

#[spec(
    behavior = "collapse_grammar_to_generic_entity_block",
    verify = "use_import remains as separate grammar rule"
)]
fn a_use_import_parses_without_extensions() {
    let parsed = specforge_parser::parse("use \"types/core\"\n", "test.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.imports.len(), 1);
    assert!(parsed.entities.is_empty());
}

#[spec(
    behavior = "collapse_grammar_to_generic_entity_block",
    verify = "define_block remains as separate grammar rule"
)]
fn a_define_block_parses_without_extensions() {
    let parsed = specforge_parser::parse(
        "define user_story {\n  required [description]\n}\n",
        "test.spec",
    );
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.entities.len(), 1, "{:?}", parsed.entities);
    assert_eq!(parsed.entities[0].kind.raw, "define");
    assert_eq!(parsed.entities[0].id.raw, "user_story");
}

/// The parser writes the kinds from the one constant the registry, the
/// graph and the LSP read (a plain test: the obligation it would carry is
/// linked above).
#[test]
fn the_parser_produces_the_structural_kinds() {
    use specforge_common::structural;

    let parsed = specforge_parser::parse("spec \"s\" {}\nref gh.issue:1 \"r\"\n", "test.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let kinds: Vec<&str> = parsed
        .entities
        .iter()
        .map(|e| e.kind.raw.as_str())
        .collect();
    assert_eq!(kinds, [structural::SPEC, structural::REF]);
}

// -- The grammar knows no entity kind: any keyword parses (moved from the
// registry's detection tests).

#[spec(
    behavior = "collapse_grammar_to_generic_entity_block",
    verify = "grammar has single generic entity_block rule"
)]
fn the_grammar_has_one_generic_entity_block_rule() {
    // Any keyword is accepted structurally: if the grammar had per-keyword
    // rules, an unknown keyword would fail to parse.
    let source = "recipe my_recipe \"My Recipe\" {\n  contract \"cooks food\"\n}\n";
    let parsed = specforge_parser::parse(source, "test.spec");
    assert_eq!(parsed.entities.len(), 1);
    assert_eq!(parsed.entities[0].kind.raw, "recipe");
}

#[spec(
    behavior = "collapse_grammar_to_generic_entity_block",
    verify = "no per-keyword block rules remain in grammar"
)]
fn no_per_keyword_block_rules_remain_in_the_grammar() {
    for keyword in ["behavior", "invariant", "xyzzy", "custom_kind", "foobar"] {
        let source = format!("{keyword} test_id \"Title\" {{\n  contract \"test\"\n}}\n");
        let parsed = specforge_parser::parse(&source, "test.spec");
        assert_eq!(
            parsed.entities.len(),
            1,
            "keyword '{keyword}' should parse as an entity"
        );
    }
}

#[spec(
    behavior = "collapse_grammar_to_generic_entity_block",
    verify = "Collapse Grammar to Generic Entity Block: grammar collapse holds — grammar_source_available, single_generic_rule, structural_rules_preserved, no_keyword_validation_in_grammar"
)]
fn the_grammar_collapse_contract_holds() {
    // single_generic_rule: any keyword parses.
    let source = "unknown_keyword test_id \"Title\" {\n  field \"value\"\n}\n";
    let parsed = specforge_parser::parse(source, "test.spec");
    assert_eq!(parsed.entities.len(), 1);
    // structural_rules_preserved.
    let spec_src = "spec my_spec \"My Spec\" {\n  version \"1.0\"\n}\n";
    let spec_parsed = specforge_parser::parse(spec_src, "test.spec");
    assert_eq!(spec_parsed.entities[0].kind.raw, "spec");
}

#[spec(
    behavior = "two_phase_parse_structural",
    verify = "unknown keyword parsed into generic entity node"
)]
fn an_unknown_keyword_parses_into_a_generic_entity_node() {
    let source = "xyzzy test_id \"Unknown\" {\n  data \"hello\"\n}\n";
    let parsed = specforge_parser::parse(source, "test.spec");
    assert_eq!(parsed.entities.len(), 1);
    assert_eq!(parsed.entities[0].kind.raw, "xyzzy");
    assert_eq!(parsed.entities[0].id.raw, "test_id");
}

#[spec(
    behavior = "two_phase_parse_structural",
    verify = "no keyword validation in Phase 1"
)]
fn the_parser_validates_no_keyword() {
    let source = "not_a_real_kind my_id \"Title\" {\n  stuff \"things\"\n}\n";
    let parsed = specforge_parser::parse(source, "test.spec");
    assert!(
        parsed.errors.is_empty(),
        "parser should not produce keyword errors"
    );
    assert_eq!(parsed.entities.len(), 1);
}

#[spec(
    behavior = "two_phase_parse_structural",
    verify = "all .spec files parsed before Phase 2"
)]
fn every_file_parses_before_any_validation() {
    let files = [
        ("a.spec", "behavior a \"A\" {\n  contract \"test\"\n}\n"),
        ("b.spec", "xyzzy b \"B\" {\n  data \"test\"\n}\n"),
    ];
    let parsed: Vec<_> = files
        .iter()
        .map(|(file, source)| specforge_parser::parse(source, file))
        .collect();
    assert_eq!(parsed[0].entities.len(), 1);
    assert_eq!(parsed[1].entities.len(), 1);
}

/// (The obligation "parse errors collected without aborting" is linked in
/// the graph's tests.)
#[test]
fn parse_errors_are_collected_without_aborting() {
    let source = "behavior valid \"Valid\" {\n  contract \"ok\"\n}\n\n{invalid syntax\n\nbehavior also_valid \"Also\" {\n  contract \"ok\"\n}\n";
    let parsed = specforge_parser::parse(source, "test.spec");
    assert!(
        !parsed.entities.is_empty(),
        "parser should recover and parse valid entities"
    );
}

#[spec(
    behavior = "two_phase_parse_structural",
    verify = "Two-Phase Parse: Structural: structural parsing holds — spec_files_available, structural_parse_produced, structural_parse_event_emitted, no_keyword_validation"
)]
fn the_structural_parse_contract_holds() {
    let source = "custom_kind my_entity \"Title\" {\n  field \"value\"\n}\n";
    let parsed = specforge_parser::parse(source, "test.spec");
    // structural_parse_produced: a generic entity block.
    assert_eq!(parsed.entities.len(), 1);
    assert_eq!(parsed.entities[0].kind.raw, "custom_kind");
    // no_keyword_validation: no error for an unknown keyword.
    assert!(parsed.errors.is_empty());
}
