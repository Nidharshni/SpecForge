//! Every block that becomes a node has a title: E076 otherwise (ADR 0148).

use specforge_graph::{FileChange, Graph, GraphBuild, GraphConfig, build_graph};
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

fn codes_of(text: &str) -> (Graph, Vec<String>) {
    let (graph, diagnostics) = build_graph(&[parse(text, "a.spec")]);
    let codes = diagnostics.iter().map(|d| d.code.to_string()).collect();
    (graph, codes)
}

#[specforge_test(
    behavior = "require_block_titles",
    verify = "a block with no title or a blank one is E076 and is still placed"
)]
fn a_block_with_no_title_is_e076() {
    let (graph, codes) = codes_of("type X {\n  id string\n}\n");

    assert_eq!(codes, ["E076"]);
    assert!(graph.nodes().iter().any(|n| n.id.raw == "X"));
}

#[specforge_test(
    behavior = "require_block_titles",
    verify = "a block with no title or a blank one is E076 and is still placed"
)]
fn a_blank_title_is_e076() {
    assert_eq!(codes_of("type X \"\" {\n}\n").1, ["E076"]);
    assert_eq!(codes_of("type X \"   \" {\n}\n").1, ["E076"]);
}

#[specforge_test(
    behavior = "require_block_titles",
    verify = "an untitled union is E076"
)]
fn an_untitled_union_is_e076() {
    assert_eq!(codes_of("type S = a | b\n").1, ["E076"]);
    assert!(codes_of("type S \"States\" = a | b\n").1.is_empty());
}

#[specforge_test(
    behavior = "require_block_titles",
    verify = "define, spec and ref blocks are not E076"
)]
fn a_define_block_is_not_e076() {
    assert_eq!(codes_of("define d {\n}\n").1, ["W143"]);
}

#[specforge_test(
    behavior = "require_block_titles",
    verify = "define, spec and ref blocks are not E076"
)]
fn spec_and_ref_are_titled_by_the_grammar() {
    let text = "spec \"shop\" {\n}\n\nref github.issue:org/repo#1 \"Issue\"\n";
    assert!(codes_of(text).1.is_empty());
}

#[specforge_test(
    behavior = "require_block_titles",
    verify = "a block with no title or a blank one is E076 and is still placed"
)]
fn an_untitled_block_is_still_in_the_graph() {
    let (graph, _) = codes_of("type X {\n}\n\ntype Y \"Y\" {\n}\n");

    assert_eq!(graph.nodes().len(), 2);
    assert_eq!(graph.node("X").unwrap().title, None);
}

#[specforge_test(
    behavior = "require_block_titles",
    verify = "E076 needs no extension and follows the file in an incremental build"
)]
fn e076_needs_no_extension_and_follows_the_file_in_an_incremental_build() {
    let mut build = GraphBuild::new(GraphConfig::default());
    build.apply([FileChange::Parsed(parse("type X {\n}\n", "a.spec"))]);
    assert_eq!(build.diagnostics().len(), 1, "{:?}", build.diagnostics());

    build.apply([FileChange::Parsed(parse("type X \"X\" {\n}\n", "a.spec"))]);
    assert!(build.diagnostics().is_empty(), "{:?}", build.diagnostics());
}
