//! Type syntax the spec corpus uses, and a sweep proving every `.spec` in
//! the repository parses without a syntax error. Tree-sitter recovers from
//! errors silently, so a gap in the grammar used to drop fields and methods
//! from the graph without a diagnostic.

use specforge_parser::{FieldValue, parse, parse_incremental};
use specforge_test_macros::test as specforge_test;
use std::path::{Path, PathBuf};

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "optional and annotated parameters, nested arrays, unit and function types, and string-literal unions parse"
)]
fn type_syntax_used_by_the_corpus_parses() {
    let source = r#"
type t "T" {
  cycles integer[][] @optional
  status "ok" | "warn" | "error"
  mixed  string | "none"
}
port p "P" {
  method a(x?: string, y: Id @optional) -> Result<(), string>
  method b(h: fn(), g: fn(string, i32) -> bool) -> Result<float[][], E>
}
"#;
    let file = parse(source, "t.spec");
    assert!(file.errors.is_empty(), "{:?}", file.errors);
    let t = &file.entities[0];
    let field = |key: &str| {
        t.fields
            .entries()
            .iter()
            .find(|e| e.key.as_str() == key)
            .unwrap_or_else(|| panic!("field {key}"))
    };
    assert!(matches!(&field("cycles").value, FieldValue::Identifier(v) if v == "integer[][]"));
    assert!(
        matches!(&field("status").value, FieldValue::TypeUnion(v) if v == &["\"ok\"", "\"warn\"", "\"error\""]),
        "{:?}",
        field("status").value
    );
    assert!(
        matches!(&field("mixed").value, FieldValue::TypeUnion(v) if v == &["string", "\"none\""])
    );

    let port = &file.entities[1];
    let a = &port.methods[0];
    assert!(a.params[0].optional && !a.params[1].optional);
    assert_eq!(a.params[1].annotations[0].name, "optional");
    assert_eq!(a.returns.as_deref(), Some("Result<(), string>"));
    let b = &port.methods[1];
    assert_eq!(b.params[0].ty, "fn()");
    assert_eq!(b.params[1].ty, "fn(string, i32) -> bool");
    assert_eq!(b.returns.as_deref(), Some("Result<float[][], E>"));
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "a type may declare a field named verify next to verify statements"
)]
fn a_field_named_verify_is_not_a_verify_statement() {
    let source = r#"
type result "R" {
  verify string @optional
  verify unit "with a kind"
  verify "bare"
}
"#;
    let file = parse(source, "r.spec");
    assert!(file.errors.is_empty(), "{:?}", file.errors);
    let entity = &file.entities[0];
    let keys: Vec<&str> = entity
        .fields
        .entries()
        .iter()
        .map(|e| e.key.as_str())
        .collect();
    assert!(keys.contains(&"verify"), "{keys:?}");
    let statements = entity
        .fields
        .entries()
        .iter()
        .filter_map(|e| match &e.value {
            FieldValue::VerifyList(list) => Some(list.len()),
            _ => None,
        })
        .sum::<usize>();
    assert_eq!(statements, 2);
    let kinds: Vec<&str> = entity.obligations.iter().map(|o| o.kind.as_str()).collect();
    assert_eq!(entity.obligations.len(), 2);
    assert_eq!(kinds, ["unit", ""]);
}

fn spec_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            spec_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "spec") {
            out.push(path);
        }
    }
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "every spec in the repository parses without a syntax error"
)]
fn every_repository_spec_parses_cleanly() {
    let root = specforge_repo_root::repo_root();
    let mut files = Vec::new();
    for corpus in ["spec", "examples", "integrations/rust/spec"] {
        spec_files(&root.join(corpus), &mut files);
    }
    assert!(files.len() > 150, "found only {} spec files", files.len());
    let broken: Vec<String> = files
        .iter()
        .filter_map(|path| {
            let source = std::fs::read_to_string(path).unwrap();
            let (_, tree) = parse_incremental(&source, &path.to_string_lossy(), None);
            let tree = tree?;
            tree.root_node().has_error().then(|| {
                path.strip_prefix(root)
                    .unwrap_or(path)
                    .display()
                    .to_string()
            })
        })
        .collect();
    assert!(broken.is_empty(), "syntax errors in: {broken:#?}");
}

#[specforge_test(
    behavior = "parse_all_block_types",
    verify = "a syntax error inside a method signature is reported"
)]
fn a_malformed_parameter_is_reported() {
    let source = r#"
port p "P" {
  method a(x: string, : Id) -> Result<string, E>
  verify unit "p works"
}
"#;
    let file = parse(source, "p.spec");
    assert!(
        !file.errors.is_empty(),
        "a malformed parameter must not be silent"
    );
    assert_eq!(file.errors[0].span.start_line, 3, "{:?}", file.errors);
}
