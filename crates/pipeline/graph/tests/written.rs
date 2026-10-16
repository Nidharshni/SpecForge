//! Where an edge is written: the span of each reference it was linked from, kept beside the edge so
//! navigation reads a position and never decides it again (ADR 0127).

use crate::derived::{build, derived};
use specforge_common::{SourceSpan, Sym};
use specforge_graph::{Edge, FileChange, Graph, GraphBuild, GraphConfig};
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

fn edge(source: &str, target: &str, label: &str) -> Edge {
    Edge {
        source: Sym::new(source),
        target: Sym::new(target),
        label: Sym::new(label),
    }
}

fn at(span: &SourceSpan) -> String {
    format!(
        "{}:{}-{}:{}",
        span.start_line, span.start_col, span.end_line, span.end_col
    )
}

/// Each place `source -label-> target` is written, as `"L:C-L:C"`.
fn places(graph: &Graph, source: &str, target: &str, label: &str) -> Vec<String> {
    graph
        .written(&edge(source, target, label))
        .iter()
        .map(at)
        .collect()
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "an edge records the span of each reference it was linked from"
)]
fn a_list_items_edge_is_written_at_the_item() {
    let source = "shape Wheel {\n  size number\n}\nshape Car {\n  parts [Wheel]\n}\n";
    let (graph, _) = build(source, derived());

    assert_eq!(places(&graph, "Car", "Wheel", "parts"), ["5:10-5:15"]);
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "an edge records the span of each reference it was linked from"
)]
fn a_single_reference_is_written_at_its_value() {
    let source = "shape Base {\n  id string\n}\nshape Note {\n  base Base\n}\n";
    let (graph, _) = build(source, derived());

    assert_eq!(places(&graph, "Note", "Base", "base"), ["5:8-5:12"]);
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "an edge records the span of each reference it was linked from"
)]
fn an_edge_written_twice_keeps_both_places_in_written_order() {
    let source = "shape Wheel {\n  size number\n}\nshape Car {\n  parts [Wheel, Wheel]\n}\n";
    let (graph, _) = build(source, derived());

    assert_eq!(graph.edges_from("Car").len(), 1);
    assert_eq!(
        places(&graph, "Car", "Wheel", "parts"),
        ["5:10-5:15", "5:17-5:22"]
    );
}

#[test]
fn an_edge_added_by_hand_is_written_nowhere() {
    let mut graph = Graph::new();
    graph.add_edge(edge("a", "b", "uses"));

    assert!(graph.written(&edge("a", "b", "uses")).is_empty());
    assert!(graph.written(&edge("a", "c", "uses")).is_empty());
}

#[test]
fn a_subgraph_keeps_where_its_edges_are_written() {
    let source = "shape Wheel {\n  size number\n}\nshape Car {\n  parts [Wheel]\n}\n";
    let (graph, _) = build(source, derived());

    let sub = graph.subgraph("Car").unwrap();

    assert_eq!(places(&sub, "Car", "Wheel", "parts"), ["5:10-5:15"]);
}

#[specforge_test(
    behavior = "build_in_memory_graph",
    verify = "an incremental build records what a cold build records"
)]
fn an_incremental_build_records_what_a_cold_build_records() {
    let limit = "invariant inv_one \"One\" {\n  contract \"must hold\"\n}\n";
    let holder = "behavior alpha \"A\" {\n  contract \"first\"\n  invariants [inv_one]\n}\n";
    let mut build = GraphBuild::of(
        [parse(limit, "a.spec"), parse(holder, "b.spec")],
        GraphConfig::default(),
    );
    build.set_verify(true);
    let place = |build: &GraphBuild| places(build.graph(), "alpha", "inv_one", "invariants");
    assert_eq!(place(&build), ["3:15-3:22"]);

    // The holder moves down two lines: its spans are recomputed, never stale.
    let moved = format!("\n\n{holder}");
    let applied = build.apply([FileChange::Parsed(parse(&moved, "b.spec"))]);

    assert_eq!(applied.verification, Some(Ok(())));
    assert_eq!(place(&build), ["5:15-5:22"]);
}

/// The delta compares methods through their serialized form, which has no positions.
#[test]
fn a_methods_positions_are_not_serialized() {
    let file = parse("port P {\n  method run(c: Config) -> Out\n}\n", "p.spec");
    let method = &file.entities[0].methods[0];
    assert!(method.params[0].ty_span.is_some());

    let json = serde_json::to_value(method).unwrap().to_string();

    assert!(
        !json.contains("ty_span") && !json.contains("returns_span"),
        "{json}"
    );
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a derived edge is written at each name it was derived from"
)]
fn a_derived_edge_is_written_at_each_name() {
    let source = r#"
shape Config {
  root string
}
shape Project {
  name string
}
shape Report {
  ok boolean
}
iface Detector {
  method detect(config: Config) -> Result<Project, Error>
  method report(items: Project[]) -> Report
}
"#;
    let (graph, _) = build(source, derived());

    assert_eq!(
        places(&graph, "Detector", "Project", "uses"),
        ["12:43-12:50", "13:24-13:31"]
    );
    assert_eq!(
        places(&graph, "Detector", "Config", "uses"),
        ["12:25-12:31"]
    );
    assert_eq!(
        places(&graph, "Detector", "Report", "uses"),
        ["13:38-13:44"]
    );
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a derived edge is written at each name it was derived from"
)]
fn a_parameters_name_is_not_where_its_edge_is_written() {
    let source =
        "shape Config {\n  root string\n}\niface X {\n  method keep(Config: Config) -> Config\n}\n";
    let (graph, _) = build(source, derived());

    assert_eq!(
        places(&graph, "X", "Config", "uses"),
        ["5:23-5:29", "5:34-5:40"]
    );
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a derived edge is written at each name it was derived from"
)]
fn a_union_member_is_written_at_its_name() {
    let source = "shape Wheel {\n  size number\n}\nshape Config {\n  root string\n}\n\
shape Kind = Wheel | Config\nshape Holder {\n  slot Wheel | Config\n}\n";
    let (graph, _) = build(source, derived());

    assert_eq!(places(&graph, "Kind", "Wheel", "parts"), ["7:14-7:19"]);
    assert_eq!(places(&graph, "Kind", "Config", "parts"), ["7:22-7:28"]);
    assert_eq!(places(&graph, "Holder", "Wheel", "parts"), ["9:8-9:13"]);
    assert_eq!(places(&graph, "Holder", "Config", "parts"), ["9:16-9:22"]);
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a derived edge is written at each name it was derived from"
)]
fn a_name_inside_a_type_on_a_later_line_is_written_on_its_line() {
    let source = "shape Config {\n  root string\n}\nshape Project {\n  name string\n}\n\
iface X {\n  method run() -> Result<\n    Config,\n    Project>\n}\n";
    let (graph, diagnostics) = build(source, derived());

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(places(&graph, "X", "Config", "uses"), ["9:5-9:11"]);
    assert_eq!(places(&graph, "X", "Project", "uses"), ["10:5-10:12"]);
}
