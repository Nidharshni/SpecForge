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

#[specforge_test(
    behavior = "require_block_titles",
    verify = "E076 names specforge migrate exactly where migrate writes the title"
)]
fn a_spec_blocks_e076_names_its_name_not_migrate() {
    let (_, diagnostics) = build_graph(&[parse("spec \"\" {\n}\n\ntype X {\n}\n", "a.spec")]);
    let help = |line: usize| {
        diagnostics
            .iter()
            .find(|d| d.code == "E076" && d.span.as_ref().is_some_and(|s| s.start_line == line))
            .and_then(|d| d.suggestion.clone())
            .unwrap_or_default()
    };

    assert!(
        help(1).contains("its name") && !help(1).contains("migrate"),
        "{}",
        help(1)
    );
    assert!(help(4).contains("specforge migrate"), "{}", help(4));
}

#[specforge_test(
    behavior = "require_block_titles",
    verify = "E076 is reported exactly for the blocks the title rule says owe one"
)]
fn owes_title_is_exactly_e076s_subject() {
    let text = "type titled \"T\" {\n}\n\ntype missing {\n}\n\ntype blank \"\" {\n}\n\ntype spaces \"  \" {\n}\n\n\
        type untitled_union = a | b\n\ntype titled_union \"U\" = a | b\n\nspec \"s\" {\n}\n\nspec \"\" {\n}\n\n\
        ref github.issue:org/repo#1 \"R\"\n\nref github.issue:org/repo#2 \"\"\n\ndefine d {\n}\n\n\
        defaults behavior {\n  category query\n}\n";
    let file = parse(text, "a.spec");
    let (_, diagnostics) = build_graph(std::slice::from_ref(&file));
    let reported: std::collections::BTreeSet<usize> = diagnostics
        .iter()
        .filter(|d| d.code == "E076")
        .filter_map(|d| d.span.as_ref().map(|s| s.start_line))
        .collect();
    let owing: std::collections::BTreeSet<usize> = file
        .entities
        .iter()
        .filter(|e| specforge_graph::title::owes_title(e))
        .map(|e| e.span.start_line)
        .collect();

    assert!(!owing.is_empty());
    assert_eq!(owing, reported, "{diagnostics:?}");
}
