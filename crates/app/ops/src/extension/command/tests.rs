use super::*;
use serde_json::json;
use specforge_common::{SourceSpan, Sym};
use specforge_extension_sdk::prelude::*;
use specforge_graph::{Graph, Node};
use specforge_parser::{EntityId, EntityKind, FieldMap};
use specforge_test_macros::test as specforge_test;
use specforge_wasm::runtime::WasmCallResult;
use specforge_wasm::testing::InProcessRuntime;

const EXT: &str = "@acme/x";

/// `@acme/x`, whose guest answers `cmd__ok` with a command output after
/// reading its input as the SDK's `CommandInput`, and panics in
/// `cmd__boom`; it routes no other export.
fn fake() -> InProcessRuntime {
    fn guest(export: &str, input: &[u8]) -> Option<Result<Vec<u8>, String>> {
        match export {
            "cmd__ok" => Some(
                serde_json::from_slice::<specforge_extension_sdk::CommandInput>(input)
                    .map_err(|e| e.to_string())
                    .map(|_| {
                        CommandOutput {
                            exit_code: 3,
                            stdout: "out\n".into(),
                            stderr: "err\n".into(),
                        }
                        .to_bytes()
                    }),
            ),
            "cmd__boom" => panic!("the command panicked"),
            _ => None,
        }
    }
    InProcessRuntime::new().with_handler(
        || ContributionsBuilder::new(ExtensionMeta::new(EXT, "1.0.0")),
        guest,
    )
}

/// The input the last call received, as JSON.
fn last_input(runtime: &InProcessRuntime) -> Value {
    runtime.calls().last().expect("a call").input.clone()
}

fn graph() -> Graph {
    let mut graph = Graph::new();
    graph.add_node(Node {
        id: EntityId {
            raw: Sym::new("f1"),
        },
        kind: EntityKind {
            raw: Sym::new("feature"),
        },
        title: Some("One".into()),
        fields: FieldMap::new(),
        source_span: SourceSpan {
            file: Sym::new("main.spec"),
            start_line: 1,
            start_col: 1,
            end_line: 1,
            end_col: 2,
        },
        methods: Vec::new(),
        obligations: Vec::new(),
    });
    graph
}

fn command(id: &str) -> CommandDescriptor {
    CommandDescriptor {
        id: id.into(),
        title: id.into(),
        description: String::new(),
        category: None,
        export: format!("cmd__{id}"),
        args: Vec::new(),
    }
}

/// `@acme/x`'s command `id`, declared with no arg.
fn routed(id: &str) -> ExtensionCommand {
    ExtensionCommand::new(EXT, "x", &command(id))
}

/// A command declared as JSON (a `CommandDescriptor`), of `@acme/x`.
fn declared(declaration: Value) -> ExtensionCommand {
    ExtensionCommand::new(EXT, "x", &serde_json::from_value(declaration).unwrap())
}

/// `ordered`: an enum arg with a declared default, a flag, a count with
/// a description (R2).
fn ordered() -> ExtensionCommand {
    declared(
        json!({"id": "ordered", "title": "Ordered", "description": "List in order",
        "export": "cmd__ordered",
        "args": [{"name": "order", "arg_type": {"enum": {"values": ["asc", "desc"]}},
                  "default_value": "desc"},
                 {"name": "all", "arg_type": "bool"},
                 {"name": "limit", "arg_type": "integer", "minimum": 0,
                  "default_value": "10", "description": "At most this many"},
                 {"name": "milestone", "arg_type": "string", "required": true}]}),
    )
}

/// `strict`: a required flag (R3).
fn strict() -> ExtensionCommand {
    declared(
        json!({"id": "strict", "title": "Strict", "description": "Check strictly",
        "export": "cmd__strict",
        "args": [{"name": "strict", "arg_type": "bool", "required": true}]}),
    )
}

fn arg(name: &str, arg_type: &str) -> Value {
    json!({"name": name, "arg_type": arg_type})
}

/// A build of `declarations` (`(name, short, command ids)`), in order.
fn build_of(declarations: &[(&str, Option<&str>, &[&str])]) -> RegistryBuild {
    let declarations = declarations
        .iter()
        .map(
            |(name, short, ids)| specforge_protocol_types::ExtensionDeclaration {
                handshake: specforge_protocol_types::HandshakeResponse {
                    name: (*name).into(),
                    version: "1.0.0".into(),
                    ext_short: short.map(str::to_string),
                    ..Default::default()
                },
                surfaces: specforge_protocol_types::SurfaceDescriptor {
                    commands: ids.iter().map(|id| command(id)).collect(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .collect();
    specforge_registry::build_registries(declarations)
}

/// The `listing` command: three declared args of three types, routed to
/// `fake()`'s `cmd__ok`.
fn listing() -> ExtensionCommand {
    declared(
        json!({"id": "listing", "title": "Listing", "description": "", "export": "cmd__ok",
        "args": [{"name": "status", "arg_type": "string"},
                 {"name": "limit", "arg_type": "integer"},
                 {"name": "all", "arg_type": "bool"}]}),
    )
}

/// `ordered`, routed to `fake()`'s `cmd__ok`.
fn ordered_ok() -> ExtensionCommand {
    declared(
        json!({"id": "ordered", "title": "Ordered", "description": "List in order",
        "export": "cmd__ok",
        "args": [{"name": "order", "arg_type": {"enum": {"values": ["asc", "desc"]}},
                  "default_value": "desc"},
                 {"name": "all", "arg_type": "bool"},
                 {"name": "limit", "arg_type": "integer", "minimum": 0,
                  "default_value": "10", "description": "At most this many"},
                 {"name": "milestone", "arg_type": "string", "required": true}]}),
    )
}

/// A fixture project whose graph is [`graph`].
fn fixture(runtime: &std::sync::Arc<InProcessRuntime>) -> crate::view::testing::Fixture {
    let mut fixture = crate::view::testing::Fixture::new().runtime(runtime.clone());
    fixture.graph = graph();
    fixture
}

/// The root a command is told for the fixture: its temp directory,
/// canonical.
fn canonical_root(fixture: &crate::view::testing::Fixture) -> String {
    std::fs::canonicalize(fixture.dir.path())
        .unwrap()
        .display()
        .to_string()
}

mod derive;
mod run;
