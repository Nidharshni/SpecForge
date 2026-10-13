//! Registries the export tests share.

use specforge_emitter::{EmitFormat, EmitOptions, EmitterError};
use specforge_graph::Graph;
use specforge_registry::{FieldRegistry, FieldRegistryEntry};

/// The graph as a plain (format 1.0) export against the empty schema:
/// `emit` with the default options.
pub fn emit_json(graph: &Graph) -> String {
    specforge_emitter::emit(graph, &EmitOptions::default())
        .expect("graph serialization cannot fail")
}

/// `graph` scoped to `scope` in the graph format, as `specforge export
/// --scope` asks `emit` for it.
pub fn scoped_json(graph: &Graph, scope: &str) -> Result<String, EmitterError> {
    specforge_emitter::emit(
        graph,
        &EmitOptions {
            scope: Some(scope),
            ..Default::default()
        },
    )
}

/// `graph` scoped to `scope` in the context format.
pub fn scoped_context(graph: &Graph, scope: &str) -> Result<String, EmitterError> {
    specforge_emitter::emit(
        graph,
        &EmitOptions {
            format: EmitFormat::Context,
            scope: Some(scope),
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

/// A field registry in which each of `kinds` declares `contract` and
/// `status` as headline fields, as `@specforge/software` declares them on
/// `behavior`.
pub fn headline_registry(kinds: &[&str]) -> FieldRegistry {
    let mut registry = FieldRegistry::new();
    for (kind, field) in kinds
        .iter()
        .flat_map(|kind| ["contract", "status"].map(|field| (*kind, field)))
    {
        registry.register(
            FieldRegistryEntry::new(
                kind,
                "@test/ext",
                specforge_registry::FieldDescriptor {
                    name: field.to_string(),
                    field_type: "string".to_string(),
                    normative: field == "contract",
                    headline: true,
                    ..Default::default()
                },
            )
            .unwrap(),
        );
    }
    registry
}
