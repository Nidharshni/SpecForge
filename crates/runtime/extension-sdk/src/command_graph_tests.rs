use super::*;

#[test]
fn a_command_input_carries_its_args_and_graph() {
    let input: CommandInput = serde_json::from_value(serde_json::json!({
        "args": {"status": "done", "limit": 2, "offset": "1", "all": true},
        "cwd": "/p",
        "graph": {
            "format_version": "1.0",
            "nodes": [
                {"id": "a", "kind": "k", "title": "A", "file": "x.spec", "line": 1,
                 "fields": {"status": "done", "refs": ["b"]}},
                {"id": "b", "kind": "k", "file": "x.spec", "line": 2, "fields": {}}
            ],
            "edges": [{"source": "a", "target": "b", "label": "refs"}]
        }
    }))
    .unwrap();
    assert_eq!(input.args["status"], "done");
    assert_eq!(input.args["limit"], 2);
    let a = input.graph.node("a").unwrap();
    assert_eq!(a.text("status"), Some("done"));
    assert_eq!(a.text("refs"), None);
    assert_eq!(a.list("refs"), ["b"]);
    assert_eq!(input.graph.nodes_of_kind("k").count(), 2);
    assert_eq!(input.graph.edges_from("a")[0].target, "b");
    assert_eq!(input.graph.edges_to("b")[0].source, "a");
    assert!(input.graph.edges_to("a").is_empty());
}

#[test]
fn a_command_input_is_built_by_its_fields() {
    // The alias of the protocol's generic input builds and defaults as
    // the SDK's own struct did.
    let input = CommandInput {
        args: serde_json::Map::new(),
        cwd: "/p".to_string(),
        graph: CommandGraph::default(),
        format: CommandFormat::Json,
        today: String::new(),
        evidence: Default::default(),
    };
    assert!(input.is_json());
    assert!(!CommandInput::default().is_json());
}
