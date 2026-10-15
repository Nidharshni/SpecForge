use specforge_common::Sym;
use specforge_parser::{Annotation, EntityId, EntityKind, FieldValue, parse};
use specforge_test_macros::test as specforge_test;
use std::path::PathBuf;

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "parse any keyword as generic entity_block"
)]
fn parse_basic_entity_block() {
    let source = r#"
behavior parse_spec_file "Parse Spec File" {
    status planned
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    assert_eq!(
        entity.kind,
        EntityKind {
            raw: Sym::new("behavior")
        }
    );
    assert_eq!(
        entity.id,
        EntityId {
            raw: Sym::new("parse_spec_file")
        }
    );
    assert_eq!(entity.title.as_deref(), Some("Parse Spec File"));

    let status = entity.fields.get("status").expect("missing 'status' field");
    assert!(matches!(status, FieldValue::Identifier(id) if id == "planned"));
}

#[specforge_test(behavior = "parse_use_imports", verify = "parse full use import")]
fn parse_use_import() {
    let source = "use \"behaviors/parsing\"\n";
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.imports.len(), 1);
    assert_eq!(result.imports[0].path, "behaviors/parsing");
    assert_eq!(result.imports[0].kind, specforge_parser::ImportKind::Full);
    assert!(result.imports[0].bindings.is_none());
}

#[specforge_test(
    behavior = "parse_use_imports",
    verify = "parse selective use import with braces"
)]
fn parse_selective_import() {
    let source = "use { SpecFile, ParseError } from \"types/core\"\n";
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.imports.len(), 1);
    assert_eq!(result.imports[0].path, "types/core");
    assert_eq!(
        result.imports[0].kind,
        specforge_parser::ImportKind::Selective
    );
    let bindings = result.imports[0]
        .bindings
        .as_ref()
        .expect("expected selective import");
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].name, "SpecFile");
    assert!(bindings[0].alias.is_none());
    assert_eq!(bindings[1].name, "ParseError");
    assert!(bindings[1].alias.is_none());
}

#[specforge_test(
    behavior = "parse_verify_statements",
    verify = "parse verify statement in any entity block"
)]
fn parse_verify_statements() {
    let source = r#"
behavior validate_input "Validate Input" {
    contract "Input must be valid UTF-8"
    verify unit "rejects invalid UTF-8"
    verify integration "validates end-to-end"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    let stmts = &entity.obligations;
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[0].kind, "unit");
    assert_eq!(stmts[0].description, "rejects invalid UTF-8");
    assert_eq!(stmts[1].kind, "integration");
    assert_eq!(stmts[1].description, "validates end-to-end");
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "spec block uses dedicated grammar rule"
)]
fn parse_spec_block() {
    let source = r#"
spec "SpecForge" {
    version "0.1.0"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    assert_eq!(
        entity.kind,
        EntityKind {
            raw: Sym::new("spec")
        }
    );
    assert_eq!(entity.title.as_deref(), Some("SpecForge"));

    let version = entity.fields.get("version").expect("missing version");
    assert!(matches!(version, FieldValue::String(v) if v == "0.1.0"));
}

#[specforge_test(behavior = "parse_ref_blocks", verify = "parse one-line ref syntax")]
fn parse_ref_inline() {
    let source = r#"ref gh.issue:42 "Support Wasm extensions""#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    assert_eq!(
        entity.kind,
        EntityKind {
            raw: Sym::new("ref")
        }
    );
    assert_eq!(entity.id.raw, "gh.issue:42");
    assert_eq!(entity.title.as_deref(), Some("Support Wasm extensions"));

    // Decomposed scheme/kind/identifier
    assert!(matches!(entity.fields.get("scheme"), Some(FieldValue::String(s)) if s == "gh"));
    assert!(matches!(entity.fields.get("ref_kind"), Some(FieldValue::String(s)) if s == "issue"));
    assert!(matches!(entity.fields.get("identifier"), Some(FieldValue::String(s)) if s == "42"));
}

#[specforge_test(
    behavior = "parse_ref_blocks",
    verify = "ref block supports optional title and body fields"
)]
fn parse_ref_full_block() {
    let source = r#"
ref gh.issue:99 "Performance tracking" {
    priority "high"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    assert_eq!(
        entity.kind,
        EntityKind {
            raw: Sym::new("ref")
        }
    );
    assert_eq!(entity.title.as_deref(), Some("Performance tracking"));
    assert!(matches!(entity.fields.get("priority"), Some(FieldValue::String(s)) if s == "high"));
}

#[specforge_test(
    behavior = "parse_define_blocks",
    verify = "parse define block with name and body"
)]
fn parse_define_block() {
    let source = r#"
define my_custom_type {
    base_kind "entity"
    verify unit "custom type works"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    assert_eq!(
        entity.kind,
        EntityKind {
            raw: Sym::new("define")
        }
    );
    assert_eq!(entity.id.raw, "my_custom_type");
    assert!(entity.title.is_none());
    assert!(matches!(entity.fields.get("base_kind"), Some(FieldValue::String(s)) if s == "entity"));
    assert!(!entity.obligations.is_empty());
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "generic block preserves kind, name, title, and fields"
)]
fn parse_all_field_value_types() {
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
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let e = &result.entities[0];

    assert!(matches!(e.fields.get("name"), Some(FieldValue::String(s)) if s == "a string"));
    assert!(matches!(
        e.fields.get("count"),
        Some(FieldValue::Integer(42))
    ));
    assert!(matches!(
        e.fields.get("enabled"),
        Some(FieldValue::Boolean(true))
    ));
    assert!(matches!(
        e.fields.get("disabled"),
        Some(FieldValue::Boolean(false))
    ));
    assert!(matches!(e.fields.get("created"), Some(FieldValue::Date(d)) if d == "2024-01-15"));
    assert!(matches!(e.fields.get("status"), Some(FieldValue::Identifier(id)) if id == "planned"));

    // Reference list (all identifiers)
    match e.fields.get("tags").expect("missing tags") {
        FieldValue::ReferenceList(items) => {
            let ids: Vec<&str> = items.iter().map(|r| r.id.as_str()).collect();
            assert_eq!(ids, ["alpha", "beta", "gamma"]);
        }
        other => panic!("expected ReferenceList, got {:?}", other),
    }

    // String list (contains quoted strings)
    match e.fields.get("labels").expect("missing labels") {
        FieldValue::StringList(items) => {
            assert_eq!(items, &["tag-one", "tag-two"]);
        }
        other => panic!("expected StringList, got {:?}", other),
    }

    // Nested block
    match e.fields.get("config").expect("missing config") {
        FieldValue::Block(map) => {
            assert!(matches!(map.get("timeout"), Some(FieldValue::Integer(30))));
            assert!(matches!(map.get("retries"), Some(FieldValue::Integer(3))));
        }
        other => panic!("expected Block, got {:?}", other),
    }
}

#[specforge_test(
    behavior = "parse_triple_quoted_strings",
    verify = "common leading whitespace is stripped"
)]
fn parse_triple_quoted_string() {
    let source = "behavior doc_test \"Doc Test\" {\n    contract \"\"\"\n        Given a valid input\n        When processed\n        Then output is correct\n    \"\"\"\n}\n";
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let e = &result.entities[0];
    let contract = e.fields.get("contract").expect("missing contract");
    match contract {
        FieldValue::String(s) => {
            // Should be dedented: leading common whitespace removed
            assert!(s.contains("Given a valid input"), "got: {s:?}");
            assert!(s.contains("When processed"), "got: {s:?}");
            assert!(s.contains("Then output is correct"), "got: {s:?}");
            // Should NOT have 8 spaces of leading indentation
            assert!(!s.starts_with("        "), "expected dedented, got: {s:?}");
        }
        other => panic!("expected String, got {:?}", other),
    }
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "valid blocks after syntax error are still parsed"
)]
fn multi_error_recovery() {
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

    // Should have errors from the broken block
    assert!(!result.errors.is_empty(), "expected parse errors");

    // But should still parse both valid blocks
    let good_entities: Vec<_> = result
        .entities
        .iter()
        .filter(|e| e.id.raw == "good_before" || e.id.raw == "good_after")
        .collect();

    assert_eq!(
        good_entities.len(),
        2,
        "expected both valid entities to be parsed despite error; got entities: {:?}",
        result
            .entities
            .iter()
            .map(|e| &e.id.raw)
            .collect::<Vec<_>>()
    );
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "parser collects multiple errors from one file"
)]
fn multiple_errors_produce_multiple_diagnostics() {
    let source = r#"
!!!

behavior valid "Valid" {
    status planned
}

@@@
"#;
    let result = parse(source, "test.spec");

    // Should collect errors from both broken sections
    assert!(
        result.errors.len() >= 2,
        "expected at least 2 errors, got {}: {:?}",
        result.errors.len(),
        result.errors
    );

    // Should still parse the valid entity
    assert_eq!(result.entities.len(), 1);
    assert_eq!(result.entities[0].id.raw, "valid");
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "unknown keyword parsed without error"
)]
fn unknown_keyword_parsed_without_error() {
    // Use a completely made-up keyword — the parser must accept it generically
    let source = r#"
zygomorphic my_entity "Made Up Kind" {
    status planned
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unknown keyword should not produce errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);
    assert_eq!(result.entities[0].kind.raw, "zygomorphic");
    assert_eq!(result.entities[0].id.raw, "my_entity");
    assert_eq!(result.entities[0].title.as_deref(), Some("Made Up Kind"));
}

#[test]
fn comments_are_skipped() {
    let source = r#"
// This is a file-level comment
use "behaviors/parsing" // inline comment

// Comment before entity
behavior example "Example" {
    // Comment inside block
    status planned
}
// Trailing comment
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.imports.len(), 1);
    assert_eq!(result.entities.len(), 1);
    assert_eq!(result.entities[0].id.raw, "example");
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "any keyword produces generic entity_block AST node"
)]
fn parse_entity_without_title() {
    let source = r#"
invariant no_orphans {
    guarantee "Every entity must have at least one incoming edge"
    risk high
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let e = &result.entities[0];
    assert_eq!(e.kind.raw, "invariant");
    assert_eq!(e.id.raw, "no_orphans");
    assert!(e.title.is_none());
}

#[specforge_test(
    behavior = "parse_spec_file_to_ast",
    verify = "parse valid file produces complete AST"
)]
fn parse_multiple_entities_in_one_file() {
    let source = r#"
use "types/core"

behavior first "First" {
    status planned
}

behavior second "Second" {
    status done
}

invariant third {
    guarantee "Always true"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.imports.len(), 1);
    assert_eq!(result.entities.len(), 3);
    assert_eq!(result.entities[0].id.raw, "first");
    assert_eq!(result.entities[1].id.raw, "second");
    assert_eq!(result.entities[2].id.raw, "third");
}

#[test]
fn parse_list_with_trailing_comma() {
    let source = r#"
behavior trailing "Trailing Comma" {
    tags [alpha, beta, gamma,]
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    match result.entities[0].fields.get("tags").expect("missing tags") {
        FieldValue::ReferenceList(items) => {
            let ids: Vec<&str> = items.iter().map(|r| r.id.as_str()).collect();
            assert_eq!(ids, ["alpha", "beta", "gamma"]);
        }
        other => panic!("expected ReferenceList, got {:?}", other),
    }
}

#[test]
fn parse_mixed_list_with_scheme_refs() {
    let source = r#"
behavior with_refs "Refs" {
    refs [gh.issue:42, jira.story:ABC-123]
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    match result.entities[0].fields.get("refs").expect("missing refs") {
        FieldValue::ReferenceList(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].id, "gh.issue:42");
            assert_eq!(items[1].id, "jira.story:ABC-123");
            // Each item keeps the exact span of its identifier token
            // (C3-07). Line 3 is `    refs [gh.issue:42, jira.story:ABC-123]`:
            // `gh.issue:42` occupies columns 11-21, `jira.story:ABC-123` 24-41.
            assert_eq!(items[0].span.start_line, 3);
            assert_eq!(items[0].span.start_col, 11);
            assert_eq!(items[0].span.end_col, 22);
            assert_eq!(items[1].span.start_line, 3);
            assert_eq!(items[1].span.start_col, 24);
            assert_eq!(items[1].span.end_col, 42);
        }
        other => panic!("expected ReferenceList with scheme refs, got {:?}", other),
    }
}

#[specforge_test(behavior = "parse_use_imports", verify = "parse full use import")]
fn parse_multiple_use_imports() {
    let source = r#"
use "behaviors/parsing"
use "types/core"
use "invariants/core"
use "features/output"
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.imports.len(), 4);
    assert_eq!(result.imports[0].path, "behaviors/parsing");
    assert_eq!(result.imports[1].path, "types/core");
    assert_eq!(result.imports[2].path, "invariants/core");
    assert_eq!(result.imports[3].path, "features/output");
}

#[specforge_test(
    behavior = "parse_spec_file_to_ast",
    verify = "AST source spans match original token positions"
)]
fn source_spans_are_accurate() {
    let source = "behavior first \"First\" {\n    status planned\n}\n";
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let span = &result.entities[0].span;
    assert_eq!(span.file, "test.spec");
    assert_eq!(span.start_line, 1);
    assert_eq!(span.start_col, 1);
    assert_eq!(span.end_line, 3);
}

#[specforge_test(
    behavior = "parse_spec_file_to_ast",
    verify = "parse valid file produces complete AST"
)]
fn parse_empty_file() {
    let result = parse("", "empty.spec");
    assert!(result.errors.is_empty());
    assert!(result.entities.is_empty());
    assert!(result.imports.is_empty());
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "any keyword produces generic entity_block AST node"
)]
fn parse_empty_block() {
    let source = "behavior empty_block \"Empty\" {\n}\n";
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);
    assert_eq!(result.entities[0].id.raw, "empty_block");
    assert_eq!(result.entities[0].fields.entries().len(), 0);
}

#[specforge_test(
    behavior = "parse_spec_file_to_ast",
    verify = "parse valid file produces complete AST"
)]
fn parse_real_world_spec_file() {
    // A realistic .spec file exercising many constructs at once
    let source = r#"
// Parsing behaviors
use "invariants/core"
use "types/core"
use "types/errors"

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
    verify unit "collects multiple syntax errors"
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
    let result = parse(source, "behaviors/parsing.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.imports.len(), 3);
    assert_eq!(result.entities.len(), 3); // 2 behaviors + 1 ref

    // First behavior
    let b1 = &result.entities[0];
    assert_eq!(b1.kind.raw, "behavior");
    assert_eq!(b1.id.raw, "parse_spec_file_to_ast");
    assert_eq!(b1.title.as_deref(), Some("Parse Spec File to AST"));
    assert!(b1.fields.get("invariants").is_some());
    assert!(b1.fields.get("requires").is_some());
    assert!(b1.fields.get("ensures").is_some());
    assert!(b1.fields.get("contract").is_some());
    assert!(!b1.obligations.is_empty());

    // Ref
    let r = &result.entities[1];
    assert_eq!(r.kind.raw, "ref");

    // Second behavior
    let b2 = &result.entities[2];
    assert_eq!(b2.id.raw, "recover_from_syntax_errors");
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "generic block preserves kind, name, title, and fields"
)]
fn parse_deeply_nested_dbc_blocks() {
    let source = r#"
behavior complex "Complex" {
    requires {
        precondition_a "First precondition"
        precondition_b "Second precondition"
    }
    ensures {
        postcondition_a "First postcondition"
    }
    maintains {
        invariant_a "Always holds"
    }
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let e = &result.entities[0];

    // requires, ensures, maintains are nested blocks
    match e.fields.get("requires").expect("missing requires") {
        FieldValue::Block(map) => {
            assert!(matches!(
                map.get("precondition_a"),
                Some(FieldValue::String(s)) if s == "First precondition"
            ));
            assert!(matches!(
                map.get("precondition_b"),
                Some(FieldValue::String(s)) if s == "Second precondition"
            ));
        }
        other => panic!("expected Block for requires, got {:?}", other),
    }

    match e.fields.get("ensures").expect("missing ensures") {
        FieldValue::Block(map) => {
            assert_eq!(map.entries().len(), 1);
        }
        other => panic!("expected Block for ensures, got {:?}", other),
    }
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "any keyword produces generic entity_block AST node"
)]
fn parse_type_struct_with_annotations() {
    let source = r#"
type SpecFile {
    path       string    @readonly
    imports    ImportDeclaration[]
    entities   Entity[]
    errors     ParseError[]  @optional
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    assert_eq!(entity.kind.raw, "type");
    assert_eq!(entity.id.raw, "SpecFile");
    // Should have parsed at least the field names
    assert!(entity.fields.get("path").is_some(), "missing 'path' field");
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "any keyword produces generic entity_block AST node"
)]
fn parse_union_type() {
    let source = r#"
type FieldValue = StringValue | ReferenceList | StringList | Block | VerifyList
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    assert_eq!(entity.kind.raw, "type");
    assert_eq!(entity.id.raw, "FieldValue");
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "any keyword produces generic entity_block AST node"
)]
fn parse_string_union_type() {
    let source = r#"
type McpErrorCode = "invalid_input" | "compilation_failed" | "entity_not_found"
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    assert_eq!(entity.kind.raw, "type");
    assert_eq!(entity.id.raw, "McpErrorCode");
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "any keyword produces generic entity_block AST node"
)]
fn parse_multiline_union_type() {
    // Multi-line union with continuation via leading |
    let source = r#"
type ExportFormat = "json" | "dot" | "context"
  | "brief" | "graph"
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);
    assert_eq!(result.entities[0].id.raw, "ExportFormat");
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "any keyword produces generic entity_block AST node"
)]
fn parse_integer_union_type() {
    let source = r#"
type JsonRpcErrorCode = -32700 | -32600 | -32601
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);
    assert_eq!(result.entities[0].id.raw, "JsonRpcErrorCode");
}

#[test]
fn reject_use_import_with_spec_extension() {
    let source = "use \"behaviors/parsing.spec\"\n";
    let result = parse(source, "test.spec");

    // The path is a quoted string containing ".spec" — the parser will parse it,
    // but validation (resolver) rejects it. Check the path is correctly extracted.
    assert_eq!(result.imports.len(), 1);
    assert_eq!(result.imports[0].path, "behaviors/parsing.spec");
}

#[specforge_test(
    behavior = "parse_verify_statements",
    verify = "verify parsed in spec block"
)]
fn verify_statements_in_spec_block() {
    let source = r#"
spec "MyProject" {
    version "1.0.0"
    verify integration "project compiles end-to-end"
    verify unit "version field is valid semver"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    assert_eq!(
        entity.kind,
        EntityKind {
            raw: Sym::new("spec")
        }
    );

    let stmts = &entity.obligations;
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[0].kind, "integration");
    assert_eq!(stmts[0].description, "project compiles end-to-end");
    assert_eq!(stmts[1].kind, "unit");
    assert_eq!(stmts[1].description, "version field is valid semver");
}

#[specforge_test(
    behavior = "parse_verify_statements",
    verify = "verify parsed in define block"
)]
fn verify_statements_in_define_block() {
    let source = r#"
define my_custom_kind {
    base_kind "entity"
    testable true
    verify unit "custom kind registers in KindRegistry"
    verify property "custom kind passes round-trip serialization"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    assert_eq!(
        entity.kind,
        EntityKind {
            raw: Sym::new("define")
        }
    );

    let stmts = &entity.obligations;
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[0].kind, "unit");
    assert_eq!(stmts[1].kind, "property");
}

/// The `after` entity of the unclosed-string fixtures, with its fields.
fn recovered_after<'a>(
    result: &'a specforge_parser::SpecFile,
    status: &str,
) -> &'a specforge_parser::Entity {
    let after = result
        .entities
        .iter()
        .find(|e| e.id.raw == "after")
        .unwrap_or_else(|| {
            panic!(
                "the block after the unclosed string was lost; entities: {:?}, errors: {:?}",
                result
                    .entities
                    .iter()
                    .map(|e| e.id.raw.as_str())
                    .collect::<Vec<_>>(),
                result.errors
            )
        });
    assert_eq!(after.kind.raw, "behavior");
    assert_eq!(after.title.as_deref(), Some("After"));
    assert!(
        matches!(after.fields.get("status"), Some(FieldValue::Identifier(s)) if s == status),
        "after.status: {:?}",
        after.fields.get("status")
    );
    after
}

#[specforge_test(
    behavior = "parse_triple_quoted_strings",
    verify = "recover from unclosed triple-quoted string with diagnostic"
)]
fn unclosed_triple_quoted_string_produces_error() {
    let source = r#"
behavior broken "Broken" {
    contract """
        This string is never closed
}

behavior after "After" {
    status planned
}
"#;
    let result = parse(source, "test.spec");

    // One diagnostic, at the opening `"""` (line 3, column 14).
    assert_eq!(result.errors.len(), 1, "errors: {:?}", result.errors);
    let err = &result.errors[0];
    assert_eq!(err.span.file, "test.spec");
    assert_eq!(
        (err.span.start_line, err.span.start_col),
        (3, 14),
        "{err:?}"
    );
    assert!(err.message.contains("unclosed"), "{}", err.message);

    // The string ends before the next block, which parses normally.
    let after = recovered_after(&result, "planned");
    assert_eq!((after.span.start_line, after.span.start_col), (7, 1));
    assert_eq!((after.span.end_line, after.span.end_col), (9, 2));
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "valid blocks after syntax error are still parsed"
)]
fn unclosed_regular_string_recovers_next_block() {
    // A regular string may span lines: without recovery, the unclosed title
    // pairs with the next title's opening quote and swallows its header.
    let source = "behavior broken \"Broken {\n    status planned\n}\n\nbehavior after \"After\" {\n    status done\n}\n";
    let result = parse(source, "test.spec");

    assert_eq!(result.errors.len(), 1, "errors: {:?}", result.errors);
    let err = &result.errors[0];
    assert_eq!(err.span.file, "test.spec");
    assert_eq!(
        (err.span.start_line, err.span.start_col),
        (1, 17),
        "{err:?}"
    );
    assert!(err.message.contains("unclosed"), "{}", err.message);

    let after = recovered_after(&result, "done");
    assert_eq!((after.span.start_line, after.span.start_col), (5, 1));
    assert_eq!((after.span.end_line, after.span.end_col), (7, 2));
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "valid blocks after syntax error are still parsed"
)]
fn each_unclosed_string_is_reported_and_the_blocks_between_survive() {
    // Two unclosed strings whose stray quotes happen to pair up (1 + 3),
    // and a well-formed string inside the first broken block.
    let source = "behavior one \"One {\n    contract \"Oops.\"\n}\n\nbehavior two \"Two\" {\n    status planned\n    contract \"\"\"\n        never closed\n}\n\nbehavior after \"After\" {\n    status done\n}\n";
    let result = parse(source, "test.spec");

    let at: Vec<(usize, usize)> = result
        .errors
        .iter()
        .map(|e| (e.span.start_line, e.span.start_col))
        .collect();
    assert_eq!(at, vec![(1, 14), (7, 14)], "errors: {:?}", result.errors);
    assert!(result.errors.iter().all(|e| e.message.contains("unclosed")));

    let two = result
        .entities
        .iter()
        .find(|e| e.id.raw == "two")
        .expect("the block between the two unclosed strings");
    assert!(matches!(two.fields.get("status"), Some(FieldValue::Identifier(s)) if s == "planned"));
    assert_eq!(two.span.start_line, 5);
    recovered_after(&result, "done");
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "a closed multi-line string that contains a block-like line stays whole"
)]
fn closed_multiline_string_with_block_like_line_is_not_split() {
    // Both string kinds, each holding a line that looks like a block start.
    // The file also has an unrelated syntax error, so recovery is armed.
    let source = "behavior doc \"Doc\" {\n    description \"Example:\nbehavior inner \\\"Inner\\\" {\n}\"\n    contract \"\"\"\n        Example:\nbehavior nested \"Nested\" {\n}\n    \"\"\"\n}\n\nbehavior ??? {\n}\n\nbehavior after \"After\" {\n    status done\n}\n";
    let result = parse(source, "test.spec");

    let doc = result
        .entities
        .iter()
        .find(|e| e.id.raw == "doc")
        .expect("doc entity");
    assert!(
        matches!(doc.fields.get("description"),
            Some(FieldValue::String(s)) if s == "Example:\nbehavior inner \"Inner\" {\n}"),
        "description: {:?}",
        doc.fields.get("description")
    );
    assert!(
        matches!(doc.fields.get("contract"),
            Some(FieldValue::String(s)) if s.contains("behavior nested \"Nested\" {")),
        "contract: {:?}",
        doc.fields.get("contract")
    );
    assert!(
        result
            .entities
            .iter()
            .all(|e| e.id.raw != "inner" && e.id.raw != "nested"),
        "a string's content was parsed as a block"
    );
    recovered_after(&result, "done");
    assert!(
        result
            .errors
            .iter()
            .all(|e| !e.message.contains("unclosed")),
        "errors: {:?}",
        result.errors
    );

    // Without the unrelated error the file is valid and parses cleanly.
    let valid = source.replace("behavior ??? {\n}\n\n", "");
    let result = parse(&valid, "test.spec");
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
    assert_eq!(result.entities.len(), 2);
}

#[specforge_test(
    behavior = "parse_ref_blocks",
    verify = "reject ref block with missing scheme or identifier"
)]
fn malformed_ref_missing_scheme_produces_error() {
    // ref without proper scheme.kind:identifier should produce a parse error
    // since the grammar requires scheme_ref_id format, tree-sitter will
    // produce an ERROR node for malformed refs
    let source = "ref badref \"Missing scheme\"\n";
    let result = parse(source, "test.spec");

    // Either parsed as error, or parsed as entity with an error diagnostic
    let has_error = !result.errors.is_empty();
    let not_a_ref = result.entities.iter().all(|e| e.kind.raw != "ref");
    assert!(
        has_error || not_a_ref,
        "malformed ref should produce an error or not be parsed as ref; errors: {:?}, entities: {:?}",
        result.errors,
        result
            .entities
            .iter()
            .map(|e| (&e.kind.raw, &e.id.raw))
            .collect::<Vec<_>>()
    );
}

#[specforge_test(
    behavior = "parse_spec_file_to_ast",
    verify = "parse valid file produces complete AST"
)]
fn spec_files_without_extension_syntax_parse_cleanly() {
    // Files that use ONLY standard field syntax (no port method signatures,
    // no term sub-blocks) should parse with ZERO errors.
    // This catches grammar regressions on the ~90% of spec files that
    // use standard entity_block / spec / ref / define / union syntax.
    let spec_dir = specforge_repo_root::repo_root().join("spec");

    // Files with known extension-specific body syntax that the core grammar
    // correctly cannot parse — these will be handled by body parsers.
    let extension_syntax_files: &[&str] = &[
        "ports/inbound.spec",  // port method signatures: method name(...) -> Result<...>
        "ports/outbound.spec", // port method signatures
        "glossary.spec",       // nested term sub-blocks inside term singleton
        "specforge.spec",      // spec block with extension-specific fields
        "types/zero-entity-core.spec", // inline union field types: string | string[]
    ];

    let mut failures = Vec::new();
    for entry in walkdir(spec_dir.clone()) {
        let rel = entry
            .strip_prefix(&spec_dir)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // Skip files with known extension-specific syntax
        if extension_syntax_files.iter().any(|&f| rel == f) {
            continue;
        }
        // Skip extension-specific ports files
        if rel.contains("extensions/") && rel.contains("ports") {
            continue;
        }

        let source = std::fs::read_to_string(&entry).unwrap();
        let result = parse(&source, entry.to_str().unwrap());
        if !result.errors.is_empty() {
            failures.push((
                rel,
                result.errors.len(),
                result
                    .errors
                    .iter()
                    .map(|e| {
                        format!(
                            "  L{}:{}: {}",
                            e.span.start_line, e.span.start_col, e.message
                        )
                    })
                    .collect::<Vec<_>>(),
            ));
        }
    }

    if !failures.is_empty() {
        let mut msg = format!("{} standard spec files had parse errors:\n", failures.len());
        for (path, count, details) in &failures {
            msg.push_str(&format!("\n{path} ({count} errors):\n"));
            for d in details {
                msg.push_str(&format!("{d}\n"));
            }
        }
        panic!("{msg}");
    }
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "field annotations are extracted into FieldEntry"
)]
fn annotation_with_string_value() {
    let source = r#"
type WasmConfig {
    max_memory_pages   integer   @optional @doc "Default: 16"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "annotation with string value should not produce errors: {:?}",
        result.errors
    );
    let entry = result.entities[0]
        .fields
        .entries()
        .iter()
        .find(|e| e.key == "max_memory_pages")
        .expect("field should be parsed");
    // Both annotations, in order; the string one keeps its value unquoted.
    let annotations: Vec<(&str, Option<&str>)> = entry
        .annotations
        .iter()
        .map(|a| (a.name.as_str(), a.value.as_deref()))
        .collect();
    assert_eq!(
        annotations,
        [("optional", None), ("doc", Some("Default: 16"))]
    );
}

#[specforge_test(
    behavior = "parse_verify_statements",
    verify = "verify kind and description extracted correctly"
)]
fn verify_statement_without_kind() {
    let source = r#"
event manifests_loaded "Manifests Loaded" {
    verify "Extension manifests MUST be loaded before populating the kind registry"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "verify without kind should not produce errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    let stmts = &entity.obligations;
    assert_eq!(stmts.len(), 1);
    // When kind is omitted, it should default to empty string or a sentinel
    assert!(
        stmts[0].kind.is_empty(),
        "kind should be empty when omitted, got: {:?}",
        stmts[0].kind
    );
    assert_eq!(
        stmts[0].description,
        "Extension manifests MUST be loaded before populating the kind registry"
    );
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "parse string field values correctly"
)]
fn string_field_values_distinct_from_identifiers() {
    let source = r#"
behavior example "Example" {
    status     planned
    contract   "This is a string value"
    count      42
    active     true
    created    2025-01-15
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let e = &result.entities[0];

    // Bare word -> Identifier
    assert!(
        matches!(e.fields.get("status"), Some(FieldValue::Identifier(s)) if s == "planned"),
        "bare word 'planned' should be FieldValue::Identifier, got: {:?}",
        e.fields.get("status")
    );

    // Quoted string -> String
    assert!(
        matches!(e.fields.get("contract"), Some(FieldValue::String(s)) if s == "This is a string value"),
        "quoted text should be FieldValue::String, got: {:?}",
        e.fields.get("contract")
    );

    // Integer
    assert!(
        matches!(e.fields.get("count"), Some(FieldValue::Integer(42))),
        "integer should be FieldValue::Integer, got: {:?}",
        e.fields.get("count")
    );

    // Boolean
    assert!(
        matches!(e.fields.get("active"), Some(FieldValue::Boolean(true))),
        "true should be FieldValue::Boolean, got: {:?}",
        e.fields.get("active")
    );

    // Date
    assert!(
        matches!(e.fields.get("created"), Some(FieldValue::Date(d)) if d == "2025-01-15"),
        "date should be FieldValue::Date, got: {:?}",
        e.fields.get("created")
    );
}

#[specforge_test(
    behavior = "parse_triple_quoted_strings",
    verify = "relative indentation is preserved"
)]
fn triple_quoted_relative_indentation_preserved() {
    let source = "behavior b \"B\" {\n    contract \"\"\"\n        line one\n            indented deeper\n        back to base\n    \"\"\"\n}\n";
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let contract = result.entities[0]
        .fields
        .get("contract")
        .expect("missing contract");
    match contract {
        FieldValue::String(s) => {
            let lines: Vec<&str> = s.lines().collect();
            assert_eq!(lines.len(), 3, "expected 3 lines, got: {lines:?}");
            assert_eq!(lines[0], "line one");
            assert_eq!(lines[1], "    indented deeper");
            assert_eq!(lines[2], "back to base");
        }
        other => panic!("expected String, got {:?}", other),
    }
}

#[test]
fn parse_empty_reference_list() {
    let source = r#"
behavior foo "T" {
    tags []
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    match result.entities[0].fields.get("tags").expect("missing tags") {
        FieldValue::ReferenceList(items) => {
            assert!(items.is_empty(), "expected empty list, got: {items:?}");
        }
        other => panic!("expected ReferenceList, got {:?}", other),
    }
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "generic block preserves kind, name, title, and fields"
)]
fn parse_empty_nested_block() {
    let source = r#"
behavior foo "T" {
    requires {}
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    match result.entities[0]
        .fields
        .get("requires")
        .expect("missing requires")
    {
        FieldValue::Block(map) => {
            assert!(
                map.entries().is_empty(),
                "expected empty block, got {} entries",
                map.entries().len()
            );
        }
        other => panic!("expected Block, got {:?}", other),
    }
}

#[specforge_test(
    behavior = "parse_define_blocks",
    verify = "define block supports standard field syntax"
)]
fn define_block_supports_standard_field_syntax() {
    let source = r#"
define my_kind {
    testable true
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    // `testable true` is parsed as a standard field.
    assert!(
        matches!(
            result.entities[0].fields.get("testable"),
            Some(FieldValue::Boolean(true))
        ),
        "{:?}",
        result.entities[0].fields
    );
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "any keyword produces generic entity_block AST node"
)]
fn parse_single_variant_union() {
    let source = "type Singleton = OnlyOne\n";
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);
    assert_eq!(result.entities[0].id.raw, "Singleton");

    match result.entities[0]
        .fields
        .get("variants")
        .expect("missing variants")
    {
        FieldValue::VariantList(variants) => {
            assert_eq!(variants.len(), 1, "expected 1 variant, got: {variants:?}");
            assert_eq!(variants[0], "OnlyOne");
        }
        other => panic!("expected VariantList for variants, got {:?}", other),
    }
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "any keyword produces generic entity_block AST node"
)]
fn parse_union_type_variants_extracted() {
    let source =
        "type FieldValue = StringValue | ReferenceList | StringList | Block | VerifyList\n";
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities[0].id.raw, "FieldValue");

    match result.entities[0]
        .fields
        .get("variants")
        .expect("missing variants")
    {
        FieldValue::VariantList(variants) => {
            assert_eq!(
                variants,
                &[
                    "StringValue",
                    "ReferenceList",
                    "StringList",
                    "Block",
                    "VerifyList"
                ]
            );
        }
        other => panic!("expected VariantList for variants, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// NEW TESTS: Fill coverage gaps for uncovered verify statements
// ---------------------------------------------------------------------------

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "ref block uses dedicated grammar rule"
)]
fn ref_block_uses_dedicated_grammar_rule() {
    // Ref blocks have unique syntax: ref scheme.kind:identifier [title] { fields }
    // This verifies that ref is parsed via a dedicated grammar rule, not generic entity_block
    let source = r#"
ref gh.pr:100 "Ref grammar rule" {
    status "open"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "ref block should parse without errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    assert_eq!(entity.kind.raw, "ref");
    assert_eq!(entity.id.raw, "gh.pr:100");
    assert_eq!(entity.title.as_deref(), Some("Ref grammar rule"));

    // Ref-specific decomposition proves dedicated rule (generic blocks don't extract these)
    assert!(matches!(entity.fields.get("scheme"), Some(FieldValue::String(s)) if s == "gh"));
    assert!(matches!(entity.fields.get("ref_kind"), Some(FieldValue::String(s)) if s == "pr"));
    assert!(matches!(entity.fields.get("identifier"), Some(FieldValue::String(s)) if s == "100"));
    assert!(matches!(entity.fields.get("status"), Some(FieldValue::String(s)) if s == "open"));
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "define block uses dedicated grammar rule"
)]
fn define_block_uses_dedicated_grammar_rule() {
    // Define blocks have unique syntax: define <name> { fields }
    // No title allowed — this differentiates from generic entity_block
    let source = r#"
define custom_entity {
    base_kind "entity"
    testable true
    singleton false
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "define block should parse without errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    assert_eq!(entity.kind.raw, "define");
    assert_eq!(entity.id.raw, "custom_entity");
    // Define blocks have no title — this distinguishes from generic entity_block
    assert!(
        entity.title.is_none(),
        "define blocks must not have a title"
    );
    assert!(matches!(entity.fields.get("base_kind"), Some(FieldValue::String(s)) if s == "entity"));
    assert!(matches!(
        entity.fields.get("testable"),
        Some(FieldValue::Boolean(true))
    ));
    assert!(matches!(
        entity.fields.get("singleton"),
        Some(FieldValue::Boolean(false))
    ));
}

#[specforge_test(
    behavior = "parse_triple_quoted_strings",
    verify = "triple-quoted string preserves newlines"
)]
fn triple_quoted_string_preserves_newlines() {
    // Verify that internal newlines are preserved in triple-quoted strings
    let source = "behavior b \"B\" {\n    contract \"\"\"\n        First line\n        Second line\n        Third line\n    \"\"\"\n}\n";
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let contract = result.entities[0]
        .fields
        .get("contract")
        .expect("missing contract");
    match contract {
        FieldValue::String(s) => {
            let lines: Vec<&str> = s.lines().collect();
            assert_eq!(
                lines.len(),
                3,
                "expected 3 lines (newlines preserved), got: {lines:?}"
            );
            assert_eq!(lines[0], "First line");
            assert_eq!(lines[1], "Second line");
            assert_eq!(lines[2], "Third line");
        }
        other => panic!("expected String, got {:?}", other),
    }
}

#[specforge_test(
    behavior = "parse_verify_statements",
    verify = "parse multiple verify statements in same entity"
)]
fn parse_multiple_verify_statements_in_same_entity() {
    let source = r#"
behavior multi_verify "Multi Verify" {
    contract "The system MUST handle multiple verify statements"
    verify unit "first unit test"
    verify unit "second unit test"
    verify integration "integration test"
    verify property "property-based test"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    let stmts = &entity.obligations;
    assert_eq!(
        stmts.len(),
        4,
        "expected 4 verify statements in same entity"
    );
    assert_eq!(stmts[0].kind, "unit");
    assert_eq!(stmts[0].description, "first unit test");
    assert_eq!(stmts[1].kind, "unit");
    assert_eq!(stmts[1].description, "second unit test");
    assert_eq!(stmts[2].kind, "integration");
    assert_eq!(stmts[2].description, "integration test");
    assert_eq!(stmts[3].kind, "property");
    assert_eq!(stmts[3].description, "property-based test");
}

#[specforge_test(
    behavior = "parse_ref_blocks",
    verify = "parse ref block with scheme.kind:identifier format"
)]
fn parse_ref_block_with_scheme_kind_identifier_format() {
    // Verify the full ref block syntax with scheme.kind:identifier is parsed
    let source = r#"ref jira.epic:PROJ-42 "Epic tracking""#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    assert_eq!(entity.kind.raw, "ref");
    assert_eq!(entity.id.raw, "jira.epic:PROJ-42");
    assert_eq!(entity.title.as_deref(), Some("Epic tracking"));

    // scheme.kind:identifier components must be decomposed
    assert!(matches!(entity.fields.get("scheme"), Some(FieldValue::String(s)) if s == "jira"));
    assert!(matches!(entity.fields.get("ref_kind"), Some(FieldValue::String(s)) if s == "epic"));
    assert!(
        matches!(entity.fields.get("identifier"), Some(FieldValue::String(s)) if s == "PROJ-42")
    );
}

#[specforge_test(
    behavior = "parse_define_blocks",
    verify = "define block parsed without extension knowledge"
)]
fn define_block_parsed_without_extension_knowledge() {
    // Define blocks are core grammar constructs — they must parse even when
    // no extensions are loaded. The parser has no extension context at all,
    // so successful parsing proves no extension knowledge is required.
    let source = r#"
define exotic_kind {
    base_kind "entity"
    testable true
    singleton false
    supports_verify true
    incremental false
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "define block must parse without any extension knowledge: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    assert_eq!(entity.kind.raw, "define");
    assert_eq!(entity.id.raw, "exotic_kind");
    // All standard field types must work inside define blocks
    assert!(matches!(entity.fields.get("base_kind"), Some(FieldValue::String(s)) if s == "entity"));
    assert!(matches!(
        entity.fields.get("testable"),
        Some(FieldValue::Boolean(true))
    ));
    assert!(matches!(
        entity.fields.get("singleton"),
        Some(FieldValue::Boolean(false))
    ));
    assert!(matches!(
        entity.fields.get("supports_verify"),
        Some(FieldValue::Boolean(true))
    ));
    assert!(matches!(
        entity.fields.get("incremental"),
        Some(FieldValue::Boolean(false))
    ));
}

// --- pub use import tests ---

#[specforge_test(behavior = "parse_use_imports", verify = "parse full use import")]
fn parse_pub_use_import() {
    let source = "pub use \"./foo\"\n";
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.imports.len(), 1);
    assert_eq!(result.imports[0].path, "./foo");
    assert!(result.imports[0].is_pub, "pub use should set is_pub = true");
    assert_eq!(result.imports[0].kind, specforge_parser::ImportKind::Full);
    assert!(result.imports[0].bindings.is_none());
}

#[specforge_test(
    behavior = "parse_use_imports",
    verify = "parse selective use import with braces"
)]
fn parse_pub_use_selective() {
    let source = "pub use { Bar, Baz } from \"./foo\"\n";
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.imports.len(), 1);
    assert!(result.imports[0].is_pub, "pub use should set is_pub = true");
    assert_eq!(
        result.imports[0].kind,
        specforge_parser::ImportKind::Selective
    );
    let bindings = result.imports[0]
        .bindings
        .as_ref()
        .expect("expected selective bindings");
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].name, "Bar");
    assert_eq!(bindings[1].name, "Baz");
}

#[specforge_test(behavior = "parse_use_imports", verify = "parse full use import")]
fn parse_mixed_use_and_pub_use() {
    let source = r#"
use "./private"
pub use "./public"
use "./another_private"
pub use { Foo } from "./another_public"
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.imports.len(), 4);
    assert!(!result.imports[0].is_pub, "use should set is_pub = false");
    assert!(result.imports[1].is_pub, "pub use should set is_pub = true");
    assert!(!result.imports[2].is_pub, "use should set is_pub = false");
    assert!(result.imports[3].is_pub, "pub use should set is_pub = true");
    assert_eq!(
        result.imports[3].kind,
        specforge_parser::ImportKind::Selective
    );
    let bindings = result.imports[3]
        .bindings
        .as_ref()
        .expect("expected selective bindings");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].name, "Foo");
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "negative integer parsed as field value"
)]
fn parse_negative_integer_as_field_value() {
    let source = r#"
behavior test "Test" {
    priority -1
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    assert_eq!(result.entities.len(), 1);

    let entity = &result.entities[0];
    match entity.fields.get("priority") {
        Some(FieldValue::Integer(val)) => assert_eq!(*val, -1, "expected -1, got {val}"),
        other => panic!("expected FieldValue::Integer(-1), got {:?}", other),
    }
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "negative integer parsed as field value with larger magnitude"
)]
fn parse_negative_integer_larger_magnitude() {
    let source = r#"
behavior test "Test" {
    offset -42
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    match entity.fields.get("offset") {
        Some(FieldValue::Integer(val)) => assert_eq!(*val, -42, "expected -42, got {val}"),
        other => panic!("expected FieldValue::Integer(-42), got {:?}", other),
    }
}

fn walkdir(dir: PathBuf) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                results.extend(walkdir(path));
            } else if path.extension().is_some_and(|ext| ext == "spec") {
                results.push(path);
            }
        }
    }
    results
}

#[test]
fn unquoted_import_path_misparsed_as_entity() {
    // BUG: unquoted import paths like `use invariants/core` are not matched by use_import rule
    // which requires quotes. Tree-sitter error recovery might be creating entity_block nodes.
    let source = r#"
use invariants/core

behavior test "Test" {
  status planned
}
"#;

    let result = parse(source, "test.spec");

    eprintln!("=== UNQUOTED IMPORT TEST ===");
    eprintln!("Imports: {}", result.imports.len());
    for imp in &result.imports {
        eprintln!("  - path={}", imp.path);
    }

    eprintln!("Entities: {}", result.entities.len());
    for (i, ent) in result.entities.iter().enumerate() {
        eprintln!(
            "  [{}] kind={}, id={}, line={}",
            i, ent.kind.raw, ent.id.raw, ent.span.start_line
        );
    }

    eprintln!("Errors: {}", result.errors.len());
    for err in &result.errors {
        eprintln!("  - line={}: {}", err.span.start_line, err.message);
    }
}

#[test]
fn debug_treesitter_ast() {
    use tree_sitter::Parser;

    let source = r#"use invariants/core

behavior test "Test" {
  status planned
}"#;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_specforge::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();

    fn print_node(node: tree_sitter::Node, source: &str, indent: usize) {
        let indent_str = " ".repeat(indent);
        let text = node.utf8_text(source.as_bytes()).unwrap_or("???");
        let text_display = if text.len() > 40 {
            format!("{}...", &text[..40])
        } else {
            text.to_string()
        };
        eprintln!(
            "{}kind={}, line={}, text={:?}",
            indent_str,
            node.kind(),
            node.start_position().row + 1,
            text_display
        );

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            print_node(child, source, indent + 2);
        }
    }

    eprintln!("\n=== TREE-SITTER AST ===");
    print_node(tree.root_node(), source, 0);
}

// === Phase 6: Parser Robustness ===

// B:parse_all_block_types — verify unit "integer overflow produces parse error instead of silent 0"
#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "integer overflow produces parse error instead of silent 0"
)]
fn integer_overflow_produces_parse_error() {
    let source = r#"
behavior overflowed "Overflow" {
    priority 99999999999999999999999999999
}
"#;
    let result = parse(source, "test.spec");

    // Should report an error about integer overflow
    let has_overflow_error = result
        .errors
        .iter()
        .any(|e| e.message.contains("integer") && e.message.contains("too large"));
    assert!(
        has_overflow_error,
        "expected parse error for integer overflow, got errors: {:?}",
        result.errors
    );

    // Entity is still created (error recovery), but the error is surfaced
    assert!(
        !result.errors.is_empty(),
        "overflow must be reported as an error"
    );
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("too large")),
        "the error says the integer is too large: {:?}",
        result.errors
    );
}

// B:recover_from_syntax_errors — verify unit "missing opening brace produces a parse error"
#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "missing opening brace produces a parse error"
)]
fn missing_opening_brace_produces_error() {
    let source = "behavior no_brace \"No Brace\"\n    status planned\n}\n";
    let result = parse(source, "test.spec");

    assert!(
        !result.errors.is_empty(),
        "missing brace must produce a parse error, got entities {:?}",
        result.entities
    );
    assert!(result.errors.iter().all(|e| e.span.file == "test.spec"));
}

// B:recover_from_syntax_errors — verify unit "completely invalid syntax produces error with location"
#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "completely invalid syntax produces error with location"
)]
fn completely_invalid_syntax_produces_error_with_location() {
    let source = "@@@ invalid !!! garbage\n";
    let result = parse(source, "test.spec");

    assert!(
        !result.errors.is_empty(),
        "invalid syntax should produce errors, got none"
    );
    // Error should have a meaningful span
    let err = &result.errors[0];
    assert!(
        err.span.start_line >= 1,
        "error span should have valid line number"
    );
}

// B:parse_all_block_types — verify unit "valid integer parses correctly"
#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "valid integer parses correctly"
)]
fn valid_integer_parses_correctly() {
    let source = r#"
behavior normal "Normal" {
    priority 42
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    match entity.fields.get("priority") {
        Some(FieldValue::Integer(val)) => assert_eq!(*val, 42),
        other => panic!("expected Integer(42), got {:?}", other),
    }
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "parse string field values correctly"
)]
fn unquote_processes_escaped_quotes() {
    let source = r#"behavior test "Test" { contract "hello \"world\"" }"#;
    let result = parse(source, "test.spec");
    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    match entity.fields.get("contract") {
        Some(FieldValue::String(val)) => assert_eq!(val, r#"hello "world""#),
        other => panic!(r#"expected String with unescaped quotes, got {:?}"#, other),
    }
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "parse string field values correctly"
)]
fn unquote_processes_escaped_backslashes() {
    let source = r#"behavior test "Test" { contract "back\\slash" }"#;
    let result = parse(source, "test.spec");
    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    match entity.fields.get("contract") {
        Some(FieldValue::String(val)) => assert_eq!(val, r#"back\slash"#),
        other => panic!(
            r#"expected String with unescaped backslash, got {:?}"#,
            other
        ),
    }
}

// === H8: Annotations on fields ===

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "field annotations are extracted into FieldEntry"
)]
fn parse_field_annotation_deprecated() {
    // A single @deprecated annotation on a field should be extracted
    let source = r#"
behavior annotated "Annotated" {
    status "active"  @deprecated
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    let entry = entity
        .fields
        .entries()
        .iter()
        .find(|e| e.key == "status")
        .expect("missing 'status' field");
    assert_eq!(
        entry.annotations.len(),
        1,
        "expected 1 annotation, got: {:?}",
        entry.annotations
    );
    assert_eq!(
        entry.annotations[0],
        Annotation {
            name: "deprecated".to_string(),
            value: None
        }
    );
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "multiple annotations on a single field are all extracted"
)]
fn parse_field_multiple_annotations() {
    // Two annotations: @optional (no value) and @doc with a string value
    let source = r#"
type Config {
    limit 16  @optional @doc "Default: 16"
}
"#;
    let result = parse(source, "test.spec");

    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    let entry = entity
        .fields
        .entries()
        .iter()
        .find(|e| e.key == "limit")
        .expect("missing 'limit' field");
    assert_eq!(
        entry.annotations.len(),
        2,
        "expected 2 annotations, got: {:?}",
        entry.annotations
    );
    assert_eq!(
        entry.annotations[0],
        Annotation {
            name: "optional".to_string(),
            value: None
        }
    );
    assert_eq!(
        entry.annotations[1],
        Annotation {
            name: "doc".to_string(),
            value: Some("Default: 16".to_string())
        }
    );
}

// ============================================================================
// Mixed-type list parsing (L4)
// ============================================================================

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "mixed-type list preserves per-item types"
)]
fn parse_mixed_list_preserves_item_types() {
    let source = r#"
behavior mixed_test "Mixed Test" {
    items ["hello", 42, true]
}
"#;
    let result = parse(source, "test.spec");

    // Mixed lists produce a warning but should still parse
    let entity = &result.entities[0];
    let items = entity.fields.get("items").expect("missing 'items' field");
    match items {
        FieldValue::MixedList(values) => {
            assert_eq!(values.len(), 3, "expected 3 items in mixed list");
            assert!(
                matches!(&values[0], FieldValue::String(s) if s == "hello"),
                "first item should be String(\"hello\"), got: {:?}",
                values[0]
            );
            assert!(
                matches!(&values[1], FieldValue::Integer(42)),
                "second item should be Integer(42), got: {:?}",
                values[1]
            );
            assert!(
                matches!(&values[2], FieldValue::Boolean(true)),
                "third item should be Boolean(true), got: {:?}",
                values[2]
            );
        }
        other => panic!("expected MixedList, got: {:?}", other),
    }
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "homogeneous string list is not MixedList"
)]
fn parse_homogeneous_string_list_remains_string_list() {
    let source = r#"
behavior homo_test "Homo Test" {
    tags ["a", "b", "c"]
}
"#;
    let result = parse(source, "test.spec");
    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    let tags = entity.fields.get("tags").expect("missing 'tags' field");
    assert!(
        matches!(tags, FieldValue::StringList(_)),
        "expected StringList, got: {:?}",
        tags
    );
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "homogeneous reference list is not MixedList"
)]
fn parse_homogeneous_ref_list_remains_ref_list() {
    let source = r#"
behavior ref_test "Ref Test" {
    features [auth_login, auth_logout]
}
"#;
    let result = parse(source, "test.spec");
    assert!(
        result.errors.is_empty(),
        "unexpected errors: {:?}",
        result.errors
    );
    let entity = &result.entities[0];
    let features = entity
        .fields
        .get("features")
        .expect("missing 'features' field");
    assert!(
        matches!(features, FieldValue::ReferenceList(_)),
        "expected ReferenceList, got: {:?}",
        features
    );
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "mixed list with strings and integers preserves both"
)]
fn parse_mixed_list_strings_and_integers() {
    let source = r#"
behavior mix_si "Mix String Int" {
    values ["alpha", 99]
}
"#;
    let result = parse(source, "test.spec");
    let entity = &result.entities[0];
    let values = entity.fields.get("values").expect("missing 'values' field");
    match values {
        FieldValue::MixedList(items) => {
            assert_eq!(items.len(), 2);
            assert!(matches!(&items[0], FieldValue::String(s) if s == "alpha"));
            assert!(matches!(&items[1], FieldValue::Integer(99)));
        }
        other => panic!("expected MixedList, got: {:?}", other),
    }
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "recovery resumes at every top-level form the grammar reads, a union block included"
)]
fn a_block_after_an_unclosed_string_survives_in_every_top_level_form() {
    let source = "behavior a \"A\" {\n  contract \"oops\n}\n\ntype status = \"open\" | \"closed\"\n\nbehavior b \"B\" {\n  contract \"fine\"\n}\n";
    let result = parse(source, "test.spec");
    let ids: Vec<&str> = result.entities.iter().map(|e| e.id.raw.as_str()).collect();
    assert!(
        ids.contains(&"status") && ids.contains(&"b"),
        "entities: {ids:?}, errors: {:?}",
        result.errors
    );
    assert_eq!(result.errors.len(), 1, "errors: {:?}", result.errors);
    assert!(
        result.errors[0]
            .message
            .contains("before the block on line 5"),
        "{}",
        result.errors[0].message
    );
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "the blocks after an unclosed body are parsed, and its missing closing brace is reported"
)]
fn the_blocks_after_an_unclosed_body_are_parsed() {
    let source = "behavior a \"A\" {\n  status done\n\ntype status = open | closed\n\nbehavior b \"B\" {\n  status done\n}\n";
    let result = parse(source, "test.spec");
    let ids: Vec<&str> = result.entities.iter().map(|e| e.id.raw.as_str()).collect();
    assert_eq!(ids, ["a", "status", "b"], "errors: {:?}", result.errors);
    assert!(result.entities[0].fields.get("status").is_some());
    assert_eq!(result.errors.len(), 1, "errors: {:?}", result.errors);
    let error = &result.errors[0];
    assert_eq!(
        error.message,
        "syntax error: unclosed block — missing closing '}'; it was ended before the block on line 4"
    );
    assert_eq!((error.span.start_line, error.span.start_col), (1, 16));
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "an unclosed body is reported at its opening brace, naming the line where it was ended"
)]
fn an_unclosed_body_at_the_end_names_no_line() {
    let result = parse("behavior a \"A\" {\n  status done\n", "test.spec");
    assert_eq!(result.errors.len(), 1, "errors: {:?}", result.errors);
    let error = &result.errors[0];
    assert_eq!(
        error.message,
        "syntax error: unclosed block — missing closing '}'"
    );
    assert_eq!((error.span.start_line, error.span.start_col), (1, 16));
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "an unclosed body is reported at its opening brace, naming the line where it was ended"
)]
fn an_unclosed_string_takes_its_bodys_error() {
    let source = "port pay \"Pay\" {\n  direction inbound\n  description \"\"\"Takes payments\n  method charge(amount: String) -> String\n\nbehavior settle \"Settle\" {\n  contract \"Settles.\"\n}\n";
    let result = parse(source, "test.spec");
    let ids: Vec<&str> = result.entities.iter().map(|e| e.id.raw.as_str()).collect();
    assert!(ids.contains(&"settle"), "entities: {ids:?}");
    assert_eq!(result.errors.len(), 1, "errors: {:?}", result.errors);
    assert!(
        result.errors[0]
            .message
            .contains("unclosed triple-quoted string"),
        "{}",
        result.errors[0].message
    );
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "a backslash before a line break leaves a string unclosed, as the grammar reads it"
)]
fn a_backslash_before_a_line_break_leaves_the_string_unclosed() {
    let source = "behavior a \"A\" {\n  contract \"line one \\\n  line two\"\n}\n\nbehavior b \"B\" {\n  contract \"fine\"\n}\n";
    let result = parse(source, "test.spec");
    assert!(
        result.entities.iter().any(|e| e.id.raw == "b"),
        "entities: {:?}, errors: {:?}",
        result.entities,
        result.errors
    );
    assert_eq!(result.errors.len(), 1, "errors: {:?}", result.errors);
    let error = &result.errors[0];
    assert_eq!(
        (error.span.start_line, error.span.start_col),
        (2, 12),
        "{error:?}"
    );
    assert!(
        error.message.contains("unclosed string"),
        "{}",
        error.message
    );
}

/// A body left unclosed while a field's name is being typed, then two blocks.
const HALF_TYPED_FIELD: &str = "behavior a1 \"A\" {\n  contract \"x\"\n  st\n\nbehavior b1 \"B\" {\n  contract \"y\"\n}\n\nbehavior c1 \"C\" {\n  contract \"z\"\n}\n";

/// A method header typed without its parameter list, then a block.
const HALF_TYPED_METHOD: &str = "port pay \"Pay\" {\n  direction inbound\n  method charge\n\nbehavior settle \"Settle\" {\n  contract \"Settles.\"\n}\n";

/// A parameter list left open, then a block.
const OPEN_PARAMETER_LIST: &str = "port pay \"Pay\" {\n  direction inbound\n  method charge(amount: String\n\nbehavior settle \"Settle\" {\n  contract \"Settles.\"\n}\n";

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "a body member left half-typed ends where the next top-level form starts"
)]
fn a_half_typed_field_ends_its_body_at_the_next_form() {
    let result = parse(HALF_TYPED_FIELD, "test.spec");
    let ids: Vec<&str> = result.entities.iter().map(|e| e.id.raw.as_str()).collect();
    assert_eq!(ids, ["a1", "b1", "c1"], "errors: {:?}", result.errors);
    assert_eq!(result.errors.len(), 1, "errors: {:?}", result.errors);
    let error = &result.errors[0];
    assert_eq!(
        error.message,
        "syntax error: unclosed block — missing closing '}'; it was ended before the block on line 5"
    );
    assert_eq!((error.span.start_line, error.span.start_col), (1, 17));
}

#[specforge_test(
    behavior = "recover_from_syntax_errors",
    verify = "the parser resumes at every form start the grammar's tree missed"
)]
fn a_half_typed_method_header_ends_its_body_at_the_next_form() {
    let result = parse(HALF_TYPED_METHOD, "test.spec");
    let ids: Vec<&str> = result.entities.iter().map(|e| e.id.raw.as_str()).collect();
    // `pay` itself is lost: the grammar reads no entity from a method header
    // cut short (README section 5, R15).
    assert!(ids.contains(&"settle"), "entities: {ids:?}");
    assert!(
        result.errors.iter().any(|e| e.span.start_line == 1),
        "errors: {:?}",
        result.errors
    );
}

#[test]
fn an_open_parameter_list_ends_its_body_at_the_next_form() {
    let result = parse(OPEN_PARAMETER_LIST, "test.spec");
    let ids: Vec<&str> = result.entities.iter().map(|e| e.id.raw.as_str()).collect();
    assert_eq!(ids, ["pay", "settle"], "errors: {:?}", result.errors);
}
