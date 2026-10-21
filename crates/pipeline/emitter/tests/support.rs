//! Registry builds the export tests share.

use specforge_emitter::{EmitFormat, EmitOptions, EmitterError};
use specforge_graph::Graph;
use specforge_protocol_types::{
    EdgeTypeDescriptor, EntityKindDescriptor, ExtensionDeclaration, FieldDescriptor,
};
use specforge_registry::{RegistryBuild, build_registries};

/// The graph as a plain (format 1.0) export against the empty schema:
/// `emit` with the default options.
pub fn emit_json(graph: &Graph) -> String {
    specforge_emitter::emit(graph, &EmitOptions::default())
        .expect("graph serialization cannot fail")
}

/// The selection of `scope` and what it reaches.
fn select(graph: &Graph, scope: &str) -> specforge_graph::Selected {
    graph
        .select(Some(scope), None, &[])
        .expect("tests scope at an entity of the graph")
}

/// `graph` scoped to `scope` in the graph format, as `specforge export
/// --scope` asks `emit` for it.
pub fn scoped_json(graph: &Graph, scope: &str) -> Result<String, EmitterError> {
    let selected = select(graph, scope);
    specforge_emitter::emit(
        graph,
        &EmitOptions {
            selected: Some(&selected),
            ..Default::default()
        },
    )
}

/// `graph` scoped to `scope` in the context format.
pub fn scoped_context(graph: &Graph, scope: &str) -> Result<String, EmitterError> {
    let selected = select(graph, scope);
    specforge_emitter::emit(
        graph,
        &EmitOptions {
            format: EmitFormat::Context,
            selected: Some(&selected),
            ..Default::default()
        },
    )
}

/// The graph export under `max_tokens`, without a schema: the call `specforge
/// export --max-tokens` makes.
pub fn budgeted_json(graph: &Graph, max_tokens: usize) -> String {
    specforge_emitter::emit(
        graph,
        &EmitOptions {
            token_budget: Some(max_tokens),
            ..Default::default()
        },
    )
    .unwrap()
}

/// The extension `name` (version 1.0.0) declaring `entities` and `edges`.
pub fn declaration(
    name: &str,
    entities: Vec<EntityKindDescriptor>,
    edges: Vec<EdgeTypeDescriptor>,
) -> ExtensionDeclaration {
    let mut declaration = ExtensionDeclaration::default();
    declaration.handshake.name = name.to_string();
    declaration.handshake.version = "1.0.0".to_string();
    declaration.entities = entities;
    declaration.edges = edges;
    declaration
}

/// A kind declaration: `name` is its keyword.
pub fn kind(name: &str, fields: Vec<FieldDescriptor>) -> EntityKindDescriptor {
    EntityKindDescriptor {
        name: name.to_string(),
        keyword: Some(name.to_string()),
        fields,
        ..Default::default()
    }
}

/// A string field declaration.
pub fn field(name: &str, normative: bool, headline: bool) -> FieldDescriptor {
    FieldDescriptor {
        name: name.to_string(),
        field_type: "string".to_string(),
        normative,
        headline,
        ..Default::default()
    }
}

/// The Registry build of one extension, `@test/ext`, declaring `kinds`.
pub fn build_of(kinds: Vec<EntityKindDescriptor>) -> RegistryBuild {
    build_registries(vec![declaration("@test/ext", kinds, vec![])])
}

/// A build in which each of `kinds` declares `contract` and `status` as
/// headline fields, as `@specforge/software` declares them on `behavior`.
pub fn headline_build(kinds: &[&str]) -> RegistryBuild {
    build_of(
        kinds
            .iter()
            .map(|name| {
                kind(
                    name,
                    vec![field("contract", true, true), field("status", false, true)],
                )
            })
            .collect(),
    )
}
