//! What an entity holds is written once (ADR 0222): the delta reports a changed annotation as its
//! field's change and a method edit as the methods aspect, at any depth.

use specforge_graph::{
    GraphDelta, Members, NodeAspect, build_graph, compute_graph_delta, field_form::write_fields,
};
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

fn delta(before: &str, after: &str) -> GraphDelta {
    let graph = |text| build_graph(&[parse(text, "a.spec")]).0;
    compute_graph_delta(&graph(before), &graph(after))
}

fn only_change(delta: &GraphDelta) -> (&[String], &[NodeAspect]) {
    assert_eq!(delta.modified_nodes.len(), 1, "{delta:?}");
    let modified = &delta.modified_nodes[0];
    (&modified.changed_fields, &modified.changed_aspects)
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "a changed annotation is a changed field, never an aspect"
)]
fn a_changed_annotation_is_a_changed_field() {
    let order = |id: &str| format!("type Order \"O\" {{\n  id string{id}\n}}\n");
    let added = delta(&order(""), &order(" @optional"));
    assert_eq!(only_change(&added), (&["id".to_string()][..], &[][..]));
    let valued = delta(&order(" @since \"1.2\""), &order(" @since \"1.3\""));
    assert_eq!(only_change(&valued), (&["id".to_string()][..], &[][..]));
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "a method edit is the methods aspect, not a changed field"
)]
fn a_method_edit_is_the_methods_aspect() {
    let port =
        |method: &str| format!("port p \"P\" {{\n  direction outbound\n  method {method}\n}}\n");
    let base = port("save(o: string) -> string");
    for edited in [
        port("save(o: string, force?: bool) -> string"),
        port("save(o: string @optional) -> string"),
    ] {
        let d = delta(&base, &edited);
        let (fields, aspects) = only_change(&d);
        assert!(fields.is_empty(), "{d:?}");
        assert!(aspects.contains(&NodeAspect::Methods), "{d:?}");
    }
}

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "an annotation changed inside a block field is a change of the block field"
)]
fn a_block_entrys_changed_annotation_is_a_changed_field() {
    let order = |note: &str| {
        format!("type Order \"O\" {{\n  address {{\n    street string{note}\n  }}\n}}\n")
    };
    let d = delta(&order(""), &order(" @optional"));
    assert_eq!(only_change(&d).0, ["address".to_string()]);
}

const HELD: &str = r#"type Order "O" {
  id string @optional
  id integer
  note string @since "1.2"
  address {
    street string @optional
  }
}
port p "P" {
  direction outbound
  method save(order: Order @optional, force?: bool) -> Result<void, never>
}
"#;

#[specforge_test(
    behavior = "serialize_json_graph",
    verify = "what an entity holds is written once: its fields, their annotations at any depth and its methods, with no position"
)]
fn members_are_written_without_positions() {
    let entities = parse(HELD, "a.spec").entities;
    let order = Members::new(&entities[0].fields, &entities[0].methods);
    let port = Members::new(&entities[1].fields, &entities[1].methods);

    let annotations = serde_json::to_string(&order.annotations).unwrap();
    let methods = serde_json::to_string(&port.methods).unwrap();
    for position in ["span", "start_line", "file"] {
        assert!(!annotations.contains(position) && !methods.contains(position));
    }
    assert_eq!(order.fields, write_fields(&entities[0].fields));
    let paths: Vec<&str> = order.annotations.keys().map(String::as_str).collect();
    assert_eq!(paths, ["address.street", "id", "note"]);
    // A key written twice is its first entry.
    assert_eq!(
        serde_json::to_string(&order.fields["id"]).unwrap(),
        "\"string\""
    );
    assert_eq!(order.annotations["id"].len(), 1);
    assert_eq!(
        port.methods[0].signature(),
        "save(order: Order @optional, force?: bool) -> Result<void, never>"
    );
}
