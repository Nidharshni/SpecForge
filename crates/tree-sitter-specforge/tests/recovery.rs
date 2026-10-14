//! An unclosed body ends where the next top-level form starts: the grammar
//! resumes there, as the lexer's form-start rule reads it (ADR 0038 D6), so
//! the blocks after a body being typed stay in the tree.

use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser, Tree};
use tree_sitter_specforge::{LANGUAGE, kind};

fn parse(source: &str) -> Tree {
    let mut parser = Parser::new();
    parser.set_language(&LANGUAGE.into()).unwrap();
    parser.parse(source, None).unwrap()
}

/// The root's named children other than comments, as (kind, first line).
fn top_level(source: &str, tree: &Tree) -> Vec<(String, String)> {
    let root = tree.root_node();
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .filter(|n| n.kind() != kind::COMMENT)
        .map(|n| {
            let text = &source[n.start_byte()..n.end_byte()];
            (
                n.kind().to_string(),
                text.lines().next().unwrap_or("").to_string(),
            )
        })
        .collect()
}

/// Whether `node` holds a MISSING `}`.
fn misses_a_brace(node: Node) -> bool {
    if node.is_missing() && node.kind() == "}" {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(misses_a_brace)
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(k, l)| (k.to_string(), l.to_string()))
        .collect()
}

#[specforge_test_macros::test(
    behavior = "recover_from_syntax_errors",
    verify = "an unclosed body ends where the next top-level form starts, missing its closing brace"
)]
fn an_unclosed_body_ends_at_the_next_form() {
    let source = r#"behavior a "A" {
  status done
  requires {
    ok "fine"
  guard expr { x < 1 }

behavior b "B" {
  status done
}
type status = open | closed
ref github.issue:7 "Seven"
define widget {
  testable true
  tags [x, y
spec "Project" {
  version "1.0"
  check expr { x < 1
use "types/core"
pub use { A } from "types/core"
behavior c {
  status done
"#;
    let tree = parse(source);
    assert_eq!(
        top_level(source, &tree),
        pairs(&[
            ("entity_block", r#"behavior a "A" {"#),
            ("entity_block", r#"behavior b "B" {"#),
            ("union_block", "type status = open | closed"),
            ("ref_block", r#"ref github.issue:7 "Seven""#),
            ("define_block", "define widget {"),
            ("spec_block", r#"spec "Project" {"#),
            ("use_import", r#"use "types/core""#),
            ("pub_use_import", r#"pub use { A } from "types/core""#),
            ("entity_block", "behavior c {"),
        ]),
        "tree: {}",
        tree.root_node().to_sexp()
    );
    let root = tree.root_node();
    let mut cursor = root.walk();
    let blocks: Vec<Node> = root.named_children(&mut cursor).collect();
    let unclosed = [0, 4, 5, 8];
    for (i, block) in blocks.iter().enumerate() {
        assert_eq!(
            misses_a_brace(*block),
            unclosed.contains(&i),
            "block {i} ({}): {}",
            block.kind(),
            block.to_sexp()
        );
        assert!(
            !block.has_error() || misses_a_brace(*block),
            "{}",
            block.to_sexp()
        );
    }
    // The list and the expression group left open end with their bodies.
    assert!(
        blocks[4]
            .to_sexp()
            .contains("(list (identifier) (identifier))")
    );
    assert!(blocks[5].to_sexp().contains("(expr_group"));
    let sexp = tree.root_node().to_sexp();
    assert!(!sexp.contains("ERROR"), "{sexp}");
    // A form-shaped line indented deeper than its form is one of its fields.
    let a = &source[blocks[0].start_byte()..blocks[0].end_byte()];
    assert!(a.contains("guard expr { x < 1 }"), "{a}");
}

#[specforge_test_macros::test(
    behavior = "recover_from_syntax_errors",
    verify = "a body member left half-typed ends where the next top-level form starts"
)]
fn a_half_typed_member_ends_at_the_next_form() {
    for member in [
        "  st",                                             // a field's name, no value
        "  method charge(amount: String",                   // an open parameter list
        "  method charge(amount: String) -> Result<String", // an open generic
        "  method on(cb: fn(String",                        // an open function type
        "  guard expr { (latency < 10ms",                   // an open parenthesis
    ] {
        let source = format!(
            "port p \"P\" {{\n  direction inbound\n{member}\n\nbehavior b \"B\" {{\n  status done\n}}\n"
        );
        let tree = parse(&source);
        assert_eq!(
            top_level(&source, &tree),
            pairs(&[
                ("entity_block", "port p \"P\" {"),
                ("entity_block", "behavior b \"B\" {"),
            ]),
            "{member}: {}",
            tree.root_node().to_sexp()
        );
        let root = tree.root_node();
        let mut cursor = root.walk();
        let blocks: Vec<Node> = root.named_children(&mut cursor).collect();
        assert!(
            misses_a_brace(blocks[0]),
            "{member}: the body reports its error"
        );
        assert!(!blocks[1].has_error(), "{member}");
    }
}

#[specforge_test_macros::test(
    behavior = "recover_from_syntax_errors",
    verify = "an unclosed body ends at a form indented no deeper than the form it opened"
)]
fn indented_forms_end_an_unclosed_body() {
    let source = "  behavior a \"A\" {\n    status done\n    guard expr { x < 1 }\n  behavior b \"B\" {\n    status done\n  }\n";
    let tree = parse(source);
    assert_eq!(
        top_level(source, &tree),
        pairs(&[
            ("entity_block", "behavior a \"A\" {"),
            ("entity_block", "behavior b \"B\" {"),
        ]),
        "tree: {}",
        tree.root_node().to_sexp()
    );
}

#[specforge_test_macros::test(
    behavior = "recover_from_syntax_errors",
    verify = "the grammar resumes at every top-level form of the repository's spec after an unclosed body"
)]
fn the_grammar_resumes_after_every_unclosed_body_of_the_corpora() {
    let root = specforge_repo_root::repo_root();
    let mut files = Vec::new();
    for corpus in ["spec", "integrations/rust/spec", "examples"] {
        spec_files(&root.join(corpus), &mut files);
    }
    assert!(files.len() > 150, "found only {} spec files", files.len());
    let mut lost = Vec::new();
    let mut bodies = 0;
    for file in files {
        let source = std::fs::read_to_string(&file).unwrap();
        let tree = parse(&source);
        assert!(
            !tree.root_node().has_error(),
            "{}: does not parse",
            file.display()
        );
        let expected = top_level(&source, &tree);
        let root_node = tree.root_node();
        let mut cursor = root_node.walk();
        let closes: Vec<usize> = root_node
            .named_children(&mut cursor)
            .filter_map(closing_brace)
            .collect();
        for close in closes {
            bodies += 1;
            let mut unclosed = source.clone();
            unclosed.replace_range(close..close + 1, " ");
            let reparsed = parse(&unclosed);
            let got = top_level(&unclosed, &reparsed);
            if got != expected {
                let line = source[..close].matches('\n').count() + 1;
                lost.push(format!(
                    "{}:{line}: expected {} top-level forms, got {}",
                    file.display(),
                    expected.len(),
                    got.len()
                ));
            }
        }
    }
    assert!(bodies > 1000, "found only {bodies} bodies");
    assert!(
        lost.is_empty(),
        "forms lost after an unclosed body: {lost:#?}"
    );
}

#[specforge_test_macros::test(
    behavior = "recover_from_syntax_errors",
    verify = "a body member left half-typed ends where the next top-level form starts"
)]
fn the_grammar_resumes_after_every_half_typed_body_of_the_corpora() {
    let root = specforge_repo_root::repo_root();
    let mut files = Vec::new();
    for corpus in ["spec", "integrations/rust/spec", "examples"] {
        spec_files(&root.join(corpus), &mut files);
    }
    let mut lost = Vec::new();
    let mut half_typed = 0;
    for file in files {
        let source = std::fs::read_to_string(&file).unwrap();
        let tree = parse(&source);
        let expected = form_starts(&tree);
        let root_node = tree.root_node();
        let mut cursor = root_node.walk();
        for form in root_node.named_children(&mut cursor) {
            let Some((value_from, field_to, close)) = last_field_and_close(form) else {
                continue;
            };
            // `_close` ends the field only before a form line: a form that
            // ends the file has none to end it at.
            let mut next = form.next_named_sibling();
            while next.is_some_and(|n| n.kind() == kind::COMMENT) {
                next = next.and_then(|n| n.next_named_sibling());
            }
            if next.is_none() {
                continue;
            }
            half_typed += 1;
            // The last field loses its value (and annotations), the body its
            // `}`: bytes become spaces, so offsets and lines stay.
            let mut bytes = source.clone().into_bytes();
            for byte in &mut bytes[value_from..field_to] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
            bytes[close] = b' ';
            let broken = String::from_utf8(bytes).unwrap();
            let got = form_starts(&parse(&broken));
            if got != expected {
                let line = source[..value_from].matches('\n').count() + 1;
                lost.push(format!(
                    "{}:{line}: expected {} top-level forms, got {}",
                    file.display(),
                    expected.len(),
                    got.len()
                ));
            }
        }
    }
    assert!(
        half_typed > 300,
        "found only {half_typed} bodies ending in a field"
    );
    assert!(
        lost.is_empty(),
        "forms lost after a half-typed field: {lost:#?}"
    );
}

/// The root's named children other than comments, as (kind, start byte).
fn form_starts(tree: &Tree) -> Vec<(String, usize)> {
    let root = tree.root_node();
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .filter(|n| n.kind() != kind::COMMENT)
        .map(|n| (n.kind().to_string(), n.start_byte()))
        .collect()
}

/// For a top-level entity block whose last member is a field with a value:
/// the byte where that value starts, where the field ends, and the body's
/// closing `}`.
fn last_field_and_close(node: Node) -> Option<(usize, usize, usize)> {
    if node.kind() != kind::ENTITY_BLOCK {
        return None;
    }
    let close = closing_brace(node)?;
    let mut cursor = node.walk();
    let last = node
        .named_children(&mut cursor)
        .filter(|n| n.kind() != kind::COMMENT)
        .last()?;
    if last.kind() != kind::FIELD {
        return None;
    }
    let value = last.child_by_field_name(tree_sitter_specforge::field::VALUE)?;
    Some((value.start_byte(), last.end_byte(), close))
}

/// The byte offset of a top-level block's closing `}`, for a block with a body.
fn closing_brace(node: Node) -> Option<usize> {
    let block = match node.kind() {
        kind::REF_BLOCK => node.named_child(0)?,
        _ => node,
    };
    match block.kind() {
        kind::ENTITY_BLOCK | kind::SPEC_BLOCK | kind::DEFINE_BLOCK | kind::REF_FULL => {
            let last = block.child(u32::try_from(block.child_count().checked_sub(1)?).ok()?)?;
            (last.kind() == "}").then(|| last.start_byte())
        }
        _ => None,
    }
}

fn spec_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            spec_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "spec") {
            out.push(path);
        }
    }
}
