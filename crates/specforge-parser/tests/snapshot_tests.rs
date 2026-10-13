use specforge_parser::parse;

fn parse_ok(source: &str, file: &str) -> specforge_parser::SpecFile {
    let result = parse(source, file);
    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    result
}

#[test]
fn snap_basic_entity_block() {
    let source = r#"
behavior parse_spec_file "Parse Spec File" {
    status planned
    tags [alpha, beta, gamma]
    verify unit "produces AST for minimal valid file"
    verify integration "parses all .spec files in project"
}
"#;
    insta::assert_debug_snapshot!(parse_ok(source, "test.spec").entities);
}

#[test]
fn snap_all_field_value_types() {
    let source = r#"
behavior test_values "Test Values" {
    name "a string"
    count 42
    enabled true
    disabled false
    created 2024-01-15
    status planned
    tags [alpha, beta, gamma]
    labels ["tag-one", "tag-two"]
    config {
        timeout 30
        retries 3
    }
    verify unit "it works"
    verify property "always valid"
}
"#;
    insta::assert_debug_snapshot!(parse_ok(source, "test.spec").entities);
}

#[test]
fn snap_multi_error_recovery() {
    let source = r#"
behavior good_before "Before" {
    status planned
}

behavior ??? {
    this is broken
}

behavior good_after "After" {
    status done
}
"#;
    let result = parse(source, "test.spec");
    insta::assert_debug_snapshot!("multi_error_entities", result.entities);
    insta::assert_debug_snapshot!("multi_error_diagnostics", result.errors);
}

#[test]
fn snap_real_world_spec() {
    let source = r#"
use "invariants/core"
use "types/core"

behavior parse_spec_file_to_ast "Parse Spec File to AST" {
    invariants [multi_error_collection, string_interning_consistency]
    types      [SpecFile, ParseError, SourceSpan]

    requires {
        valid_utf8_input "Input buffer is valid UTF-8"
    }

    ensures {
        ast_produced "A complete AST is produced"
    }

    contract """
        Given a source file
        When parsed by the Tree-sitter grammar
        Then an AST is produced with all entities
    """

    verify unit "produces AST for minimal valid file"
    verify integration "parses all .spec files in project"
}

ref gh.issue:1 "Initial parser"

behavior recover_from_syntax_errors "Recover from Syntax Errors" {
    invariants [multi_error_collection]
    types      [ParseError]
    status     planned
    verify property "N errors produce N diagnostics"
}
"#;
    let result = parse_ok(source, "behaviors/parsing.spec");
    insta::assert_debug_snapshot!("real_world_imports", result.imports);
    insta::assert_debug_snapshot!("real_world_entities", result.entities);
}

#[test]
fn snap_union_types() {
    let source = r#"
type FieldValue = StringValue | ReferenceList | Block

type McpErrorCode = "invalid_input" | "compilation_failed" | "entity_not_found"

type JsonRpcErrorCode = -32700 | -32600 | -32601
"#;
    insta::assert_debug_snapshot!(parse_ok(source, "test.spec").entities);
}

#[test]
fn snap_structural_constructs() {
    let source = r#"
spec "SpecForge" {
    version "0.1.0"
    verify integration "project compiles"
}

ref gh.issue:42 "Support Wasm extensions"

ref gh.issue:99 "Performance tracking" {
    priority "high"
}

define my_custom_kind {
    base_kind "entity"
    testable true
    verify unit "custom kind registers"
}
"#;
    insta::assert_debug_snapshot!(parse_ok(source, "test.spec").entities);
}
