//! A key one block writes twice (ADR 0215): the build reports each later entry (E081) and the node
//! holds the first entry of each key; the finder still reads the later entry's names.

use crate::defaults::{build, forms};
use specforge_graph::{
    FileChange, GraphBuild, GraphConfig, GraphInputs, field_form, written_tokens,
};
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

const REPEATED: &str = "\
port P1 \"P1\" {
}

port P2 \"P2\" {
}

behavior beh \"B\" {
  ports [P1]
  owner \"a\"
  ports [P2]
  owner \"b\"
  meta {
    x \"1\"
    x \"2\"
  }
}
";

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "a field written twice in one block is E081, and the node holds its first entry, at any depth"
)]
#[ignore = "flipped by T3"]
fn a_field_written_twice_is_e081_and_the_node_holds_the_first() {
    let (graph, diagnostics) = build(&[("a.spec", REPEATED)]);
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "E081")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "'ports' is written twice in behavior 'beh'; the first, at line 8, is kept",
            "'owner' is written twice in behavior 'beh'; the first, at line 9, is kept",
            "'meta.x' is written twice in behavior 'beh'; the first, at line 13, is kept",
        ]
    );
    let node = graph.node("beh").unwrap();
    assert_eq!(node.fields.entries().len(), 3);
    let written = field_form::write_fields(&node.fields);
    assert_eq!(
        serde_json::to_string(&written).unwrap(),
        r#"{"meta":{"x":"1"},"owner":"a","ports":["P1"]}"#
    );
    let targets: Vec<String> = graph
        .edges_from("beh")
        .iter()
        .map(|e| e.target.to_string())
        .collect();
    assert_eq!(targets, ["P1"]);
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "a later entry's names stay tokens a rename edits, and link nothing"
)]
#[ignore = "flipped by T3"]
fn a_later_entrys_names_are_tokens_and_link_nothing() {
    let file = parse(REPEATED, "a.spec");
    let inputs = GraphInputs {
        field_forms: forms(),
        ..Default::default()
    };
    let config = GraphConfig {
        inputs: inputs.clone(),
        ..Default::default()
    };
    let (graph, _) = specforge_graph::build_graph_with_config(std::slice::from_ref(&file), &config);
    let tokens = written_tokens(&file, &inputs, |n| graph.node(n).map(|n| n.kind.raw));
    let later: Vec<_> = tokens
        .iter()
        .filter(|t| t.target.as_str() == "P2")
        .collect();
    assert_eq!(later.len(), 1, "{tokens:?}");
    assert_eq!(later[0].field.as_str(), "ports");
    assert_eq!(later[0].span.start_line, 10);
    assert!(graph.edges_from("beh").iter().all(|e| e.target != "P2"));
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "an update reports a field written twice as the cold build does"
)]
#[ignore = "flipped by T3"]
fn an_update_reports_a_field_written_twice_as_the_cold_build_does() {
    let once = "behavior beh \"B\" {\n  owner \"a\"\n}\n";
    let twice = "behavior beh \"B\" {\n  owner \"a\"\n  owner \"b\"\n}\n";
    let mut build = GraphBuild::of([parse(once, "a.spec")], GraphConfig::default());
    build.set_verify(true);
    let applied = build.apply([FileChange::Parsed(parse(twice, "a.spec"))]);
    assert!(matches!(applied.verification, Some(Ok(()))));
    let codes: Vec<_> = build
        .file_diagnostics("a.spec")
        .iter()
        .map(|d| d.code.to_string())
        .collect();
    assert_eq!(codes, ["E081"]);
}
