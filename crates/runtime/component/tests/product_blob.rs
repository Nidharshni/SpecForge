//! What `@specforge/product`'s vendored component proves that its source
//! tests (`extensions/product/src/tests/`) cannot: every command it
//! declares answers through the component, as the host calls it, and the
//! dependency commands walk a long chain within the component's stack.
//! The commands' behaviour is tested with the extension (ADR 0013 D8).

use serde_json::{Map, Value, json};
use specforge_component::{ComponentRuntime, builtins};
use specforge_protocol_types::{
    CommandEvidence, CommandFormat, CommandInput, CommandOutput, RawGraph,
};
use specforge_test::prelude::*;
use specforge_wasm::ExtensionCalls;
use specforge_wasm::protocol::load_declaration;

const PRODUCT: &str = "@specforge/product";

/// The nine kinds a product command can name.
const KINDS: [&str; 9] = [
    "feature",
    "journey",
    "deliverable",
    "milestone",
    "module",
    "term",
    "persona",
    "channel",
    "release",
];

fn runtime() -> ComponentRuntime {
    let runtime = ComponentRuntime::new();
    builtins::load_builtins(&runtime).unwrap();
    runtime
}

fn node(id: &str, kind: &str) -> Value {
    json!({"id": id, "kind": kind, "title": id, "fields": {}})
}

/// `export` over `graph`, as the host runs it: the typed input, the
/// strictly decoded answer (ADR 0013).
fn run(
    runtime: &ComponentRuntime,
    export: &str,
    args: Map<String, Value>,
    graph: &Value,
    format: CommandFormat,
) -> CommandOutput {
    let input = CommandInput {
        args,
        cwd: "/p".into(),
        format,
        today: "2026-10-03".into(),
        graph: RawGraph::new(graph.to_string()).expect("one JSON value"),
        evidence: CommandEvidence::None,
    };
    ExtensionCalls::new(runtime)
        .run_command(PRODUCT, export, &input)
        .unwrap_or_else(|error| panic!("{export}: {error}"))
}

#[specforge_test(
    behavior = "pe_declare_surface_contributions",
    verify = "every declared command answers through the component as the host calls it, over a graph holding every kind"
)]
fn every_product_command_answers_through_the_component_as_the_host_calls_it() {
    let runtime = runtime();
    let commands = load_declaration(&runtime, PRODUCT)
        .unwrap()
        .declaration
        .surfaces
        .commands;
    assert_eq!(commands.len(), 40);
    let nodes: Vec<Value> = KINDS
        .iter()
        .map(|kind| node(&format!("{kind}1"), kind))
        .collect();
    let graph = json!({"nodes": nodes, "edges": []});
    for command in &commands {
        // A required arg is an entity id, named after its kind.
        let args: Map<String, Value> = command
            .args
            .iter()
            .filter(|arg| arg.required)
            .map(|arg| (arg.name.clone(), json!(format!("{}1", arg.name))))
            .collect();
        let out = run(
            &runtime,
            &command.export,
            args.clone(),
            &graph,
            CommandFormat::Json,
        );
        assert_eq!(out.exit_code, 0, "{}: {}", command.export, out.stderr);
        let payload: Value = serde_json::from_str(&out.stdout)
            .unwrap_or_else(|e| panic!("{}: stdout is not JSON ({e})", command.export));
        assert!(payload.is_object(), "{}: {payload}", command.export);
        let human = run(
            &runtime,
            &command.export,
            args,
            &graph,
            CommandFormat::Human,
        );
        assert_eq!(human.exit_code, 0, "{}: {}", command.export, human.stderr);
        assert!(!human.stdout.is_empty(), "{}", command.export);
    }
}

#[specforge_test(
    behavior = "pe_declare_surface_contributions",
    verify = "the dependency commands answer a 5000-long depends_on chain through the component"
)]
fn a_five_thousand_long_dependency_chain_answers_through_the_component() {
    // Each kind a 5000-long chain, plus a hub every module depends on.
    let mut nodes = vec![node("hub", "module")];
    let mut edges = Vec::new();
    for kind in ["feature", "milestone", "module"] {
        for i in 0..5000 {
            let id = format!("{kind}{i:04}");
            nodes.push(node(&id, kind));
            if i > 0 {
                let target = format!("{kind}{:04}", i - 1);
                edges.push(json!({"source": id, "target": target, "label": "depends_on"}));
            }
            if kind == "module" {
                edges.push(json!({"source": id, "target": "hub", "label": "depends_on"}));
            }
        }
    }
    let graph = json!({"nodes": nodes, "edges": edges});
    let runtime = runtime();
    let json_of = |export: &str, args: Value| -> Value {
        let args = args.as_object().cloned().unwrap();
        let out = run(&runtime, export, args, &graph, CommandFormat::Json);
        assert_eq!(out.exit_code, 0, "{export}: {}", out.stderr);
        serde_json::from_str(&out.stdout).unwrap()
    };
    let order = json_of("cmd__product_feature_ordering", json!({}));
    assert_eq!(order["sorted_features"][0], "feature0000");
    assert_eq!(order["sorted_features"][4999], "feature4999");
    let path = json_of("cmd__product_critical_path", json!({}));
    assert_eq!(path["path_length"], 5000);
    let depth = json_of("cmd__product_module_depth", json!({"module": "module4999"}));
    assert_eq!(depth["depth"], 5000);
    let coupling = json_of("cmd__product_module_coupling", json!({}));
    assert_eq!(coupling["most_coupled_id"], "hub");
    assert_eq!(coupling["modules"][0]["fan_in"], 5000);
}
