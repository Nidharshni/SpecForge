use tree_sitter::{QueryError, StreamingIterator};

fn load_query(scm: &str) -> Result<tree_sitter::Query, QueryError> {
    let language = tree_sitter_specforge::LANGUAGE.into();
    tree_sitter::Query::new(&language, scm)
}

#[test]
fn highlights_scm_loads_without_error() {
    let scm = include_str!("../queries/highlights.scm");
    load_query(scm).expect("highlights.scm failed to load");
}

#[test]
fn folds_scm_loads_without_error() {
    let scm = include_str!("../queries/folds.scm");
    load_query(scm).expect("folds.scm failed to load");
}

#[test]
fn indents_scm_loads_without_error() {
    let scm = include_str!("../queries/indents.scm");
    load_query(scm).expect("indents.scm failed to load");
}

#[test]
fn highlights_captures_entity_kind_as_keyword() {
    let scm = include_str!("../queries/highlights.scm");
    let language = tree_sitter_specforge::LANGUAGE.into();
    let query = tree_sitter::Query::new(&language, scm).unwrap();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let source = r#"behavior foo "T" { status "ok" }"#;
    let tree = parser.parse(source, None).unwrap();

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut capture_names: Vec<&str> = Vec::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        for c in m.captures {
            capture_names.push(query.capture_names()[c.index as usize]);
        }
    }

    assert!(
        capture_names.contains(&"keyword"),
        "entity kind should be captured as @keyword, got: {:?}",
        capture_names
    );
    assert!(
        capture_names.contains(&"constant"),
        "entity name should be captured as @constant"
    );
    assert!(
        capture_names.contains(&"string"),
        "strings should be captured as @string"
    );
}

/// Helper: run highlights.scm against source and return all (capture_name, matched_text) pairs.
fn highlight_captures(source: &str) -> Vec<(String, String)> {
    let scm = include_str!("../queries/highlights.scm");
    let language = tree_sitter_specforge::LANGUAGE.into();
    let query = tree_sitter::Query::new(&language, scm).unwrap();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let tree = parser.parse(source, None).unwrap();

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut results = Vec::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        for c in m.captures {
            let name = query.capture_names()[c.index as usize].to_string();
            let text = c.node.utf8_text(source.as_bytes()).unwrap().to_string();
            results.push((name, text));
        }
    }
    results
}

/// Helper: run a query file against source and return all (capture_name, node_kind) pairs.
fn query_captures(scm: &str, source: &str) -> Vec<(String, String)> {
    let language = tree_sitter_specforge::LANGUAGE.into();
    let query = tree_sitter::Query::new(&language, scm).unwrap();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let tree = parser.parse(source, None).unwrap();

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut results = Vec::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        for c in m.captures {
            let name = query.capture_names()[c.index as usize].to_string();
            results.push((name, c.node.kind().to_string()));
        }
    }
    results
}

#[test]
fn folds_marks_all_brace_delimited_blocks() {
    let source = r#"
behavior foo "T" {
  requires {
    input_valid "valid"
  }
  tags [alpha, beta]
}

spec "Project" {
  version "1.0"
}

ref gh.issue:42 "Bug" {
  priority "high"
}

define my_kind {
  testable true
}
"#;
    let scm = include_str!("../queries/folds.scm");
    let captures = query_captures(scm, source);

    let fold_kinds: Vec<&str> = captures
        .iter()
        .filter(|(name, _)| name == "fold")
        .map(|(_, kind)| kind.as_str())
        .collect();

    assert!(
        fold_kinds.contains(&"entity_block"),
        "entity_block should be @fold, got: {:?}",
        fold_kinds
    );
    assert!(
        fold_kinds.contains(&"spec_block"),
        "spec_block should be @fold, got: {:?}",
        fold_kinds
    );
    assert!(
        fold_kinds.contains(&"ref_full"),
        "ref_full should be @fold, got: {:?}",
        fold_kinds
    );
    assert!(
        fold_kinds.contains(&"define_block"),
        "define_block should be @fold, got: {:?}",
        fold_kinds
    );
    assert!(
        fold_kinds.contains(&"nested_block"),
        "nested_block should be @fold, got: {:?}",
        fold_kinds
    );
    assert!(
        fold_kinds.contains(&"list"),
        "list should be @fold, got: {:?}",
        fold_kinds
    );
}

#[test]
fn indents_triggers_on_braces_and_brackets() {
    let source = r#"behavior foo "T" {
  tags [alpha, beta]
}"#;
    let scm = include_str!("../queries/indents.scm");
    let captures = query_captures(scm, source);

    let capture_pairs: Vec<(&str, &str)> = captures
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();

    // Opening brace and bracket → @indent
    assert!(
        capture_pairs.contains(&("indent", "{")),
        "opening brace should be @indent, got: {:?}",
        capture_pairs
    );
    assert!(
        capture_pairs.contains(&("indent", "[")),
        "opening bracket should be @indent, got: {:?}",
        capture_pairs
    );
    // Closing brace and bracket → @dedent
    assert!(
        capture_pairs.contains(&("dedent", "}")),
        "closing brace should be @dedent, got: {:?}",
        capture_pairs
    );
    assert!(
        capture_pairs.contains(&("dedent", "]")),
        "closing bracket should be @dedent, got: {:?}",
        capture_pairs
    );
}

#[test]
fn highlights_union_block_captures() {
    let source = "type FieldValue = StringValue | ReferenceList | Block\n";
    let captures = highlight_captures(source);

    let keywords: Vec<&str> = captures
        .iter()
        .filter(|(n, _)| n == "keyword")
        .map(|(_, t)| t.as_str())
        .collect();
    let constants: Vec<&str> = captures
        .iter()
        .filter(|(n, _)| n == "constant")
        .map(|(_, t)| t.as_str())
        .collect();
    let types: Vec<&str> = captures
        .iter()
        .filter(|(n, _)| n == "type")
        .map(|(_, t)| t.as_str())
        .collect();

    assert!(
        keywords.contains(&"type"),
        "union kind 'type' should be @keyword, got: {keywords:?}"
    );
    assert!(
        constants.contains(&"FieldValue"),
        "union name should be @constant, got: {constants:?}"
    );
    assert!(
        types.contains(&"StringValue"),
        "variant should be @type, got: {types:?}"
    );
    assert!(
        types.contains(&"ReferenceList"),
        "variant should be @type, got: {types:?}"
    );
    assert!(
        types.contains(&"Block"),
        "variant should be @type, got: {types:?}"
    );
}

#[test]
fn folds_ref_inline_not_foldable_ref_full_is() {
    let source = r#"
ref gh.issue:1 "Inline ref"

ref gh.issue:2 "Full ref" {
    priority "high"
}
"#;
    let scm = include_str!("../queries/folds.scm");
    let captures = query_captures(scm, source);

    let fold_kinds: Vec<&str> = captures
        .iter()
        .filter(|(name, _)| name == "fold")
        .map(|(_, kind)| kind.as_str())
        .collect();

    // ref_full should be foldable
    assert!(
        fold_kinds.contains(&"ref_full"),
        "ref_full should be @fold, got: {fold_kinds:?}"
    );
    // ref_inline has no braces — should NOT appear as foldable
    assert!(
        !fold_kinds.contains(&"ref_inline"),
        "ref_inline should NOT be @fold (no braces), got: {fold_kinds:?}"
    );
}

#[test]
fn highlights_captures_import_path_as_string() {
    let source = "use \"behaviors/parsing\"\nuse \"types/core\"\n";
    let captures = highlight_captures(source);
    let strings: Vec<&str> = captures
        .iter()
        .filter(|(name, text)| name == "string" && text.contains('/'))
        .map(|(_, text)| text.as_str())
        .collect();

    assert_eq!(
        strings.len(),
        2,
        "expected 2 import paths as @string, got: {strings:?}"
    );
    assert!(
        strings.contains(&"\"behaviors/parsing\""),
        "missing behaviors/parsing, got: {strings:?}"
    );
    assert!(
        strings.contains(&"\"types/core\""),
        "missing types/core, got: {strings:?}"
    );
}

#[test]
fn highlights_captures_from_and_as_keywords() {
    let source = "use { Foo } from \"types/core\"\nuse * as ns from \"types/core\"\n";
    let captures = highlight_captures(source);
    let keywords: Vec<&str> = captures
        .iter()
        .filter(|(name, _)| name == "keyword")
        .map(|(_, text)| text.as_str())
        .collect();

    assert!(
        keywords.contains(&"from"),
        "from should be @keyword, got: {keywords:?}"
    );
    assert!(
        keywords.contains(&"as"),
        "as should be @keyword, got: {keywords:?}"
    );
}

#[test]
fn highlights_captures_annotations_as_attribute() {
    let source = r#"
type MyType {
    name  string  @readonly
    items Item[]  @optional @doc "Some doc"
}
"#;
    let captures = highlight_captures(source);
    let attributes: Vec<&str> = captures
        .iter()
        .filter(|(name, _)| name == "attribute")
        .map(|(_, text)| text.as_str())
        .collect();

    assert!(
        attributes.iter().any(|a| a.starts_with("@readonly")),
        "expected @readonly as @attribute, got: {attributes:?}"
    );
    assert!(
        attributes.iter().any(|a| a.starts_with("@optional")),
        "expected @optional as @attribute, got: {attributes:?}"
    );
    assert!(
        attributes.iter().any(|a| a.starts_with("@doc")),
        "expected @doc as @attribute, got: {attributes:?}"
    );
    assert_eq!(
        attributes.len(),
        3,
        "expected 3 annotations, got: {attributes:?}"
    );
}

#[test]
fn highlights_captures_structural_keywords() {
    // use, spec, ref, define, verify should all be @keyword
    let source = r#"
use "behaviors/parsing"

spec "My Project" {
    version "1.0"
    verify unit "compiles"
}

ref gh.issue:1 "Bug"

define my_kind {
    testable true
}
"#;
    let captures = highlight_captures(source);
    let keywords: Vec<&str> = captures
        .iter()
        .filter(|(name, _)| name == "keyword")
        .map(|(_, text)| text.as_str())
        .collect();

    assert!(
        keywords.contains(&"use"),
        "use should be @keyword, keywords: {keywords:?}"
    );
    assert!(
        keywords.contains(&"spec"),
        "spec should be @keyword, keywords: {keywords:?}"
    );
    assert!(
        keywords.contains(&"ref"),
        "ref should be @keyword, keywords: {keywords:?}"
    );
    assert!(
        keywords.contains(&"define"),
        "define should be @keyword, keywords: {keywords:?}"
    );
    assert!(
        keywords.contains(&"verify"),
        "verify should be @keyword, keywords: {keywords:?}"
    );
}

#[test]
fn parser_handles_consecutive_use_imports_without_error() {
    let source = r#"// Error reporting behaviors
use "events/compilation"
use "invariants/core"
use "invariants/validation"
use "invariants/zero-entity-core"
use "types/core"
use "types/diagnostics"
use "types/zero-entity-core"

behavior format_diagnostics "Format Diagnostics" {
  types [Diagnostic, SourceSpan]
}
"#;
    let language = tree_sitter_specforge::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();

    // No ERROR nodes should exist
    assert!(
        !root.has_error(),
        "consecutive use imports should parse without errors, tree: {}",
        root.to_sexp()
    );

    // Count use_import nodes
    let mut use_count = 0;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "use_import" {
            use_count += 1;
        }
    }
    assert_eq!(use_count, 7, "expected 7 use_import nodes, got {use_count}");
}

#[test]
fn highlights_captures_triple_quoted_string() {
    let source = "behavior foo \"T\" {\n  contract \"\"\"\n    hello\n  \"\"\"\n}";
    let captures = highlight_captures(source);

    let has_triple_string = captures
        .iter()
        .any(|(name, text)| name == "string" && text.contains("hello"));
    assert!(
        has_triple_string,
        "triple-quoted string should be captured as @string, got: {:?}",
        captures
    );
}
